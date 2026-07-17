use crate::state::{AppState, RegisterScanError};
use mova_application::{ExecuteScanJobOutcome, ScanJobEvent, ScanJobProgressUpdate};
use serde::Deserialize;
use std::{sync::Arc, time::Duration};
use tokio::sync::watch;
use uuid::Uuid;

const BACKGROUND_JOB_LEASE_SECONDS: i64 = 60;
const BACKGROUND_JOB_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(15);
const BACKGROUND_JOB_LEASE_SAFETY_WINDOW: Duration = Duration::from_secs(45);
const BACKGROUND_JOB_IDLE_POLL_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Debug, Deserialize)]
struct LibraryScanJobPayload {
    library_id: i64,
    scan_job_id: i64,
}

pub fn start_background_workers(state: AppState, concurrency: usize) {
    for worker_index in 0..concurrency.max(1) {
        let worker_state = state.clone();
        let worker_id = format!(
            "mova-{}-{}-{}",
            std::process::id(),
            worker_index,
            Uuid::new_v4()
        );
        tokio::spawn(async move {
            run_background_worker(worker_state, worker_id).await;
        });
    }
}

async fn run_background_worker(state: AppState, worker_id: String) {
    loop {
        let job = match mova_db::claim_background_job(
            &state.db,
            &worker_id,
            BACKGROUND_JOB_LEASE_SECONDS,
        )
        .await
        {
            Ok(job) => job,
            Err(error) => {
                tracing::warn!(worker_id, error = ?error, "background worker failed to claim a job");
                wait_for_work(&state).await;
                continue;
            }
        };

        let Some(job) = job else {
            wait_for_work(&state).await;
            continue;
        };

        let result = match job.job_type.as_str() {
            "library.scan" => execute_library_scan_background_job(&state, &worker_id, &job).await,
            unsupported => Err(anyhow::anyhow!(
                "unsupported background job type: {unsupported}"
            )),
        };

        match result {
            Ok(()) => {
                if let Err(error) =
                    mova_db::complete_background_job(&state.db, job.id, &worker_id).await
                {
                    tracing::warn!(job_id = job.id, error = ?error, "failed to complete background job");
                }
            }
            Err(error) => {
                let retry_delay_seconds = i64::from(job.attempt_count.max(1)).pow(2) * 2;
                let error_message = error.to_string();
                match mova_db::retry_or_fail_background_job(
                    &state.db,
                    job.id,
                    &worker_id,
                    &error_message,
                    retry_delay_seconds,
                )
                .await
                {
                    Ok(Some(status)) => {
                        tracing::warn!(
                            job_id = job.id,
                            attempt = job.attempt_count,
                            %status,
                            error = ?error,
                            "background job execution failed"
                        );
                        if let Some(scan_job_id) = job.related_scan_job_id {
                            if status == "pending" {
                                match mova_db::mark_scan_job_retry_pending(
                                    &state.db,
                                    scan_job_id,
                                    &error_message,
                                )
                                .await
                                {
                                    Ok(Some(scan_job)) => {
                                        state.realtime_dispatcher.publish_scan_event(
                                            ScanJobEvent::Updated(ScanJobProgressUpdate {
                                                scan_job,
                                                phase: None,
                                            }),
                                        )
                                    }
                                    Ok(None) => {}
                                    Err(update_error) => tracing::warn!(
                                        scan_job_id,
                                        error = ?update_error,
                                        "failed to mark scan job as pending for retry"
                                    ),
                                }
                            } else if status == "failed" {
                                match mova_db::get_scan_job(&state.db, scan_job_id).await {
                                    Ok(Some(scan_job)) => {
                                        match mova_db::finalize_scan_job(
                                            &state.db,
                                            scan_job_id,
                                            "failed",
                                            scan_job.total_files,
                                            scan_job.scanned_files,
                                            Some(&error_message),
                                            None,
                                        )
                                        .await
                                        {
                                            Ok(Some(scan_job)) => {
                                                state.realtime_dispatcher.publish_scan_event(
                                                    ScanJobEvent::Finished(ScanJobProgressUpdate {
                                                        scan_job,
                                                        phase: Some("finished".to_string()),
                                                    }),
                                                )
                                            }
                                            Ok(None) => {}
                                            Err(update_error) => tracing::warn!(
                                                scan_job_id,
                                                error = ?update_error,
                                                "failed to finalize exhausted scan job"
                                            ),
                                        }
                                    }
                                    Ok(None) => {}
                                    Err(update_error) => tracing::warn!(
                                        scan_job_id,
                                        error = ?update_error,
                                        "failed to load exhausted scan job"
                                    ),
                                }
                            }
                        }
                    }
                    Ok(None) => {}
                    Err(update_error) => {
                        tracing::error!(
                            job_id = job.id,
                            error = ?update_error,
                            "failed to persist background job failure"
                        );
                    }
                }
            }
        }
    }
}

