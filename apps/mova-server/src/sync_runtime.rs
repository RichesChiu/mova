use crate::{
    realtime::RealtimeEvent,
    state::{AppState, RegisterScanError},
};
use mova_domain::Library;
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::{
    sync::{mpsc, watch},
    time::{Duration, Instant},
};

const WATCH_DEBOUNCE_WINDOW: Duration = Duration::from_secs(2);
const PERIODIC_RECONCILE_INTERVAL: Duration = Duration::from_secs(300);

/// 为所有已启用媒体库启动 watcher 和定时校准任务。
pub async fn initialize_library_sync(state: &AppState) {
    match mova_application::list_libraries(&state.db).await {
        Ok(libraries) => {
            for library in libraries {
                if library.is_enabled {
                    start_library_watcher(state, library).await;
                }
            }
        }
        Err(error) => {
            tracing::error!(error = ?error, "failed to initialize library watchers");
        }
    }
}

/// 在新建媒体库后启动 watcher 和定时校准任务。
pub async fn start_library_watcher(state: &AppState, library: Library) {
    if !library.is_enabled {
        state.library_sync_registry.stop_watcher(library.id);
        return;
    }

    let (stop_tx, stop_rx) = watch::channel(false);
    if let Some(previous_stop_tx) = state
        .library_sync_registry
        .replace_watcher(library.id, stop_tx)
    {
        let _ = previous_stop_tx.send(true);
    }

    let periodic_state = state.clone();
    let periodic_stop_rx = stop_rx.clone();
    tokio::spawn(async move {
        run_periodic_library_reconcile(periodic_state, library.id, periodic_stop_rx).await;
    });

    let (event_tx, event_rx) = mpsc::unbounded_channel::<notify::Result<Event>>();
    let mut watcher = match RecommendedWatcher::new(
        move |result| {
            let _ = event_tx.send(result);
        },
        Config::default(),
    ) {
        Ok(watcher) => watcher,
        Err(error) => {
            tracing::error!(library_id = library.id, error = ?error, "failed to create library watcher");
            return;
        }
    };

    if let Err(error) = watcher.watch(Path::new(&library.root_path), RecursiveMode::Recursive) {
        tracing::error!(
            library_id = library.id,
            root_path = %library.root_path,
            error = ?error,
            "failed to watch library root path"
        );
        return;
    }

    let state = state.clone();
    tokio::spawn(async move {
        run_library_watcher(state, library.id, event_rx, stop_rx, watcher).await;
    });
}

/// 新建并启用媒体库时自动触发第一次后台扫描。
pub async fn maybe_enqueue_initial_library_scan(
    state: &AppState,
    library_id: i64,
    is_enabled: bool,
) {
    if !is_enabled {
        return;
    }

    enqueue_background_scan(state, library_id, "initial library scan").await;
}

/// 供手动扫描入口和访问兜底复用的后台扫描入队逻辑。
pub async fn enqueue_background_scan(state: &AppState, library_id: i64, reason: &'static str) {
    match mova_application::enqueue_library_scan(&state.db, library_id).await {
        Ok(enqueue_result) if enqueue_result.created => {
            if let Err(error) =
                spawn_library_scan_job(state, library_id, enqueue_result.scan_job.id)
            {
                handle_scan_registration_rejected(
                    state,
                    library_id,
                    enqueue_result.scan_job.id,
                    error,
                )
                .await;
            }
        }
        Ok(_) => {}
        Err(error) => {
            tracing::error!(
                library_id,
                error = ?error,
                reason,
                "failed to enqueue background library scan"
            );
        }
    }
}

pub fn spawn_library_scan_job(
    state: &AppState,
    library_id: i64,
    scan_job_id: i64,
) -> Result<(), RegisterScanError> {
    let active_scan = state.scan_registry.register_scan(library_id, scan_job_id)?;
    let db = state.db.clone();
    let artwork_cache_dir = state.artwork_cache_dir.clone();
    let metadata_provider = state.metadata_provider.clone();
    let scan_registry = state.scan_registry.clone();
    let library_sync_registry = state.library_sync_registry.clone();
    let realtime_hub = state.realtime_hub.clone();
    let cancellation_flag = active_scan.cancellation_flag();
    let scan_event_listener: Arc<dyn Fn(mova_application::ScanJobEvent) + Send + Sync> =
        Arc::new(move |event| match event {
            mova_application::ScanJobEvent::Updated(scan_job) => {
                realtime_hub.publish(RealtimeEvent::ScanJobUpdated { scan_job });
            }
            mova_application::ScanJobEvent::Finished(scan_job) => {
                realtime_hub.publish(RealtimeEvent::ScanJobFinished { scan_job });
            }
        });

    tokio::spawn(async move {
        let result = mova_application::execute_scan_job_with_cancellation(
            &db,
            library_id,
            scan_job_id,
            cancellation_flag,
            artwork_cache_dir,
            metadata_provider,
            scan_event_listener,
        )
        .await;

        match result {
            Ok(mova_application::ExecuteScanJobOutcome::Completed(_)) => {
                library_sync_registry.record_reconciled(library_id);
            }
            Ok(mova_application::ExecuteScanJobOutcome::Cancelled) => {
                tracing::info!(library_id, scan_job_id, "background scan job cancelled");
            }
            Err(error) => {
                library_sync_registry.mark_dirty(library_id);
                tracing::error!(
                    library_id,
                    scan_job_id,
                    error = ?error,
                    "background scan job failed"
                );
            }
        }

        scan_registry.finish_scan(library_id, scan_job_id);
    });

    Ok(())
}

