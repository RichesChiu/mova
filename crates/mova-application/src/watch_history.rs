use crate::{
    error::{ApplicationError, ApplicationResult},
    media_items::{get_media_file, get_media_item},
};
use mova_domain::{WatchHistory, WatchHistoryItem};
use sqlx::postgres::PgPool;
use time::{Duration, OffsetDateTime};

const DEFAULT_WATCH_HISTORY_LIMIT: i64 = 50;
const MAX_WATCH_HISTORY_LIMIT: i64 = 200;
const WATCH_HISTORY_SESSION_GAP: Duration = Duration::hours(4);

#[derive(Debug, Clone)]
pub(crate) struct RecordWatchHistoryInput {
    pub media_file_id: i64,
    pub position_seconds: i32,
    pub duration_seconds: Option<i32>,
    pub is_finished: bool,
}

pub async fn list_watch_history(
    pool: &PgPool,
    user_id: i64,
    limit: Option<i64>,
) -> ApplicationResult<Vec<WatchHistoryItem>> {
    let limit = normalize_watch_history_limit(limit)?;

    mova_db::list_watch_history(pool, user_id, limit)
        .await
        .map_err(ApplicationError::from)
}

pub(crate) async fn record_watch_history_for_media_item(
    pool: &PgPool,
    user_id: i64,
    media_item_id: i64,
    input: RecordWatchHistoryInput,
) -> ApplicationResult<WatchHistory> {
    get_media_item(pool, media_item_id).await?;
    let media_file = get_media_file(pool, input.media_file_id).await?;

    if media_file.media_item_id != media_item_id {
        return Err(ApplicationError::Validation(format!(
            "media file {} does not belong to media item {}",
            input.media_file_id, media_item_id
        )));
    }

    let now = OffsetDateTime::now_utc();
    let latest_open = mova_db::get_latest_open_watch_history(pool, user_id, input.media_file_id)
        .await
        .map_err(ApplicationError::from)?;

    if let Some(existing) = latest_open {
        if should_reuse_open_watch_history(&existing, now) {
            return mova_db::update_watch_history(
                pool,
                mova_db::UpdateWatchHistoryParams {
                    id: existing.id,
                    position_seconds: input.position_seconds,
                    duration_seconds: input.duration_seconds,
                    last_watched_at: now,
                    ended_at: input.is_finished.then_some(now),
                    completed_at: input.is_finished.then_some(now),
                },
            )
            .await
            .map_err(ApplicationError::from);
        }

        mova_db::update_watch_history(
            pool,
            mova_db::UpdateWatchHistoryParams {
                id: existing.id,
                position_seconds: existing.position_seconds,
                duration_seconds: existing.duration_seconds,
                last_watched_at: existing.last_watched_at,
                ended_at: Some(existing.last_watched_at),
                completed_at: existing.completed_at,
            },
        )
        .await
        .map_err(ApplicationError::from)?;
    }

    mova_db::create_watch_history(
        pool,
        mova_db::CreateWatchHistoryParams {
            user_id,
            media_item_id,
            media_file_id: input.media_file_id,
            position_seconds: input.position_seconds,
            duration_seconds: input.duration_seconds,
            started_at: now,
            last_watched_at: now,
            ended_at: input.is_finished.then_some(now),
            completed_at: input.is_finished.then_some(now),
        },
    )
    .await
    .map_err(ApplicationError::from)
}

fn normalize_watch_history_limit(limit: Option<i64>) -> ApplicationResult<i64> {
    let limit = limit.unwrap_or(DEFAULT_WATCH_HISTORY_LIMIT);

    if limit <= 0 {
        return Err(ApplicationError::Validation(
            "limit must be greater than 0".to_string(),
        ));
    }

    Ok(limit.min(MAX_WATCH_HISTORY_LIMIT))
}

fn should_reuse_open_watch_history(entry: &WatchHistory, now: OffsetDateTime) -> bool {
    if entry.ended_at.is_some() {
        return false;
    }

    now - entry.last_watched_at <= WATCH_HISTORY_SESSION_GAP
}

#[cfg(test)]
mod tests {
    use super::{
        normalize_watch_history_limit, should_reuse_open_watch_history, WATCH_HISTORY_SESSION_GAP,
    };
    use crate::error::ApplicationError;
    use mova_domain::WatchHistory;
    use time::{Duration, OffsetDateTime};

    fn build_watch_history(
        last_watched_at: OffsetDateTime,
        ended_at: Option<OffsetDateTime>,
    ) -> WatchHistory {
        WatchHistory {
            id: 1,
            media_item_id: 1,
            media_file_id: 1,
            position_seconds: 120,
            duration_seconds: Some(1800),
            started_at: last_watched_at - Duration::minutes(5),
            last_watched_at,
            ended_at,
            completed_at: None,
        }
    }

    #[test]
    fn normalize_watch_history_limit_uses_default_value() {
        assert_eq!(normalize_watch_history_limit(None).unwrap(), 50);
    }

    #[test]
    fn normalize_watch_history_limit_rejects_non_positive_value() {
        let result = normalize_watch_history_limit(Some(0));

        assert!(matches!(
            result,
            Err(ApplicationError::Validation(message))
                if message.contains("greater than 0")
        ));
    }

    #[test]
    fn should_reuse_open_watch_history_accepts_recent_open_entry() {
        let now = OffsetDateTime::now_utc();
        let entry = build_watch_history(now - Duration::minutes(20), None);

        assert!(should_reuse_open_watch_history(&entry, now));
    }

    #[test]
    fn should_reuse_open_watch_history_rejects_stale_entry() {
        let now = OffsetDateTime::now_utc();
        let entry =
            build_watch_history(now - WATCH_HISTORY_SESSION_GAP - Duration::seconds(1), None);

        assert!(!should_reuse_open_watch_history(&entry, now));
    }

    #[test]
    fn should_reuse_open_watch_history_rejects_closed_entry() {
        let now = OffsetDateTime::now_utc();
        let entry = build_watch_history(
            now - Duration::minutes(10),
            Some(now - Duration::minutes(1)),
        );

        assert!(!should_reuse_open_watch_history(&entry, now));
    }
}
