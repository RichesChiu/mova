//! Shared use cases for external media clients. Wire DTOs belong to the server.
use crate::{ApplicationError, ApplicationResult};
pub use mova_db::client_playback::{Device, PlaySession};
pub use mova_db::{CatalogQuery, CatalogResult};
use mova_domain::{MediaFile, MediaItem, PlaybackProgress, UserProfile};
use sqlx::PgPool;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

pub async fn details(
    pool: &PgPool,
    user: &UserProfile,
    items: &[MediaItem],
) -> ApplicationResult<mova_db::CatalogDetails> {
    if items.iter().any(|i| !user.can_access_library(i.library_id)) {
        return Err(ApplicationError::Forbidden("library access denied".into()));
    }
    Ok(mova_db::catalog_details(
        pool,
        user.user.id,
        &items.iter().map(|i| i.id).collect::<Vec<_>>(),
    )
    .await?)
}

pub async fn seasons(
    pool: &PgPool,
    user: &UserProfile,
    series_id: i64,
) -> ApplicationResult<Vec<mova_domain::Season>> {
    let series = item(pool, user, series_id).await?;
    if series.media_type != "series" {
        return Err(ApplicationError::Validation("item is not a series".into()));
    }
    let episodes = mova_db::list_episodes_for_series(pool, series_id).await?;
    let mut seasons = mova_db::list_seasons_for_series(pool, series_id).await?;
    for season in &mut seasons {
        season.episode_count = episodes
            .iter()
            .filter(|e| e.season_id == season.id && e.has_files)
            .count() as i64;
    }
    seasons.retain(|s| s.episode_count > 0);
    Ok(seasons)
}

pub async fn season(
    pool: &PgPool,
    user: &UserProfile,
    id: i64,
) -> ApplicationResult<mova_domain::Season> {
    Ok(crate::authorize_season_with_library(pool, user, id)
        .await?
        .0)
}

pub const DEVICE_SESSION_DAYS: i64 = 90;
pub const TICKS_PER_SECOND: i64 = 10_000_000;

#[derive(Default)]
pub struct DirectProfile {
    pub container: String,
    pub video_codec: String,
    pub audio_codec: String,
}
pub struct PlaybackCapabilities {
    pub direct_play: bool,
    pub direct_stream: bool,
    pub max_bitrate: i64,
    pub profiles: Option<Vec<DirectProfile>>,
}
pub fn can_deliver_original(
    file: &MediaFile,
    capabilities: &PlaybackCapabilities,
) -> ApplicationResult<bool> {
    if capabilities.max_bitrate < 0 {
        return Err(ApplicationError::Validation(
            "invalid maximum bitrate".into(),
        ));
    }
    if !capabilities.direct_play && !capabilities.direct_stream {
        return Ok(false);
    }
    if file.bitrate.is_some_and(|b| b > capabilities.max_bitrate) {
        return Ok(false);
    }
    let Some(profiles) = &capabilities.profiles else {
        return Ok(true);
    };
    // Unknown STRM characteristics require client-side probing. Do not infer
    // codecs or bitrate from the .strm carrier itself.
    Ok(profiles.iter().any(|p| {
        profile_matches(&p.container, file.container.as_deref())
            && profile_matches(&p.video_codec, file.video_codec.as_deref())
            && profile_matches(&p.audio_codec, file.audio_codec.as_deref())
    }))
}
fn profile_matches(allowed: &str, actual: Option<&str>) -> bool {
    allowed.is_empty() || actual.is_none() || allowed.split(',').any(|s| Some(s.trim()) == actual)
}

pub struct ClientContext {
    pub user: UserProfile,
    pub device: Device,
    pub token_hash: String,
}

pub async fn server_id(pool: &PgPool) -> ApplicationResult<String> {
    Ok(mova_db::client_playback::server_id(pool, &Uuid::new_v4().simple().to_string()).await?)
}

