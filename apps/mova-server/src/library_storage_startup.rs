//! Startup storage check.
//!
//! Every bind mount comes from the deployment configuration, so a container
//! start is when folders appear or disappear. Each library is checked once at
//! startup: storage that is not connected marks it unavailable, and a library
//! whose every folder was removed from the deployment is deleted.

use crate::{handlers::libraries::delete_library_after_stopping_scan, state::AppState};
use mova_application::LibraryStorageCheck;

pub fn start_library_storage_check(state: AppState) {
    tokio::spawn(async move {
        if let Err(error) = check_all_libraries(&state).await {
            tracing::error!(error = ?error, "startup library storage check failed");
        }
    });
}

async fn check_all_libraries(state: &AppState) -> anyhow::Result<()> {
    let libraries =
        mova_application::list_libraries(&state.db, mova_domain::LibraryVisibility::All).await?;

    for library in libraries {
        let check = match mova_application::check_library_storage(
            &state.db,
            &library,
            state.storage_environment.as_ref(),
        )
        .await
        {
            Ok(check) => check,
            Err(error) => {
                tracing::warn!(
                    library_id = library.id,
                    error = ?error,
                    "failed to check library storage at startup"
                );
                continue;
            }
        };

        let LibraryStorageCheck::RemovedFromDeployment { mount_points } = check else {
            continue;
        };
        tracing::warn!(
            library_id = library.id,
            library_name = %library.name,
            mount_points = ?mount_points,
            "every folder of this library was removed from the deployment; deleting the library"
        );
        if let Err(error) = delete_library_after_stopping_scan(state, library.id).await {
            tracing::error!(
                library_id = library.id,
                error = ?error,
                "failed to delete a library removed from the deployment"
            );
            continue;
        }
        if let Err(error) = mova_db::record_library_removed_from_deployment(
            &state.db,
            library.id,
            &library.name,
            &mount_points,
        )
        .await
        {
            tracing::warn!(
                library_id = library.id,
                error = ?error,
                "failed to notify admins about a library removed from the deployment"
            );
        }
    }

    Ok(())
}
