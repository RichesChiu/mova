use crate::auth::{require_admin, require_library_access, require_user};
use crate::error::ApiError;
use crate::response::{
    accepted, created, ok, ok_message, with_status, ApiJson, LibraryDetailResponse,
    LibraryResponse, MediaItemListResponse, RecentlyAddedLibraryMediaItemsResponse,
    ScanJobResponse,
};
use crate::state::{AppState, BeginDeleteError};
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::Deserialize;
use tokio::time::{timeout, Duration};

const LIBRARY_SCAN_STOP_WAIT_TIMEOUT: Duration = Duration::from_secs(30);

/// 创建媒体库接口接收的请求体。
/// 这里的 root_path 对应 Plex/Jellyfin 里“这个库要扫描哪个目录”。
#[derive(Debug, Deserialize)]
pub struct CreateLibraryRequest {
    pub name: String,
    pub description: Option<String>,
    pub metadata_language: Option<String>,
    pub root_path: String,
}

/// 更新媒体库接口接收的请求体。
/// 支持更新名称、描述和元数据语言。
#[derive(Debug, Deserialize)]
pub struct UpdateLibraryRequest {
    pub name: Option<String>,
    pub description: Option<Option<String>>,
    pub metadata_language: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ListLibraryMediaItemsQuery {
    pub query: Option<String>,
    pub year: Option<i32>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

#[derive(Debug, Deserialize, Default)]
pub struct RecentlyAddedByLibraryQuery {
    pub days: Option<i64>,
    pub limit: Option<i64>,
}

/// 查询所有已配置的媒体库，供前端渲染列表页或设置页使用。
pub async fn list_libraries(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
) -> Result<ApiJson<Vec<LibraryResponse>>, ApiError> {
    let user = require_user(&state, &headers, &jar).await?;
    let libraries = mova_application::list_libraries(&state.db, user.library_visibility())
        .await
        .map_err(ApiError::from)?;

    Ok(ok(libraries
        .into_iter()
        .map(|library| LibraryResponse::from_domain(library, state.api_time_offset))
        .collect()))
}

/// 查询首页“最新添加”模块所需的按库分组数据。
pub async fn list_recently_added_by_library(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    Query(query): Query<RecentlyAddedByLibraryQuery>,
) -> Result<ApiJson<Vec<RecentlyAddedLibraryMediaItemsResponse>>, ApiError> {
    let user = require_user(&state, &headers, &jar).await?;
    let visible_library_ids = user
        .library_visibility()
        .restricted_library_ids()
        .map(<[i64]>::to_vec);
    let groups = mova_application::list_recently_added_media_items_by_library(
        &state.db,
        mova_application::ListRecentlyAddedByLibraryInput {
            visible_library_ids,
            days: query.days,
            limit: query.limit,
        },
    )
    .await
    .map_err(ApiError::from)?;

    Ok(ok(groups
        .into_iter()
        .map(|group| {
            RecentlyAddedLibraryMediaItemsResponse::from_domain(group, state.api_time_offset)
        })
        .collect()))
}

/// 查询单个媒体库详情。
/// 这里返回库自身信息、当前媒体数量，以及最近一次扫描摘要。
pub async fn get_library(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    Path(library_id): Path<i64>,
) -> Result<ApiJson<LibraryDetailResponse>, ApiError> {
    let user = require_user(&state, &headers, &jar).await?;
    require_library_access(&state, &user, library_id).await?;
    let detail = mova_application::get_library_detail(&state.db, library_id)
        .await
        .map_err(ApiError::from)?;

    Ok(ok(LibraryDetailResponse::from_domain(
        detail,
        state.api_time_offset,
    )))
}

/// 处理创建媒体库请求。
/// handler 只负责接收 HTTP 参数并转发给应用层，真正的业务校验放在 application 层。
pub async fn create_library(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    Json(request): Json<CreateLibraryRequest>,
) -> Result<(StatusCode, ApiJson<LibraryResponse>), ApiError> {
    require_admin(&state, &headers, &jar).await?;
    // 把 HTTP 请求对象转换成应用层命令对象，避免业务层依赖传输协议细节。
    let input = mova_application::CreateLibraryInput {
        name: request.name,
        description: request.description,
        metadata_language: request.metadata_language,
        root_path: request.root_path,
    };

    let library = mova_application::create_library(&state.db, input)
        .await
        .map_err(ApiError::from)?;

    trigger_library_scan_after_create(&state, library.id).await;

    Ok(created(LibraryResponse::from_domain(
        library,
        state.api_time_offset,
    )))
}

/// 更新媒体库基础配置。
pub async fn update_library(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    Path(library_id): Path<i64>,
    Json(request): Json<UpdateLibraryRequest>,
) -> Result<ApiJson<LibraryResponse>, ApiError> {
    require_admin(&state, &headers, &jar).await?;
    if state.scan_registry.is_deleting(library_id) {
        return Err(ApiError::Conflict(format!(
            "library {} is being deleted",
            library_id
        )));
    }

    let previous_library = mova_application::get_library(&state.db, library_id)
        .await
        .map_err(ApiError::from)?;
    let requested_metadata_language = request
        .metadata_language
        .as_ref()
        .map(|value| {
            mova_application::normalize_metadata_language(
                Some(value.clone()),
                mova_application::DEFAULT_TMDB_LANGUAGE,
            )
            .map_err(|error| ApiError::BadRequest(error.to_string()))
        })
        .transpose()?;
    let metadata_language_will_change = requested_metadata_language
        .as_deref()
        .is_some_and(|language| language != previous_library.metadata_language);

    if metadata_language_will_change {
        stop_active_library_scan_for_metadata_language_change(&state, library_id).await?;
    }

    let updated_library = mova_application::update_library(
        &state.db,
        library_id,
        mova_application::UpdateLibraryInput {
            name: request.name,
            description: request.description,
            metadata_language: request.metadata_language,
        },
    )
    .await
    .map_err(ApiError::from)?;

    let metadata_language_changed =
        previous_library.metadata_language != updated_library.metadata_language;

    if metadata_language_changed {
        mova_application::prepare_library_metadata_rescan(&state.db, library_id)
            .await
            .map_err(ApiError::from)?;

        trigger_library_scan_after_metadata_language_change(&state, library_id).await?;
    }

    Ok(ok(LibraryResponse::from_domain(
        updated_library,
        state.api_time_offset,
    )))
}

/// 删除媒体库。
/// 删除前会先阻止新的扫描启动，并尽量等待当前扫描安全停止。
pub async fn delete_library(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    Path(library_id): Path<i64>,
) -> Result<ApiJson<()>, ApiError> {
    require_admin(&state, &headers, &jar).await?;
    mova_application::get_library(&state.db, library_id)
        .await
        .map_err(ApiError::from)?;

    let _delete_guard =
        state
            .scan_registry
            .begin_delete(library_id)
            .map_err(|error| match error {
                BeginDeleteError::AlreadyDeleting => {
                    ApiError::Conflict(format!("library {} is already being deleted", library_id))
                }
            })?;

    if let Some(active_scan) = state.scan_registry.active_scan(library_id) {
        active_scan.cancel();

        timeout(LIBRARY_SCAN_STOP_WAIT_TIMEOUT, active_scan.wait_finished())
            .await
            .map_err(|_| {
                ApiError::Conflict(format!(
                    "library {} is still stopping scan job {}, please retry shortly",
                    library_id,
                    active_scan.scan_job_id()
                ))
            })?;
    }

    mova_application::delete_library(&state.db, library_id, &state.artwork_cache_dir)
        .await
        .map_err(ApiError::from)?;
    Ok(ok_message("library deleted", ()))
}

/// 查询某个媒体库下已经扫描出的媒体条目。
pub async fn list_library_media_items(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    Path(library_id): Path<i64>,
    Query(query): Query<ListLibraryMediaItemsQuery>,
) -> Result<ApiJson<MediaItemListResponse>, ApiError> {
    let user = require_user(&state, &headers, &jar).await?;
    require_library_access(&state, &user, library_id).await?;
    let media_items = mova_application::list_media_items_for_library(
        &state.db,
        library_id,
        mova_application::ListMediaItemsForLibraryInput {
            query: query.query,
            year: query.year,
            page: query.page,
            page_size: query.page_size,
        },
    )
    .await
    .map_err(ApiError::from)?;

    Ok(ok(MediaItemListResponse::from_domain(
        media_items,
        state.api_time_offset,
    )))
}

/// 查询某个媒体库的扫描历史。
/// 这个接口主要保留给排障和调试使用，不作为详情页首屏主数据。
pub async fn list_library_scan_jobs(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    Path(library_id): Path<i64>,
) -> Result<ApiJson<Vec<ScanJobResponse>>, ApiError> {
    require_admin(&state, &headers, &jar).await?;
    let scan_jobs = mova_application::list_scan_jobs_for_library(&state.db, library_id)
        .await
        .map_err(ApiError::from)?;

    Ok(ok(scan_jobs
        .into_iter()
        .map(|scan_job| ScanJobResponse::from_domain(scan_job, state.api_time_offset))
        .collect()))
}

/// 查询某个媒体库下的单个扫描任务详情。
/// 前端可轮询这个接口获取异步扫描的实时状态。
pub async fn get_library_scan_job(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    Path((library_id, scan_job_id)): Path<(i64, i64)>,
) -> Result<ApiJson<ScanJobResponse>, ApiError> {
    require_admin(&state, &headers, &jar).await?;
    let scan_job = mova_application::get_scan_job_for_library(&state.db, library_id, scan_job_id)
        .await
        .map_err(ApiError::from)?;

    Ok(ok(ScanJobResponse::from_domain(
        scan_job,
        state.api_time_offset,
    )))
}

/// 触发一次媒体库扫描。
/// 如果当前媒体库已存在活跃任务，则直接返回该任务，避免重复扫描。
pub async fn scan_library(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    Path(library_id): Path<i64>,
) -> Result<(StatusCode, ApiJson<ScanJobResponse>), ApiError> {
    require_admin(&state, &headers, &jar).await?;
    if state.scan_registry.is_deleting(library_id) {
        return Err(ApiError::Conflict(format!(
            "library {} is being deleted",
            library_id
        )));
    }

    let enqueue_result = mova_application::enqueue_library_scan(&state.db, library_id)
        .await
        .map_err(ApiError::from)?;

    if enqueue_result.created {
        state.background_jobs.wake();
    }

    let response = ScanJobResponse::from_domain(enqueue_result.scan_job, state.api_time_offset);
    Ok(if enqueue_result.created {
        accepted(response)
    } else {
        with_status(StatusCode::OK, "ok", response)
    })
}

async fn trigger_library_scan_after_create(state: &AppState, library_id: i64) {
    let enqueue_result = match mova_application::enqueue_library_scan(&state.db, library_id).await {
        Ok(result) => result,
        Err(error) => {
            tracing::warn!(
                library_id,
                error = ?error,
                "library created but initial scan could not be enqueued"
            );
            return;
        }
    };

    if !enqueue_result.created {
        return;
    }

    state.background_jobs.wake();
}

async fn stop_active_library_scan_for_metadata_language_change(
    state: &AppState,
    library_id: i64,
) -> Result<(), ApiError> {
    let Some(active_scan) = state.scan_registry.active_scan(library_id) else {
        return Ok(());
    };

    active_scan.cancel();
    timeout(LIBRARY_SCAN_STOP_WAIT_TIMEOUT, active_scan.wait_finished())
        .await
        .map_err(|_| {
            ApiError::Conflict(format!(
                "library {} is still stopping its active scan; metadata language was not changed",
                library_id
            ))
        })?;

    Ok(())
}

async fn trigger_library_scan_after_metadata_language_change(
    state: &AppState,
    library_id: i64,
) -> Result<(), ApiError> {
    let enqueue_result = mova_application::enqueue_library_scan(&state.db, library_id)
        .await
        .map_err(ApiError::from)?;

    if !enqueue_result.created {
        return Ok(());
    }

    state.background_jobs.wake();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        delete_library, get_library_scan_job, list_library_scan_jobs, scan_library, update_library,
        UpdateLibraryRequest,
    };
    use crate::{
        auth::{attach_session_cookie, SESSION_TTL},
        error::ApiError,
        state::{
            AppState, BackgroundJobNotifier, RealtimeDispatcherHandle, RealtimeHub, ScanRegistry,
        },
    };
    use axum::{
        extract::{Path, State},
        http::HeaderMap,
        Json,
    };
    use axum_extra::extract::cookie::CookieJar;
    use mova_application::NullMetadataProvider;
    use mova_domain::UserRole;
    use std::{
        path::PathBuf,
        sync::{atomic::Ordering, Arc},
    };
    use time::{OffsetDateTime, UtcOffset};

    fn build_test_state(pool: sqlx::postgres::PgPool) -> AppState {
        AppState {
            db: pool,
            api_time_offset: UtcOffset::UTC,
            artwork_cache_dir: PathBuf::from("/tmp/mova-test-artwork"),
            metadata_provider: Arc::new(NullMetadataProvider),
            scan_registry: ScanRegistry::default(),
            realtime_hub: RealtimeHub::default(),
            realtime_dispatcher: RealtimeDispatcherHandle::default(),
            background_jobs: BackgroundJobNotifier::default(),
        }
    }

    async fn seed_admin_session(pool: &sqlx::postgres::PgPool) -> (i64, CookieJar) {
        seed_user_session(
            pool,
            "admin01",
            UserRole::Admin,
            Vec::new(),
            "admin-session",
        )
        .await
    }

    async fn seed_viewer_session(
        pool: &sqlx::postgres::PgPool,
        library_ids: Vec<i64>,
    ) -> (i64, CookieJar) {
        seed_user_session(
            pool,
            "viewer01",
            UserRole::Viewer,
            library_ids,
            "viewer-session",
        )
        .await
    }

    async fn seed_user_session(
        pool: &sqlx::postgres::PgPool,
        username: &str,
        role: UserRole,
        library_ids: Vec<i64>,
        session_token: &str,
    ) -> (i64, CookieJar) {
        let user = mova_db::create_user(
            pool,
            mova_db::CreateUserParams {
                username: username.to_string(),
                nickname: username.to_string(),
                password_hash: "hash".to_string(),
                role,
                is_enabled: true,
                library_ids,
            },
        )
        .await
        .unwrap();
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
            user.user.id,
            attach_session_cookie(CookieJar::new(), session_token, expires_at),
        )
    }