pub async fn login(
    pool: &PgPool,
    username: String,
    password: String,
    device: Device,
) -> ApplicationResult<(String, ClientContext)> {
    for (value, max) in [
        (&device.device_id, 256),
        (&device.device_name, 256),
        (&device.client_name, 128),
        (&device.client_version, 64),
    ] {
        if value.trim().is_empty() || value.chars().count() > max {
            return Err(ApplicationError::Validation(
                "invalid client identification".into(),
            ));
        }
    }
    let user = crate::users::authenticate_login(pool, username, password).await?;
    let token = crate::users::generate_native_token();
    let token_hash = crate::users::hash_token(&token);
    mova_db::client_playback::create_session(
        pool,
        mova_db::CreateSessionParams {
            token_hash: token_hash.clone(),
            user_id: user.user.id,
            expires_at: OffsetDateTime::now_utc() + Duration::days(DEVICE_SESSION_DAYS),
        },
        &device,
    )
    .await?;
    Ok((
        token,
        ClientContext {
            user,
            device,
            token_hash,
        },
    ))
}

pub async fn authenticate(pool: &PgPool, token: &str) -> ApplicationResult<ClientContext> {
    let token_hash = crate::users::hash_token(token);
    let device = mova_db::client_playback::get_device(pool, &token_hash)
        .await?
        .ok_or_else(|| ApplicationError::Unauthorized("invalid client token".into()))?;
    let session = crate::get_user_session(pool, token).await?;
    Ok(ClientContext {
        user: session.user,
        device,
        token_hash,
    })
}

pub async fn logout(pool: &PgPool, context: &ClientContext) -> ApplicationResult<()> {
    Ok(mova_db::delete_session_by_token_hash(pool, &context.token_hash).await?)
}

pub async fn catalog(
    pool: &PgPool,
    user: &UserProfile,
    mut q: CatalogQuery,
) -> ApplicationResult<CatalogResult> {
    if q.offset < 0 || q.limit < 0 || q.limit > 500 {
        return Err(ApplicationError::Validation(
            "StartIndex must be nonnegative; Limit must be between 0 and 500".into(),
        ));
    }
    if q.kinds
        .iter()
        .any(|s| !matches!(s.as_str(), "movie" | "series" | "episode"))
    {
        return Err(ApplicationError::Validation("unsupported item type".into()));
    }
    for (name, _) in &q.sort {
        if !matches!(
            name.as_str(),
            "Name"
                | "SortName"
                | "DateCreated"
                | "DatePlayed"
                | "ProductionYear"
                | "PremiereDate"
                | "IndexNumber"
                | "ParentIndexNumber"
        ) {
            return Err(ApplicationError::Validation(format!(
                "unsupported SortBy: {name}"
            )));
        }
    }
    if let Some(id) = q.library_id {
        crate::authorize_library(pool, user, id).await?;
    }
    if let Some(id) = q.series_id {
        crate::authorize_media_item_with_library(pool, user, id).await?;
    }
    if let Some(id) = q.season_id {
        crate::authorize_season_with_library(pool, user, id).await?;
    }
    q.user_id = user.user.id;
    q.library_ids = user
        .library_visibility()
        .restricted_library_ids()
        .map(<[i64]>::to_vec);
    Ok(mova_db::query_catalog(pool, &q).await?)
}

pub async fn item(pool: &PgPool, user: &UserProfile, id: i64) -> ApplicationResult<MediaItem> {
    Ok(crate::authorize_media_item_with_library(pool, user, id)
        .await?
        .0)
}

pub async fn select_file(
    pool: &PgPool,
    user: &UserProfile,
    item_id: i64,
    file_id: Option<i64>,
) -> ApplicationResult<MediaFile> {
    item(pool, user, item_id).await?;
    let files = crate::list_media_files_for_media_item(pool, item_id).await?;
    let selected = if let Some(id) = file_id {
        files.into_iter().find(|f| f.id == id)
    } else {
        let progress =
            crate::get_playback_progress_for_media_item(pool, user.user.id, item_id).await?;
        let preferred = progress.and_then(|p| p.last_media_file_id);
        files
            .iter()
            .find(|f| Some(f.id) == preferred)
            .cloned()
            .or_else(|| files.into_iter().next())
    };
    selected.ok_or_else(|| ApplicationError::NotFound("playable media source not found".into()))
}

