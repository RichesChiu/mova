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

#[derive(Debug, Deserialize)]
struct LibraryCacheCleanupJobPayload {
    library_id: i64,
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
        let claim_outcome = match mova_db::claim_background_job(
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

        publish_abandoned_background_job_outcomes(&state, claim_outcome.terminalized_jobs);

        let Some(job) = claim_outcome.claimed_job else {
            wait_for_work(&state).await;
            continue;
        };
        let fence = match job.execution_fence() {
            Ok(fence) => fence,
            Err(error) => {
                tracing::error!(
                    job_id = job.id,
                    error = ?error,
                    "claimed background job did not provide a valid execution fence"
                );
                continue;
            }
        };

        let result = match job.job_type.as_str() {
            "library.scan" => execute_library_scan_background_job(&state, &job, &fence).await,
            "library.cache.cleanup" => {
                execute_library_cache_cleanup_background_job(&state, &job, &fence).await
            }
            unsupported => Err(anyhow::anyhow!(
                "unsupported background job type: {unsupported}"
            )),
        };

        match result {
            Ok(()) if job.job_type == "library.scan" => {
                tracing::info!(
                    job_id = job.id,
                    job_type = job.job_type,
                    "library scan and background job completed atomically"
                );
            }
            Ok(()) => match mova_db::complete_background_job(&state.db, &fence).await {
                Ok(status) => tracing::info!(
                    job_id = job.id,
                    job_type = job.job_type,
                    %status,
                    "background job completed"
                ),
                Err(error) => {
                    tracing::warn!(job_id = job.id, error = ?error, "failed to complete background job");
                }
            },
            Err(error) => {
                let retry_delay_seconds = i64::from(job.attempt_count.max(1)).pow(2) * 2;
                let error_message = error.to_string();
                match mova_db::retry_or_fail_background_job(
                    &state.db,
                    &fence,
                    &error_message,
                    retry_delay_seconds,
                )
                .await
                {
                    Ok(Some(outcome)) => {
                        let status = outcome.status;
                        if status == "cancelled" {
                            tracing::info!(
                                job_id = job.id,
                                job_type = job.job_type,
                                "background job cancelled"
                            );
                        } else {
                            tracing::warn!(
                                job_id = job.id,
                                attempt = job.attempt_count,
                                scope_type = %job.scope_type,
                                scope_id = job.scope_id,
                                related_scan_job_id = ?job.related_scan_job_id,
                                %status,
                                error = ?error,
                                "background job execution failed"
                            );
                        }
                        if status == "failed" && job.job_type == "library.cache.cleanup" {
                            if let Err(notification_error) =
                                mova_db::persist_cache_cleanup_failure_notification(
                                    &state.db,
                                    &job,
                                    &error_message,
                                )
                                .await
                            {
                                tracing::error!(
                                    job_id = job.id,
                                    error = ?notification_error,
                                    "failed to persist cache cleanup failure notification"
                                );
                            }
                        }
                        if let Some(scan_job) = outcome.scan_job {
                            if status == "pending" {
                                state.realtime_dispatcher.publish_scan_event(
                                    ScanJobEvent::Updated(ScanJobProgressUpdate {
                                        scan_job,
                                        phase: None,
                                    }),
                                );
                            } else if status == "failed" || status == "cancelled" {
                                state.realtime_dispatcher.publish_scan_event(
                                    ScanJobEvent::Finished(ScanJobProgressUpdate {
                                        scan_job,
                                        phase: Some("finished".to_string()),
                                    }),
                                );
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

fn publish_abandoned_background_job_outcomes(
    state: &AppState,
    outcomes: Vec<mova_db::AbandonedBackgroundJobOutcome>,
) {
    for outcome in outcomes {
        let status = outcome.job.status.as_str();
        tracing::warn!(
            job_id = outcome.job.id,
            job_type = outcome.job.job_type,
            %status,
            "abandoned background job reached a terminal state"
        );

        if status == "failed" && outcome.job.job_type == "library.cache.cleanup" {
            let pool = state.db.clone();
            let job = outcome.job.clone();
            tokio::spawn(async move {
                if let Err(error) = mova_db::persist_cache_cleanup_failure_notification(
                    &pool,
                    &job,
                    mova_db::BACKGROUND_JOB_FINAL_ATTEMPT_LEASE_EXPIRED_MESSAGE,
                )
                .await
                {
                    tracing::error!(
                        job_id = job.id,
                        error = ?error,
                        "failed to persist abandoned cache cleanup failure notification"
                    );
                }
            });
        }

        if let Some(scan_job) = outcome.scan_job {
            state
                .realtime_dispatcher
                .publish_scan_event(ScanJobEvent::Finished(ScanJobProgressUpdate {
                    scan_job,
                    phase: Some("finished".to_string()),
                }));
        }
    }
}

async fn execute_library_cache_cleanup_background_job(
    state: &AppState,
    job: &mova_db::BackgroundJob,
    fence: &mova_db::BackgroundJobFence,
) -> anyhow::Result<()> {
    let payload: LibraryCacheCleanupJobPayload = serde_json::from_str(&job.payload_json)?;
    if job.scope_type != "library" || job.scope_id != payload.library_id || payload.library_id <= 0
    {
        return Err(anyhow::anyhow!(
            "invalid library cache cleanup job scope for job {}",
            job.id
        ));
    }

    let cancellation_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (stop_heartbeat, heartbeat_stop) = watch::channel(false);
    let heartbeat_pool = state.db.clone();
    let heartbeat_fence = fence.clone();
    let heartbeat_cancellation_flag = cancellation_flag.clone();
    let heartbeat = tokio::spawn(async move {
        run_lease_heartbeat(
            heartbeat_pool,
            heartbeat_fence,
            heartbeat_stop,
            heartbeat_cancellation_flag,
        )
        .await;
    });

    tracing::info!(
        job_id = job.id,
        library_id = payload.library_id,
        "removing deleted library cache namespace"
    );
    let result = mova_application::remove_library_cache(&state.cache_dir, payload.library_id).await;

    let _ = stop_heartbeat.send(true);
    let _ = heartbeat.await;
    result
}

async fn execute_library_scan_background_job(
    state: &AppState,
    job: &mova_db::BackgroundJob,
    fence: &mova_db::BackgroundJobFence,
) -> anyhow::Result<()> {
    let payload: LibraryScanJobPayload = serde_json::from_str(&job.payload_json)?;
    if job.scope_type != "library"
        || job.scope_id != payload.library_id
        || job.related_scan_job_id != Some(payload.scan_job_id)
        || payload.library_id <= 0
        || payload.scan_job_id <= 0
    {
        return Err(anyhow::anyhow!(
            "invalid library scan job scope or relation for job {}",
            job.id
        ));
    }
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
    let heartbeat_fence = fence.clone();
    let heartbeat_cancellation_flag = cancellation_flag.clone();
    let heartbeat = tokio::spawn(async move {
        run_lease_heartbeat(
            heartbeat_pool,
            heartbeat_fence,
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
        fence.clone(),
        cancellation_flag,
        state.cache_dir.clone(),
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
    fence: mova_db::BackgroundJobFence,
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
                    &fence,
                    BACKGROUND_JOB_LEASE_SECONDS,
                ).await {
                    Ok(true) => {
                        last_successful_renewal = tokio::time::Instant::now();
                    }
                    Ok(false) => {
                        tracing::warn!(job_id = fence.job_id, "background job lease is no longer owned by this worker");
                        cancellation_flag.store(true, std::sync::atomic::Ordering::SeqCst);
                        break;
                    }
                    Err(error) => {
                        tracing::warn!(job_id = fence.job_id, error = ?error, "failed to renew background job lease");
                        if last_successful_renewal.elapsed() >= BACKGROUND_JOB_LEASE_SAFETY_WINDOW {
                            tracing::warn!(job_id = fence.job_id, "cancelling background job before its database lease can expire");
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
