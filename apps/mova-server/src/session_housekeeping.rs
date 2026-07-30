use sqlx::PgPool;
use std::time::Duration;
use tokio::time::{Instant, MissedTickBehavior};

const CLEANUP_INTERVAL: Duration = Duration::from_secs(15 * 60);
const CLEANUP_BATCH_SIZE: i64 = 1_000;
const REVOKED_SESSION_RETENTION_SECONDS: i64 = 7 * 24 * 60 * 60;
const MAX_BATCHES_PER_RUN: usize = 32;

pub fn start_auth_session_housekeeping(pool: PgPool) {
    tokio::spawn(async move {
        run_cleanup(&pool).await;

        let mut interval =
            tokio::time::interval_at(Instant::now() + CLEANUP_INTERVAL, CLEANUP_INTERVAL);
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            run_cleanup(&pool).await;
        }
    });
}

async fn run_cleanup(pool: &PgPool) {
    let mut totals = mova_db::AuthSessionCleanupOutcome::default();

    for _ in 0..MAX_BATCHES_PER_RUN {
        let outcome = match mova_db::cleanup_auth_sessions(
            pool,
            CLEANUP_BATCH_SIZE,
            REVOKED_SESSION_RETENTION_SECONDS,
        )
        .await
        {
            Ok(outcome) => outcome,
            Err(error) => {
                tracing::warn!(error = ?error, "auth session housekeeping failed");
                return;
            }
        };

        if !outcome.lock_acquired {
            return;
        }

        totals.lock_acquired = true;
        totals.deleted_user_sessions += outcome.deleted_user_sessions;
        totals.deleted_native_sessions += outcome.deleted_native_sessions;
        totals.deleted_used_refresh_tokens += outcome.deleted_used_refresh_tokens;

        if !outcome.reached_batch_limit(CLEANUP_BATCH_SIZE) {
            break;
        }
    }

    if totals.deleted_user_sessions > 0
        || totals.deleted_native_sessions > 0
        || totals.deleted_used_refresh_tokens > 0
    {
        tracing::info!(
            deleted_user_sessions = totals.deleted_user_sessions,
            deleted_native_sessions = totals.deleted_native_sessions,
            deleted_used_refresh_tokens = totals.deleted_used_refresh_tokens,
            "auth session housekeeping completed"
        );
    }
}
