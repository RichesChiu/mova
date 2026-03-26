use crate::handlers;
use axum::{
    routing::{get, post},
    Router,
};

/// 把媒体库配置相关接口统一挂到 `/libraries` 路径下。
pub fn router() -> Router<crate::state::AppState> {
    Router::new()
        // `GET /libraries`：查询媒体库列表。
        // `POST /libraries`：创建媒体库，成功返回 `201 Created`。
        .route(
            "/libraries",
            get(handlers::libraries::list_libraries).post(handlers::libraries::create_library),
        )
        // `GET /libraries/{id}`：查询单个媒体库详情。
        // `PATCH /libraries/{id}`：更新媒体库名称。
        // `DELETE /libraries/{id}`：删除媒体库；若后台扫描仍在停止中则返回 `409 Conflict`。
        .route(
            "/libraries/{id}",
            get(handlers::libraries::get_library)
                .patch(handlers::libraries::update_library)
                .delete(handlers::libraries::delete_library),
        )
        // `GET /libraries/{id}/media-items`：查询该库下已扫描的媒体条目。
        .route(
            "/libraries/{id}/media-items",
            get(handlers::libraries::list_library_media_items),
        )
        // `GET /libraries/{id}/scan-jobs`：查询该库的扫描历史任务。
        .route(
            "/libraries/{id}/scan-jobs",
            get(handlers::libraries::list_library_scan_jobs),
        )
        // `GET /libraries/{id}/scan-jobs/{scan_job_id}`：查询单个扫描任务状态，供前端轮询。
        .route(
            "/libraries/{id}/scan-jobs/{scan_job_id}",
            get(handlers::libraries::get_library_scan_job),
        )
        // `POST /libraries/{id}/scan`：创建扫描任务并异步执行；若已有活跃任务则直接复用并返回 `200 OK`。
        // 若媒体库正在删除，则返回 `409 Conflict`。
        .route(
            "/libraries/{id}/scan",
            post(handlers::libraries::scan_library),
        )
}
