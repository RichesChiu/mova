use crate::{
    auth::require_user,
    error::ApiError,
    handlers::realtime::load_realtime_resource_snapshot,
    response::{ok, ApiJson, HomeResponse},
    state::AppState,
};
use axum::{extract::State, http::HeaderMap};
use axum_extra::extract::cookie::CookieJar;

pub async fn get_home(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
) -> Result<ApiJson<HomeResponse>, ApiError> {
    let user = require_user(&state, &headers, &jar).await?;
    let realtime = load_realtime_resource_snapshot(&state, &user).await?;
    let visible_library_ids = user
        .library_visibility()
        .restricted_library_ids()
        .map(<[i64]>::to_vec);
    let snapshot =
        mova_application::get_home_snapshot(&state.db, user.user.id, visible_library_ids)
            .await
            .map_err(ApiError::from)?;

    Ok(ok(HomeResponse::from_domain(
        snapshot,
        user,
        state.api_time_offset,
        realtime.server_epoch,
        realtime.resources,
    )))
}
