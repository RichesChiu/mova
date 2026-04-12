use crate::{
    realtime::RealtimeEvent,
    state::{AppState, RegisterScanError},
};
use std::sync::Arc;

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
    let realtime_hub = state.realtime_hub.clone();
    let cancellation_flag = active_scan.cancellation_flag();
    let scan_event_listener: Arc<dyn Fn(mova_application::ScanJobEvent) + Send + Sync> =
        Arc::new(move |event| match event {
            mova_application::ScanJobEvent::Updated(update) => {
                realtime_hub.publish(RealtimeEvent::ScanJobUpdated { update });
            }
            mova_application::ScanJobEvent::Finished(update) => {
                realtime_hub.publish(RealtimeEvent::ScanJobFinished { update });
            }
            mova_application::ScanJobEvent::ItemUpdated(item) => {
                realtime_hub.publish(RealtimeEvent::ScanItemUpdated { item });
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

        if let Err(error) = result {
            tracing::error!(
                library_id,
                scan_job_id,
                error = ?error,
                "background scan job failed"
            );
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
                    state.realtime_hub.publish(RealtimeEvent::ScanJobFinished {
                        update: mova_application::ScanJobProgressUpdate {
                            scan_job,
                            phase: Some("finished".to_string()),
                        },
                    });
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