async fn execute_library_scan_background_job(
    state: &AppState,
    worker_id: &str,
    job: &mova_db::BackgroundJob,
) -> anyhow::Result<()> {
    let payload: LibraryScanJobPayload = serde_json::from_str(&job.payload_json)?;
    let active_scan = match state
        .scan_registry
        .register_scan(payload.library_id, payload.scan_job_id)
    {
        Ok(active_scan) => active_scan,
        Err(RegisterScanError::DeleteInProgress) => {
            return Err(anyhow::anyhow!(
                "library {} is currently being deleted",
                payload.library_id
            ));
        }
        Err(RegisterScanError::AlreadyRunning) => {
            return Err(anyhow::anyhow!(
                "library {} already has an active scan runtime",
                payload.library_id
            ));
        }
    };

    let cancellation_flag = active_scan.cancellation_flag();

    let (stop_heartbeat, heartbeat_stop) = watch::channel(false);
    let heartbeat_pool = state.db.clone();
    let heartbeat_worker_id = worker_id.to_string();
    let heartbeat_job_id = job.id;
    let heartbeat_cancellation_flag = cancellation_flag.clone();
    let heartbeat = tokio::spawn(async move {
        run_lease_heartbeat(
            heartbeat_pool,
            heartbeat_job_id,
            heartbeat_worker_id,
            heartbeat_stop,
            heartbeat_cancellation_flag,
        )
        .await;
    });

    let dispatcher = state.realtime_dispatcher.clone();
    let event_listener: Arc<dyn Fn(ScanJobEvent) + Send + Sync> =
        Arc::new(move |event| dispatcher.publish_scan_event(event));
    let result = mova_application::execute_scan_job_with_cancellation(
        &state.db,
        payload.library_id,
        payload.scan_job_id,
        cancellation_flag,
        state.artwork_cache_dir.clone(),
        state.metadata_provider.clone(),
        event_listener,
    )
    .await;

    let _ = stop_heartbeat.send(true);
    let _ = heartbeat.await;
    state
        .scan_registry
        .finish_scan(payload.library_id, payload.scan_job_id);

    match result? {
        ExecuteScanJobOutcome::Completed(_) | ExecuteScanJobOutcome::Cancelled => Ok(()),
    }
}

async fn run_lease_heartbeat(
    pool: sqlx::PgPool,
    job_id: i64,
    worker_id: String,
    mut stop: watch::Receiver<bool>,
    cancellation_flag: Arc<std::sync::atomic::AtomicBool>,
) {
    let mut interval = tokio::time::interval(BACKGROUND_JOB_HEARTBEAT_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut last_successful_renewal = tokio::time::Instant::now();

    loop {
        tokio::select! {
            changed = stop.changed() => {
                if changed.is_err() || *stop.borrow() {
                    break;
                }
            }
            _ = interval.tick() => {
                match mova_db::renew_background_job_lease(
                    &pool,
                    job_id,
                    &worker_id,
                    BACKGROUND_JOB_LEASE_SECONDS,
                ).await {
                    Ok(true) => {
                        last_successful_renewal = tokio::time::Instant::now();
                    }
                    Ok(false) => {
                        tracing::warn!(job_id, "background job lease is no longer owned by this worker");
                        cancellation_flag.store(true, std::sync::atomic::Ordering::SeqCst);
                        break;
                    }
                    Err(error) => {
                        tracing::warn!(job_id, error = ?error, "failed to renew background job lease");
                        if last_successful_renewal.elapsed() >= BACKGROUND_JOB_LEASE_SAFETY_WINDOW {
                            tracing::warn!(job_id, "cancelling background job before its database lease can expire");
                            cancellation_flag.store(true, std::sync::atomic::Ordering::SeqCst);
                            break;
                        }
                    }
                }
            }
        }
    }
}

async fn wait_for_work(state: &AppState) {
    tokio::select! {
        _ = state.background_jobs.wait() => {}
        _ = tokio::time::sleep(BACKGROUND_JOB_IDLE_POLL_INTERVAL) => {}
    }
}
