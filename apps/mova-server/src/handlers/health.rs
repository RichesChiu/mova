use crate::state::AppState;
use axum::{extract::State, http::StatusCode, Json};
use mova_db::ping;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    status: &'static str,
}

/// 在返回健康状态前顺便探测数据库，确保服务依赖也是可用的。
pub async fn health(State(state): State<AppState>) -> Result<Json<HealthResponse>, StatusCode> {
    ping(&state.db)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;

    Ok(Json(HealthResponse { status: "ok" }))
}
