use crate::auth::{require_media_item_access, require_user};
use crate::error::ApiError;
use crate::response::{ok, ApiJson, ContinueWatchingItemResponse, PlaybackProgressResponse};
use crate::state::AppState;
use axum::{
    extract::{Path, Query, State},
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::Deserialize;

/// 更新播放进度接口接收的请求体。
#[derive(Debug, Deserialize)]
pub struct UpdatePlaybackProgressRequest {
    pub media_file_id: i64,
    pub position_seconds: i32,
    pub duration_seconds: Option<i32>,
    pub is_finished: Option<bool>,
}

/// 查询“继续观看”列表时支持的可选参数。
#[derive(Debug, Deserialize)]
pub struct ContinueWatchingQuery {
    pub limit: Option<i64>,
}

/// 读取当前登录用户的“继续观看”列表。
pub async fn list_continue_watching(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(query): Query<ContinueWatchingQuery>,
) -> Result<ApiJson<Vec<ContinueWatchingItemResponse>>, ApiError> {
    let user = require_user(&state, &jar).await?;
    let items = mova_application::list_continue_watching(&state.db, user.user.id, query.limit)
        .await
        .map_err(ApiError::from)?;

    Ok(ok(items
        .into_iter()
        .filter(|item| user.can_access_library(item.media_item.library_id))
        .map(|item| ContinueWatchingItemResponse::from_domain(item, state.api_time_offset))
        .collect()))
}

/// 读取某个媒体条目的最近播放进度。
pub async fn get_media_item_playback_progress(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(media_item_id): Path<i64>,
) -> Result<ApiJson<Option<PlaybackProgressResponse>>, ApiError> {
    let user = require_user(&state, &jar).await?;
    require_media_item_access(&state, &user, media_item_id).await?;
    let progress = mova_application::get_playback_progress_for_media_item(
        &state.db,
        user.user.id,
        media_item_id,
    )
    .await
    .map_err(ApiError::from)?;

    Ok(ok(progress.map(|value| {
        PlaybackProgressResponse::from_domain(value, state.api_time_offset)
    })))
}

/// 写入某个媒体条目的播放进度。
pub async fn update_media_item_playback_progress(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(media_item_id): Path<i64>,
    Json(request): Json<UpdatePlaybackProgressRequest>,
) -> Result<ApiJson<PlaybackProgressResponse>, ApiError> {
    let user = require_user(&state, &jar).await?;
    require_media_item_access(&state, &user, media_item_id).await?;
    let progress = mova_application::update_playback_progress_for_media_item(
        &state.db,
        user.user.id,
        media_item_id,
        mova_application::UpdatePlaybackProgressInput {
            media_file_id: request.media_file_id,
            position_seconds: request.position_seconds,
            duration_seconds: request.duration_seconds,
            is_finished: request.is_finished.unwrap_or(false),
        },
    )
    .await
    .map_err(ApiError::from)?;

    Ok(ok(PlaybackProgressResponse::from_domain(
        progress,
        state.api_time_offset,
    )))
}

#[cfg(test)]
mod tests {
    use super::{
        get_media_item_playback_progress, list_continue_watching,
        update_media_item_playback_progress, ContinueWatchingQuery, UpdatePlaybackProgressRequest,
    };
    use crate::{
        auth::{attach_session_cookie, SESSION_TTL},
        state::{AppState, RealtimeHub, ScanRegistry},
    };
    use axum::{
        extract::{Path, Query, State},
        Json,
    };
    use axum_extra::extract::cookie::CookieJar;
    use mova_application::NullMetadataProvider;
    use mova_domain::UserRole;
    use std::{path::PathBuf, sync::Arc};
    use time::{OffsetDateTime, UtcOffset};

    fn build_test_state(pool: sqlx::postgres::PgPool) -> AppState {
        AppState {
            db: pool,
            api_time_offset: UtcOffset::UTC,
            artwork_cache_dir: PathBuf::from("/tmp/mova-test-artwork"),
            metadata_provider: Arc::new(NullMetadataProvider),
            scan_registry: ScanRegistry::default(),
            realtime_hub: RealtimeHub::default(),
        }
    }

    async fn seed_playback_context(pool: &sqlx::postgres::PgPool) -> (CookieJar, i64, i64, i64) {
        let library = mova_db::create_library(
            pool,
            mova_db::CreateLibraryParams {
                name: "Movies".to_string(),
                description: None,
                library_type: "movie".to_string(),
                metadata_language: "zh-CN".to_string(),
                root_path: "/media/movies".to_string(),
                is_enabled: true,
            },
        )
        .await
        .unwrap();
        let user = mova_db::create_user(
            pool,
            mova_db::CreateUserParams {
                username: "viewer01".to_string(),
                nickname: "viewer01".to_string(),
                password_hash: "hash".to_string(),
                role: UserRole::Viewer,
                is_enabled: true,
                library_ids: vec![library.id],
            },
        )
        .await
        .unwrap();
        let media_item_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into media_items (
                library_id,
                media_type,
                title,
                source_title
            )
            values ($1, 'movie', 'Interstellar', 'Interstellar')
            returning id
            "#,
        )
        .bind(library.id)
        .fetch_one(pool)
        .await
        .unwrap();
        let media_file_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into media_files (
                library_id,
                media_item_id,
                file_path,
                container,
                file_size,
                duration_seconds
            )
            values ($1, $2, '/media/movies/interstellar.mkv', 'mkv', 1, 7200)
            returning id
            "#,
        )
        .bind(library.id)
        .bind(media_item_id)
        .fetch_one(pool)
        .await
        .unwrap();

        let session_token = "test-session-token";
        let expires_at = OffsetDateTime::now_utc() + SESSION_TTL;
        mova_db::create_session(
            pool,
            mova_db::CreateSessionParams {
                token: session_token.to_string(),
                user_id: user.user.id,
                expires_at,
            },
        )
        .await
        .unwrap();

        (
            attach_session_cookie(CookieJar::new(), session_token, expires_at),
            user.user.id,
            media_item_id,
            media_file_id,
        )
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn playback_progress_returns_null_when_the_user_has_not_started_playing(
        pool: sqlx::postgres::PgPool,
    ) {
        let state = build_test_state(pool.clone());
        let (jar, _user_id, media_item_id, _media_file_id) = seed_playback_context(&pool).await;

        let Json(response) =
            get_media_item_playback_progress(State(state), jar, Path(media_item_id))
                .await
                .unwrap();

        assert!(response.data.is_none());
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn finished_playback_leaves_history_but_disappears_from_continue_watching(
        pool: sqlx::postgres::PgPool,
    ) {
        let state = build_test_state(pool.clone());
        let (jar, user_id, media_item_id, media_file_id) = seed_playback_context(&pool).await;

        let Json(progress_response) = update_media_item_playback_progress(
            State(state.clone()),
            jar.clone(),
            Path(media_item_id),
            Json(UpdatePlaybackProgressRequest {
                media_file_id,
                position_seconds: 300,
                duration_seconds: Some(7200),
                is_finished: Some(false),
            }),
        )
        .await
        .unwrap();

        assert_eq!(progress_response.data.position_seconds, 300);
        assert!(!progress_response.data.is_finished);

        let Json(initial_continue_watching) = list_continue_watching(
            State(state.clone()),
            jar.clone(),
            Query(ContinueWatchingQuery { limit: Some(10) }),
        )
        .await
        .unwrap();

        assert_eq!(initial_continue_watching.data.len(), 1);
        assert_eq!(
            initial_continue_watching.data[0]
                .playback_progress
                .position_seconds,
            300
        );

        let Json(finished_progress_response) = update_media_item_playback_progress(
            State(state.clone()),
            jar.clone(),
            Path(media_item_id),
            Json(UpdatePlaybackProgressRequest {
                media_file_id,
                position_seconds: 7200,
                duration_seconds: Some(7200),
                is_finished: Some(true),
            }),
        )
        .await
        .unwrap();

        assert!(finished_progress_response.data.is_finished);

        let Json(read_back_progress) = get_media_item_playback_progress(
            State(state.clone()),
            jar.clone(),
            Path(media_item_id),
        )
        .await
        .unwrap();

        assert_eq!(
            read_back_progress
                .data
                .as_ref()
                .map(|progress| progress.position_seconds),
            Some(7200)
        );
        assert_eq!(
            read_back_progress
                .data
                .as_ref()
                .map(|progress| progress.is_finished),
            Some(true)
        );

        let Json(finished_continue_watching) = list_continue_watching(
            State(state.clone()),
            jar.clone(),
            Query(ContinueWatchingQuery { limit: Some(10) }),
        )
        .await
        .unwrap();

        assert!(finished_continue_watching.data.is_empty());

        let history = mova_db::list_watch_history(&pool, user_id, 10)
            .await
            .unwrap();
        assert_eq!(history.len(), 1);
        assert!(history[0].watch_history.completed_at.is_some());
        assert!(history[0].watch_history.ended_at.is_some());
    }
}