pub async fn handle_scan_registration_rejected(
    state: &AppState,
    library_id: i64,
    scan_job_id: i64,
    error: RegisterScanError,
) {
    match error {
        RegisterScanError::DeleteInProgress => {
            match mova_db::finalize_scan_job(
                &state.db,
                scan_job_id,
                "failed",
                0,
                0,
                Some("scan cancelled because library is being deleted"),
            )
            .await
            {
                Ok(Some(scan_job)) => {
                    state
                        .realtime_hub
                        .publish(RealtimeEvent::ScanJobFinished { scan_job });
                }
                Ok(None) => {}
                Err(finalize_error) => {
                    tracing::warn!(
                        library_id,
                        scan_job_id,
                        error = ?finalize_error,
                        "failed to finalize scan job rejected during library deletion"
                    );
                }
            }
        }
    }
}

async fn run_library_watcher(
    state: AppState,
    library_id: i64,
    mut event_rx: mpsc::UnboundedReceiver<notify::Result<Event>>,
    mut stop_rx: watch::Receiver<bool>,
    watcher: RecommendedWatcher,
) {
    let _watcher = watcher;

    loop {
        let mut pending_paths = HashSet::<PathBuf>::new();

        tokio::select! {
            changed = stop_rx.changed() => {
                if changed.is_ok() && *stop_rx.borrow() {
                    break;
                }
                continue;
            }
            maybe_event = event_rx.recv() => {
                let Some(event) = maybe_event else {
                    break;
                };
                match event {
                    Ok(event) => pending_paths.extend(event.paths),
                    Err(error) => {
                        state.library_sync_registry.mark_dirty(library_id);
                        tracing::warn!(library_id, error = ?error, "library watcher received an error");
                        continue;
                    }
                }
            }
        }

        if pending_paths.is_empty() {
            continue;
        }

        let sleep = tokio::time::sleep(WATCH_DEBOUNCE_WINDOW);
        tokio::pin!(sleep);

        loop {
            tokio::select! {
                _ = &mut sleep => {
                    break;
                }
                changed = stop_rx.changed() => {
                    if changed.is_ok() && *stop_rx.borrow() {
                        return;
                    }
                }
                maybe_event = event_rx.recv() => {
                    let Some(event) = maybe_event else {
                        break;
                    };

                    match event {
                        Ok(event) => {
                            pending_paths.extend(event.paths);
                            sleep.as_mut().reset(Instant::now() + WATCH_DEBOUNCE_WINDOW);
                        }
                        Err(error) => {
                            state.library_sync_registry.mark_dirty(library_id);
                            tracing::warn!(library_id, error = ?error, "library watcher received an error during debounce");
                        }
                    }
                }
            }
        }

        process_watch_batch(&state, library_id, pending_paths).await;
    }
}

async fn run_periodic_library_reconcile(
    state: AppState,
    library_id: i64,
    mut stop_rx: watch::Receiver<bool>,
) {
    let sleep = tokio::time::sleep(PERIODIC_RECONCILE_INTERVAL);
    tokio::pin!(sleep);

    loop {
        tokio::select! {
            _ = &mut sleep => {
                process_periodic_reconcile(&state, library_id).await;
                sleep.as_mut().reset(Instant::now() + PERIODIC_RECONCILE_INTERVAL);
            }
            changed = stop_rx.changed() => {
                if changed.is_ok() && *stop_rx.borrow() {
                    return;
                }
            }
        }
    }
}

async fn process_watch_batch(state: &AppState, library_id: i64, pending_paths: HashSet<PathBuf>) {
    if state.scan_registry.is_deleting(library_id) {
        return;
    }

    if state.scan_registry.active_scan(library_id).is_some() {
        return;
    }

    if !state.library_sync_registry.begin_sync(library_id) {
        return;
    }

    let (existing_paths, removed_paths): (Vec<PathBuf>, Vec<PathBuf>) =
        pending_paths.into_iter().partition(|path| path.exists());

    if existing_paths.is_empty() && removed_paths.is_empty() {
        return;
    }

    match mova_application::sync_library_filesystem_changes(
        &state.db,
        library_id,
        existing_paths,
        removed_paths,
        state.artwork_cache_dir.clone(),
        state.metadata_provider.clone(),
    )
    .await
    {
        Ok(()) => {
            state.library_sync_registry.finish_sync(library_id, true);
        }
        Err(error) => {
            state.library_sync_registry.finish_sync(library_id, false);
            tracing::error!(
                library_id,
                error = ?error,
                "failed to apply watcher-triggered library sync"
            );
        }
    }
}

async fn process_periodic_reconcile(state: &AppState, library_id: i64) {
    if state.scan_registry.is_deleting(library_id) {
        return;
    }

    if state.scan_registry.active_scan(library_id).is_some() {
        return;
    }

    if !state.library_sync_registry.begin_sync(library_id) {
        return;
    }

    match mova_application::reconcile_library_inventory(
        &state.db,
        library_id,
        state.artwork_cache_dir.clone(),
        state.metadata_provider.clone(),
    )
    .await
    {
        Ok(()) => {
            state.library_sync_registry.finish_sync(library_id, true);
        }
        Err(error) => {
            state.library_sync_registry.finish_sync(library_id, false);
            tracing::error!(
                library_id,
                error = ?error,
                "failed to run periodic library inventory reconcile"
            );
        }
    }
}