pub async fn prepare(
    pool: &PgPool,
    context: &ClientContext,
    item_id: i64,
    file_id: Option<i64>,
) -> ApplicationResult<(PlaySession, MediaFile)> {
    let file = select_file(pool, &context.user, item_id, file_id).await?;
    let session = PlaySession {
        id: Uuid::new_v4().simple().to_string(),
        media_item_id: item_id,
        media_file_id: file.id,
        stopped: false,
    };
    mova_db::client_playback::create_play_session(
        pool,
        &session.id,
        &context.token_hash,
        context.user.user.id,
        item_id,
        file.id,
    )
    .await?;
    Ok((session, file))
}

pub fn ticks_to_seconds(ticks: i64) -> ApplicationResult<i32> {
    if ticks < 0 {
        return Err(ApplicationError::Validation(
            "ticks cannot be negative".into(),
        ));
    }
    i32::try_from(ticks / TICKS_PER_SECOND)
        .map_err(|_| ApplicationError::Validation("ticks exceed supported duration".into()))
}

pub struct PlaybackReport {
    pub item_id: i64,
    pub file_id: Option<i64>,
    pub session_id: Option<String>,
    pub position_ticks: Option<i64>,
    pub duration_ticks: Option<i64>,
    pub stopped: bool,
    pub started: bool,
}

pub async fn report(
    pool: &PgPool,
    context: &ClientContext,
    input: PlaybackReport,
) -> ApplicationResult<Option<PlaybackProgress>> {
    item(pool, &context.user, input.item_id).await?;
    let session = mova_db::client_playback::get_play_session(
        pool,
        &context.token_hash,
        input.session_id.as_deref(),
        input.item_id,
        input.file_id,
    )
    .await?;
    let session = match session {
        Some(s) if s.stopped && input.started && input.session_id.is_none() => {
            prepare(pool, context, input.item_id, input.file_id)
                .await?
                .0
        }
        Some(s) => s,
        None if input.session_id.is_none() => {
            prepare(pool, context, input.item_id, input.file_id)
                .await?
                .0
        }
        None => {
            return Err(ApplicationError::NotFound(
                "playback session not found".into(),
            ))
        }
    };
    let file = select_file(
        pool,
        &context.user,
        input.item_id,
        Some(session.media_file_id),
    )
    .await?;
    let previous =
        crate::get_playback_progress_for_media_item(pool, context.user.user.id, input.item_id)
            .await?;
    let duration = file
        .duration_seconds
        .or(input.duration_ticks.map(ticks_to_seconds).transpose()?)
        .filter(|v| *v > 0)
        .or_else(|| previous.as_ref().and_then(|p| p.duration_seconds));
    let position = input
        .position_ticks
        .map(ticks_to_seconds)
        .transpose()?
        .unwrap_or_else(|| previous.as_ref().map_or(0, |p| p.position_seconds));
    let position = duration.map_or(position, |d| position.min(d));
    // The stopped event alone is not evidence that the media was completed.
    let finished = duration.is_some_and(|d| i64::from(position) * 100 >= i64::from(d) * 95);
    Ok(mova_db::client_playback::report_progress(
        pool,
        &context.token_hash,
        &session.id,
        input.stopped,
        mova_db::UpsertPlaybackProgressParams {
            user_id: context.user.user.id,
            media_item_id: input.item_id,
            media_file_id: file.id,
            position_seconds: position,
            duration_seconds: duration,
            is_finished: finished,
        },
    )
    .await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tick_conversion_checks_bounds_and_rounds_down() {
        assert_eq!(ticks_to_seconds(19_999_999).unwrap(), 1);
        assert!(ticks_to_seconds(-1).is_err());
        assert!(ticks_to_seconds(i64::MAX).is_err());
    }
}
