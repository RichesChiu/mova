use crate::auth::AuthenticatedUser;
use crate::error::ApiError;
use crate::response::{ok, ApiJson, GlobalSearchResultResponse};
use crate::state::AppState;
use axum::extract::{Query, State};
use serde::Deserialize;

#[derive(Debug, Deserialize, Default)]
pub struct GlobalSearchQuery {
    pub q: Option<String>,
    pub limit: Option<i64>,
}

/// 搜索当前用户可见媒体库下的电影、剧集和本地集条目。
pub async fn global_search(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
    Query(query): Query<GlobalSearchQuery>,
) -> Result<ApiJson<Vec<GlobalSearchResultResponse>>, ApiError> {
    let visible_library_ids = user
        .library_visibility()
        .restricted_library_ids()
        .map(<[i64]>::to_vec);
    let results = mova_application::global_search(
        &state.db,
        mova_application::GlobalSearchInput {
            query: query.q,
            visible_library_ids,
            limit: query.limit,
        },
    )
    .await
    .map_err(ApiError::from)?;

    Ok(ok(results
        .into_iter()
        .map(|result| GlobalSearchResultResponse::from_domain(result, state.api_time_offset))
        .collect()))
}
