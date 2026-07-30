use crate::{
    error::ApiError,
    response::{ok, ApiJson},
    state::AppState,
};
use axum::extract::State;
use mova_db::ping;
use serde::Serialize;

pub const HTTP_API_VERSION: u8 = 1;

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    status: &'static str,
    version: String,
    api_version: u8,
}

/// 在返回健康状态前顺便探测数据库，确保服务依赖也是可用的。
pub async fn health(State(state): State<AppState>) -> Result<ApiJson<HealthResponse>, ApiError> {
    ping(&state.db)
        .await
        .map_err(|_| ApiError::ServiceUnavailable("database unavailable".to_string()))?;

    Ok(ok(HealthResponse {
        status: "ok",
        version: state.build_version,
        api_version: HTTP_API_VERSION,
    }))
}

#[cfg(test)]
mod tests {
    use super::{HealthResponse, HTTP_API_VERSION};

    #[test]
    fn health_response_exposes_the_stable_http_contract_version() {
        let response = HealthResponse {
            status: "ok",
            version: "development".to_string(),
            api_version: HTTP_API_VERSION,
        };

        assert_eq!(
            serde_json::to_value(response).unwrap(),
            serde_json::json!({
                "status": "ok",
                "version": "development",
                "api_version": 1
            })
        );
    }
}
