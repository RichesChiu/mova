use crate::{
    error::{ApplicationError, ApplicationResult},
    media_items::{get_media_file, get_media_item},
    watch_history::{record_watch_history_for_media_item, RecordWatchHistoryInput},
};
use mova_domain::{ContinueWatchingItem, PlaybackProgress};
use sqlx::postgres::PgPool;

const DEFAULT_CONTINUE_WATCHING_LIMIT: i64 = 20;
const MAX_CONTINUE_WATCHING_LIMIT: i64 = 100;

/// 更新播放进度时使用的命令对象。
#[derive(Debug, Clone)]
pub struct UpdatePlaybackProgressInput {
    pub media_file_id: i64,
    pub position_seconds: i32,
    pub duration_seconds: Option<i32>,
    pub is_finished: bool,
}

/// 读取“继续观看”列表，默认返回最近 20 条未看完内容。
pub async fn list_continue_watching(
    pool: &PgPool,
    user_id: i64,
    limit: Option<i64>,
) -> ApplicationResult<Vec<ContinueWatchingItem>> {
    let limit = normalize_continue_watching_limit(limit)?;

    mova_db::list_continue_watching(pool, user_id, limit)
        .await
        .map_err(ApplicationError::from)
}

/// 读取某个媒体条目的最近播放进度。
/// 没有进度是正常情况，因此这里返回 `Option`，而不是 404。
pub async fn get_playback_progress_for_media_item(
    pool: &PgPool,
    user_id: i64,
    media_item_id: i64,
) -> ApplicationResult<Option<PlaybackProgress>> {
    get_media_item(pool, media_item_id).await?;

    mova_db::get_playback_progress_for_media_item(pool, user_id, media_item_id)
        .await
        .map_err(ApplicationError::from)
}

/// 为指定媒体文件写入播放进度。
/// 当前要求 `media_file_id` 必须确实属于传入的 `media_item_id`，避免前端误传错资源。
pub async fn update_playback_progress_for_media_item(
    pool: &PgPool,
    user_id: i64,
    media_item_id: i64,
    input: UpdatePlaybackProgressInput,
) -> ApplicationResult<PlaybackProgress> {
    validate_progress_seconds("position_seconds", input.position_seconds)?;
    if let Some(duration_seconds) = input.duration_seconds {
        validate_progress_seconds("duration_seconds", duration_seconds)?;
    }

    get_media_item(pool, media_item_id).await?;
    let media_file = get_media_file(pool, input.media_file_id).await?;

    if media_file.media_item_id != media_item_id {
        return Err(ApplicationError::Validation(format!(
            "media file {} does not belong to media item {}",
            input.media_file_id, media_item_id
        )));
    }

    let position_seconds = input
        .duration_seconds
        .map_or(input.position_seconds, |duration_seconds| {
            input.position_seconds.min(duration_seconds)
        });

    let progress = mova_db::upsert_playback_progress(
        pool,
        mova_db::UpsertPlaybackProgressParams {
            user_id,
            media_item_id,
            media_file_id: input.media_file_id,
            position_seconds,
            duration_seconds: input.duration_seconds,
            is_finished: input.is_finished,
        },
    )
    .await
    .map_err(ApplicationError::from)?;

    record_watch_history_for_media_item(
        pool,
        user_id,
        media_item_id,
        RecordWatchHistoryInput {
            media_file_id: input.media_file_id,
            position_seconds,
            duration_seconds: input.duration_seconds,
            is_finished: input.is_finished,
        },
    )
    .await?;

    Ok(progress)
}

fn validate_progress_seconds(field_name: &str, seconds: i32) -> ApplicationResult<()> {
    if seconds < 0 {
        return Err(ApplicationError::Validation(format!(
            "{} cannot be negative",
            field_name
        )));
    }

    Ok(())
}

fn normalize_continue_watching_limit(limit: Option<i64>) -> ApplicationResult<i64> {
    let limit = limit.unwrap_or(DEFAULT_CONTINUE_WATCHING_LIMIT);

    if limit <= 0 {
        return Err(ApplicationError::Validation(
            "limit must be greater than 0".to_string(),
        ));
    }

    Ok(limit.min(MAX_CONTINUE_WATCHING_LIMIT))
}

#[cfg(test)]
mod tests {
    use super::{normalize_continue_watching_limit, validate_progress_seconds};
    use crate::error::ApplicationError;

    #[test]
    fn validate_progress_seconds_accepts_zero() {
        assert!(validate_progress_seconds("position_seconds", 0).is_ok());
    }

    #[test]
    fn validate_progress_seconds_rejects_negative_value() {
        let result = validate_progress_seconds("position_seconds", -1);

        assert!(matches!(
            result,
            Err(ApplicationError::Validation(message))
                if message.contains("cannot be negative")
        ));
    }

    #[test]
    fn normalize_continue_watching_limit_uses_default_value() {
        assert_eq!(normalize_continue_watching_limit(None).unwrap(), 20);
    }

    #[test]
    fn normalize_continue_watching_limit_caps_large_value() {
        assert_eq!(normalize_continue_watching_limit(Some(999)).unwrap(), 100);
    }

    #[test]
    fn normalize_continue_watching_limit_rejects_non_positive_value() {
        let result = normalize_continue_watching_limit(Some(0));

        assert!(matches!(
            result,
            Err(ApplicationError::Validation(message))
                if message.contains("greater than 0")
        ));
    }
}
