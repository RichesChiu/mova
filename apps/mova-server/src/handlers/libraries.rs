use crate::auth::{require_admin, require_library_access, require_user};
use crate::error::ApiError;
use crate::realtime::RealtimeEvent;
use crate::response::{
    accepted, created, ok, ok_message, with_status, ApiJson, LibraryDetailResponse,
    LibraryResponse, MediaItemListResponse, ScanJobResponse,
};
use crate::state::{AppState, BeginDeleteError, RegisterScanError};
use crate::sync_runtime;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::Deserialize;
use tokio::time::{timeout, Duration};

const LIBRARY_DELETE_SCAN_WAIT_TIMEOUT: Duration = Duration::from_secs(30);

/// 创建媒体库接口接收的请求体。
/// 这里的 root_path 对应 Plex/Jellyfin 里“这个库要扫描哪个目录”。
#[derive(Debug, Deserialize)]
pub struct CreateLibraryRequest {
    pub name: String,
    pub description: Option<String>,
    pub library_type: String,
    pub metadata_language: Option<String>,
    pub root_path: String,
    pub is_enabled: Option<bool>,
}

/// 更新媒体库接口接收的请求体。
/// 当前只支持更新基础配置，不单独提供启停能力。
#[derive(Debug, Deserialize)]
pub struct UpdateLibraryRequest {
    pub name: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ListLibraryMediaItemsQuery {
    pub query: Option<String>,
    pub year: Option<i32>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

/// 查询所有已配置的媒体库，供前端渲染列表页或设置页使用。
pub async fn list_libraries(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<ApiJson<Vec<LibraryResponse>>, ApiError> {
    let user = require_user(&state, &jar).await?;
    let libraries = mova_application::list_libraries(&state.db)
        .await
        .map_err(ApiError::from)?;

    Ok(ok(libraries
        .into_iter()
        .filter(|library| user.can_access_library(library.id))
        .map(|library| LibraryResponse::from_domain(library, state.api_time_offset))
        .collect()))
}

/// 查询单个媒体库详情。
/// 这里返回库自身信息、当前媒体数量，以及最近一次扫描摘要。
pub async fn get_library(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(library_id): Path<i64>,
) -> Result<ApiJson<LibraryDetailResponse>, ApiError> {
    let user = require_user(&state, &jar).await?;
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
    jar: CookieJar,
    Json(request): Json<CreateLibraryRequest>,
) -> Result<(StatusCode, ApiJson<LibraryResponse>), ApiError> {
    require_admin(&state, &jar).await?;
    // 把 HTTP 请求对象转换成应用层命令对象，避免业务层依赖传输协议细节。
    let input = mova_application::CreateLibraryInput {
        name: request.name,
        description: request.description,
        library_type: request.library_type,
        metadata_language: request.metadata_language,
        root_path: request.root_path,
        is_enabled: request.is_enabled.unwrap_or(true),
    };

    let library = mova_application::create_library(&state.db, input)
        .await
        .map_err(ApiError::from)?;

    maybe_enqueue_initial_library_scan(&state, library.id, library.is_enabled).await;
    sync_runtime::start_library_watcher(&state, library.clone()).await;
    state
        .realtime_hub
        .publish(RealtimeEvent::LibraryUpdated { library_id: library.id });

    Ok(created(LibraryResponse::from_domain(
        library,
        state.api_time_offset,
    )))
}

/// 更新媒体库基础配置。
/// 当前只支持修改库名，不触发重扫。
pub async fn update_library(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(library_id): Path<i64>,
    Json(request): Json<UpdateLibraryRequest>,
) -> Result<ApiJson<LibraryResponse>, ApiError> {
    require_admin(&state, &jar).await?;
    if state.scan_registry.is_deleting(library_id) {
        return Err(ApiError::Conflict(format!(
            "library {} is being deleted",
            library_id
        )));
    }

    let updated_library = mova_application::update_library(
        &state.db,
        library_id,
        mova_application::UpdateLibraryInput { name: request.name },
    )
    .await
    .map_err(ApiError::from)?;
    state
        .realtime_hub
        .publish(RealtimeEvent::LibraryUpdated { library_id });

    Ok(ok(LibraryResponse::from_domain(
        updated_library,
        state.api_time_offset,
    )))
}

/// 删除媒体库。
/// 删除前会先阻止新的扫描启动，并尽量等待当前扫描安全停止。
pub async fn delete_library(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(library_id): Path<i64>,
) -> Result<ApiJson<()>, ApiError> {
    require_admin(&state, &jar).await?;
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

        timeout(
            LIBRARY_DELETE_SCAN_WAIT_TIMEOUT,
            active_scan.wait_finished(),
        )
        .await
        .map_err(|_| {
            ApiError::Conflict(format!(
                "library {} is still stopping scan job {}, please retry shortly",
                library_id,
                active_scan.scan_job_id()
            ))
        })?;
    }

    mova_application::delete_library(&state.db, library_id)
        .await
        .map_err(ApiError::from)?;
    state.library_sync_registry.clear_library(library_id);
    state
        .realtime_hub
        .publish(RealtimeEvent::LibraryDeleted { library_id });

    Ok(ok_message("library deleted", ()))
}

/// 查询某个媒体库下已经扫描出的媒体条目。
pub async fn list_library_media_items(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(library_id): Path<i64>,
    Query(query): Query<ListLibraryMediaItemsQuery>,
) -> Result<ApiJson<MediaItemListResponse>, ApiError> {
    let user = require_user(&state, &jar).await?;
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
    jar: CookieJar,
    Path(library_id): Path<i64>,
) -> Result<ApiJson<Vec<ScanJobResponse>>, ApiError> {
    require_admin(&state, &jar).await?;
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
    jar: CookieJar,
    Path((library_id, scan_job_id)): Path<(i64, i64)>,
) -> Result<ApiJson<ScanJobResponse>, ApiError> {
    require_admin(&state, &jar).await?;
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
    jar: CookieJar,
    Path(library_id): Path<i64>,
) -> Result<(StatusCode, ApiJson<ScanJobResponse>), ApiError> {
    require_admin(&state, &jar).await?;
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
        if let Err(error) = spawn_library_scan_job(&state, library_id, enqueue_result.scan_job.id) {
            handle_scan_registration_rejected(
                &state,
                library_id,
                enqueue_result.scan_job.id,
                error,
            )
            .await;
            return Err(ApiError::Conflict(format!(
                "library {} is being deleted",
                library_id
            )));
        }
    }

    let response = ScanJobResponse::from_domain(enqueue_result.scan_job, state.api_time_offset);
    Ok(if enqueue_result.created {
        accepted(response)
    } else {
        with_status(StatusCode::OK, "ok", response)
    })
}

async fn maybe_enqueue_initial_library_scan(state: &AppState, library_id: i64, is_enabled: bool) {
    sync_runtime::maybe_enqueue_initial_library_scan(state, library_id, is_enabled).await;
}

fn spawn_library_scan_job(
    state: &AppState,
    library_id: i64,
    scan_job_id: i64,
) -> Result<(), RegisterScanError> {
    sync_runtime::spawn_library_scan_job(state, library_id, scan_job_id)
}

async fn handle_scan_registration_rejected(
    state: &AppState,
    library_id: i64,
    scan_job_id: i64,
    error: RegisterScanError,
) {
    sync_runtime::handle_scan_registration_rejected(state, library_id, scan_job_id, error).await;
}
