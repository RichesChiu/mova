use crate::{
    error::ApiError,
    response::{ok, ApiJson},
    state::AppState,
};
use axum::extract::State;
use mova_db::ping;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    status: &'static str,
}

/// 在返回健康状态前顺便探测数据库，确保服务依赖也是可用的。
pub async fn health(State(state): State<AppState>) -> Result<ApiJson<HealthResponse>, ApiError> {
    ping(&state.db)
        .await
        .map_err(|_| ApiError::ServiceUnavailable("database unavailable".to_string()))?;

    Ok(ok(HealthResponse { status: "ok" }))
}