    async fn seed_library(pool: &sqlx::postgres::PgPool, name: &str) -> i64 {
        mova_db::create_library(
            pool,
            mova_db::CreateLibraryParams {
                name: name.to_string(),
                description: Some(format!("{name} description")),
                metadata_language: "zh-CN".to_string(),
                root_path: format!("/media/{}", name.to_lowercase()),
            },
        )
        .await
        .unwrap()
        .id
    }

    async fn seed_scan_job(pool: &sqlx::postgres::PgPool, library_id: i64) -> i64 {
        mova_db::create_scan_job(pool, mova_db::CreateScanJobParams { library_id })
            .await
            .unwrap()
            .id
    }

    async fn seed_library_media_graph(
        pool: &sqlx::postgres::PgPool,
        library_id: i64,
        user_id: i64,
    ) {
        let series_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into media_items (library_id, media_type, title, source_title, metadata_status)
            values ($1, 'series', 'Series title', 'Series title', 'matched')
            returning id
            "#,
        )
        .bind(library_id)
        .fetch_one(pool)
        .await
        .unwrap();
        let episode_media_item_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into media_items (library_id, media_type, title, source_title, metadata_status)
            values ($1, 'episode', 'Episode title', 'Episode title', 'matched')
            returning id
            "#,
        )
        .bind(library_id)
        .fetch_one(pool)
        .await
        .unwrap();
        let season_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into seasons (series_id, season_number, title)
            values ($1, 1, 'Season 1')
            returning id
            "#,
        )
        .bind(series_id)
        .fetch_one(pool)
        .await
        .unwrap();

        sqlx::query(
            r#"
            insert into episodes (media_item_id, series_id, season_id, episode_number, title)
            values ($1, $2, $3, 1, 'Pilot')
            "#,
        )
        .bind(episode_media_item_id)
        .bind(series_id)
        .bind(season_id)
        .execute(pool)
        .await
        .unwrap();

        sqlx::query(
            r#"
            insert into series_episode_outline_cache (series_media_item_id, outline_json, expires_at)
            values ($1, '{}', now() + interval '1 day')
            "#,
        )
        .bind(series_id)
        .execute(pool)
        .await
        .unwrap();

        sqlx::query(
            r#"
            insert into media_item_cast_cache (media_item_id, expires_at)
            values ($1, now() + interval '1 day')
            "#,
        )
        .bind(series_id)
        .execute(pool)
        .await
        .unwrap();

        sqlx::query(
            r#"
            insert into media_item_cast_members (media_item_id, sort_order, name)
            values ($1, 1, 'Cast Member')
            "#,
        )
        .bind(series_id)
        .execute(pool)
        .await
        .unwrap();

        let media_file_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into media_files (library_id, media_item_id, file_path, file_size)
            values ($1, $2, '/media/movies/series.s01e01.mkv', 1024)
            returning id
            "#,
        )
        .bind(library_id)
        .bind(episode_media_item_id)
        .fetch_one(pool)
        .await
        .unwrap();

        sqlx::query(
            r#"
            insert into subtitle_files (media_file_id, source_kind)
            values ($1, 'embedded')
            "#,
        )
        .bind(media_file_id)
        .execute(pool)
        .await
        .unwrap();

        sqlx::query(
            r#"
            insert into audio_tracks (media_file_id, stream_index)
            values ($1, 0)
            "#,
        )
        .bind(media_file_id)
        .execute(pool)
        .await
        .unwrap();

        sqlx::query(
            r#"
            insert into playback_progress (user_id, media_item_id, media_file_id, position_seconds)
            values ($1, $2, $3, 60)
            "#,
        )
        .bind(user_id)
        .bind(episode_media_item_id)
        .bind(media_file_id)
        .execute(pool)
        .await
        .unwrap();

        sqlx::query(
            r#"
            insert into continue_watching (
                user_id,
                media_item_id,
                last_played_media_item_id,
                media_file_id
            )
            values ($1, $2, $3, $4)
            "#,
        )
        .bind(user_id)
        .bind(series_id)
        .bind(episode_media_item_id)
        .bind(media_file_id)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn assert_library_media_graph_removed(pool: &sqlx::postgres::PgPool) {
        for (table_name, statement) in [
            ("scan_jobs", "select count(*) from scan_jobs"),
            (
                "user_library_access",
                "select count(*) from user_library_access",
            ),
            (
                "playback_progress",
                "select count(*) from playback_progress",
            ),
            (
                "continue_watching",
                "select count(*) from continue_watching",
            ),
            ("subtitle_files", "select count(*) from subtitle_files"),
            ("audio_tracks", "select count(*) from audio_tracks"),
            (
                "series_episode_outline_cache",
                "select count(*) from series_episode_outline_cache",
            ),
            (
                "media_item_cast_members",
                "select count(*) from media_item_cast_members",
            ),
            (
                "media_item_cast_cache",
                "select count(*) from media_item_cast_cache",
            ),
            ("episodes", "select count(*) from episodes"),
            ("seasons", "select count(*) from seasons"),
            ("media_files", "select count(*) from media_files"),
            ("media_items", "select count(*) from media_items"),
        ] {
            let row_count = sqlx::query_scalar::<_, i64>(statement)
                .fetch_one(pool)
                .await
                .unwrap();

            assert_eq!(row_count, 0, "{table_name} rows should be removed");
        }
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn update_library_rejects_changes_while_delete_is_in_progress(
        pool: sqlx::postgres::PgPool,
    ) {
        let state = build_test_state(pool.clone());
        let (_admin_id, admin_jar) = seed_admin_session(&pool).await;
        let library_id = seed_library(&pool, "Movies").await;
        let delete_guard = state.scan_registry.begin_delete(library_id).unwrap();

        let error = update_library(
            State(state),
            HeaderMap::new(),
            admin_jar,
            Path(library_id),
            Json(UpdateLibraryRequest {
                name: Some("Renamed Movies".to_string()),
                description: None,
                metadata_language: None,
            }),
        )
        .await
        .unwrap_err();

        drop(delete_guard);

        match error {
            ApiError::Conflict(message) => {
                assert_eq!(message, format!("library {} is being deleted", library_id));
            }
            other => panic!("expected conflict error, got {other:?}"),
        }
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn update_library_metadata_language_marks_all_items_pending_and_starts_scan(
        pool: sqlx::postgres::PgPool,
    ) {
        let state = build_test_state(pool.clone());
        let (admin_id, admin_jar) = seed_admin_session(&pool).await;
        let library_id = seed_library(&pool, "Movies").await;
        seed_library_media_graph(&pool, library_id, admin_id).await;

        let Json(response) = update_library(
            State(state),
            HeaderMap::new(),
            admin_jar,
            Path(library_id),
            Json(UpdateLibraryRequest {
                name: None,
                description: None,
                metadata_language: Some("en-US".to_string()),
            }),
        )
        .await
        .unwrap();

        assert_eq!(response.data.metadata_language, "en-US");

        let pending_count = sqlx::query_scalar::<_, i64>(
            "select count(*) from media_items where library_id = $1 and metadata_status = 'pending'",
        )
        .bind(library_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        let media_count =
            sqlx::query_scalar::<_, i64>("select count(*) from media_items where library_id = $1")
                .bind(library_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(media_count > 0);
        assert_eq!(pending_count, media_count);

        let scan_jobs = mova_db::list_scan_jobs_for_library(&pool, library_id)
            .await
            .unwrap();
        assert_eq!(scan_jobs.len(), 1);
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn delete_library_cancels_the_active_scan_and_removes_owned_data(
        pool: sqlx::postgres::PgPool,
    ) {
        let state = build_test_state(pool.clone());
        let (admin_id, admin_jar) = seed_admin_session(&pool).await;
        let library_id = seed_library(&pool, "Movies").await;
        let (_viewer_id, _viewer_jar) = seed_viewer_session(&pool, vec![library_id]).await;
        seed_scan_job(&pool, library_id).await;
        seed_library_media_graph(&pool, library_id, admin_id).await;
        let active_scan = state.scan_registry.register_scan(library_id, 42).unwrap();
        let cancellation_flag = active_scan.cancellation_flag();
        let finish_state = state.clone();

        tokio::spawn(async move {
            while !cancellation_flag.load(Ordering::SeqCst) {
                tokio::task::yield_now().await;
            }
            finish_state.scan_registry.finish_scan(library_id, 42);
        });

        let Json(response) = delete_library(
            State(state.clone()),
            HeaderMap::new(),
            admin_jar,
            Path(library_id),
        )
        .await
        .unwrap();

        assert_eq!(response.message, "library deleted");
        assert!(mova_db::get_library(&pool, library_id)
            .await
            .unwrap()
            .is_none());
        assert_library_media_graph_removed(&pool).await;
        assert!(state.scan_registry.active_scan(library_id).is_none());
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn scan_endpoints_require_admin_even_for_viewers_with_library_access(
        pool: sqlx::postgres::PgPool,
    ) {
        let state = build_test_state(pool.clone());
        let library_id = seed_library(&pool, "Movies").await;
        let scan_job_id = seed_scan_job(&pool, library_id).await;
        let (_viewer_id, viewer_jar) = seed_viewer_session(&pool, vec![library_id]).await;

        let list_error = list_library_scan_jobs(
            State(state.clone()),
            HeaderMap::new(),
            viewer_jar.clone(),
            Path(library_id),
        )
        .await
        .unwrap_err();
        let detail_error = get_library_scan_job(
            State(state.clone()),
            HeaderMap::new(),
            viewer_jar.clone(),
            Path((library_id, scan_job_id)),
        )
        .await
        .unwrap_err();
        let scan_error = scan_library(State(state), HeaderMap::new(), viewer_jar, Path(library_id))
            .await
            .unwrap_err();

        assert!(matches!(
            list_error,
            ApiError::Forbidden(message) if message == "admin permission required"
        ));
        assert!(matches!(
            detail_error,
            ApiError::Forbidden(message) if message == "admin permission required"
        ));
        assert!(matches!(
            scan_error,
            ApiError::Forbidden(message) if message == "admin permission required"
        ));
    }
}
