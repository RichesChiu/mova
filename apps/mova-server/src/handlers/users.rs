use crate::{
    auth::require_admin,
    error::ApiError,
    response::{created, ok, ok_message, ApiJson, UserResponse},
    state::AppState,
};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub nickname: Option<String>,
    pub password: String,
    pub role: String,
    pub is_enabled: Option<bool>,
    pub library_ids: Option<Vec<i64>>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateUserLibraryAccessRequest {
    pub library_ids: Vec<i64>,
}

#[derive(Debug, Deserialize, Default)]
pub struct UpdateUserRequest {
    pub username: Option<String>,
    pub nickname: Option<String>,
    pub role: Option<String>,
    pub is_enabled: Option<bool>,
    pub library_ids: Option<Vec<i64>>,
}

#[derive(Debug, Deserialize)]
pub struct ResetUserPasswordRequest {
    pub new_password: String,
}

pub async fn list_users(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
) -> Result<ApiJson<Vec<UserResponse>>, ApiError> {
    require_admin(&state, &headers, &jar).await?;

    let users = mova_application::list_users(&state.db)
        .await
        .map_err(ApiError::from)?;

    Ok(ok(users
        .into_iter()
        .map(|user| UserResponse::from_domain(user, state.api_time_offset))
        .collect()))
}

pub async fn create_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    Json(request): Json<CreateUserRequest>,
) -> Result<(StatusCode, ApiJson<UserResponse>), ApiError> {
    let current_user = require_admin(&state, &headers, &jar).await?;

    let user = mova_application::create_user(
        &state.db,
        current_user.user.id,
        mova_application::CreateUserInput {
            username: request.username,
            nickname: request.nickname,
            password: request.password,
            role: request.role,
            is_enabled: request.is_enabled.unwrap_or(true),
            library_ids: request.library_ids.unwrap_or_default(),
        },
    )
    .await
    .map_err(ApiError::from)?;

    Ok(created(UserResponse::from_domain(
        user,
        state.api_time_offset,
    )))
}

pub async fn update_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    Path(user_id): Path<i64>,
    Json(request): Json<UpdateUserRequest>,
) -> Result<ApiJson<UserResponse>, ApiError> {
    let current_user = require_admin(&state, &headers, &jar).await?;

    let user = mova_application::update_user(
        &state.db,
        current_user.user.id,
        user_id,
        mova_application::UpdateUserInput {
            username: request.username,
            nickname: request.nickname,
            role: request.role,
            is_enabled: request.is_enabled,
            library_ids: request.library_ids,
        },
    )
    .await
    .map_err(ApiError::from)?;

    Ok(ok(UserResponse::from_domain(user, state.api_time_offset)))
}

pub async fn update_user_library_access(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    Path(user_id): Path<i64>,
    Json(request): Json<UpdateUserLibraryAccessRequest>,
) -> Result<ApiJson<UserResponse>, ApiError> {
    let current_user = require_admin(&state, &headers, &jar).await?;

    let user = mova_application::replace_user_library_access(
        &state.db,
        current_user.user.id,
        user_id,
        mova_application::UpdateUserLibraryAccessInput {
            library_ids: request.library_ids,
        },
    )
    .await
    .map_err(ApiError::from)?;

    Ok(ok(UserResponse::from_domain(user, state.api_time_offset)))
}

pub async fn reset_user_password(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    Path(user_id): Path<i64>,
    Json(request): Json<ResetUserPasswordRequest>,
) -> Result<ApiJson<()>, ApiError> {
    let current_user = require_admin(&state, &headers, &jar).await?;

    mova_application::reset_user_password(
        &state.db,
        current_user.user.id,
        user_id,
        mova_application::ResetUserPasswordInput {
            new_password: request.new_password,
        },
    )
    .await
    .map_err(ApiError::from)?;

    Ok(ok_message("password reset", ()))
}

pub async fn delete_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    Path(user_id): Path<i64>,
) -> Result<ApiJson<()>, ApiError> {
    let current_user = require_admin(&state, &headers, &jar).await?;

    mova_application::delete_user(&state.db, current_user.user.id, user_id)
        .await
        .map_err(ApiError::from)?;

    Ok(ok_message("user deleted", ()))
}

#[cfg(test)]
mod tests {
    use super::{
        delete_user, update_user, update_user_library_access, UpdateUserLibraryAccessRequest,
        UpdateUserRequest,
    };
    use crate::{
        auth::{attach_session_cookie, SESSION_TTL},
        error::ApiError,
        state::{AppState, RealtimeHub, ScanRegistry},
    };
    use axum::{
        extract::{Path, State},
        http::HeaderMap,
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

    async fn seed_library(pool: &sqlx::postgres::PgPool, name: &str) -> i64 {
        mova_db::create_library(
            pool,
            mova_db::CreateLibraryParams {
                name: name.to_string(),
                description: None,
                metadata_language: "zh-CN".to_string(),
                root_path: format!("/media/{}", name.to_lowercase()),
                is_enabled: true,
            },
        )
        .await
        .unwrap()
        .id
    }

    async fn seed_user_with_session(
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

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn update_user_disabling_a_viewer_clears_their_sessions(pool: sqlx::postgres::PgPool) {
        let state = build_test_state(pool.clone());
        let library_id = seed_library(&pool, "Movies").await;
        let (_admin_id, admin_jar) = seed_user_with_session(
            &pool,
            "admin01",
            UserRole::Admin,
            Vec::new(),
            "admin-session",
        )
        .await;
        let (viewer_id, _viewer_jar) = seed_user_with_session(
            &pool,
            "viewer01",
            UserRole::Viewer,
            vec![library_id],
            "viewer-session",
        )
        .await;

        let Json(response) = update_user(
            State(state),
            HeaderMap::new(),
            admin_jar,
            Path(viewer_id),
            Json(UpdateUserRequest {
                is_enabled: Some(false),
                ..UpdateUserRequest::default()
            }),
        )
        .await
        .unwrap();

        assert_eq!(response.data.id, viewer_id);
        assert!(!response.data.is_enabled);
        assert_eq!(response.data.library_ids, vec![library_id]);
        assert!(mova_db::get_user_by_session_token(&pool, "viewer-session")
            .await
            .unwrap()
            .is_none());
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn update_user_rejects_self_disable_for_the_current_admin(pool: sqlx::postgres::PgPool) {
        let state = build_test_state(pool.clone());
        let (admin_id, admin_jar) = seed_user_with_session(
            &pool,
            "admin01",
            UserRole::Admin,
            Vec::new(),
            "admin-session",
        )
        .await;

        let error = update_user(
            State(state),
            HeaderMap::new(),
            admin_jar,
            Path(admin_id),
            Json(UpdateUserRequest {
                is_enabled: Some(false),
                ..UpdateUserRequest::default()
            }),
        )
        .await
        .unwrap_err();

        match error {
            ApiError::Conflict(message) => {
                assert_eq!(message, "current user cannot disable themselves");
            }
            other => panic!("expected conflict error, got {other:?}"),
        }
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn update_user_rejects_self_role_changes_for_the_current_admin(
        pool: sqlx::postgres::PgPool,
    ) {
        let state = build_test_state(pool.clone());
        let (admin_id, admin_jar) = seed_user_with_session(
            &pool,
            "admin01",
            UserRole::Admin,
            Vec::new(),
            "admin-session",
        )
        .await;

        let error = update_user(
            State(state),
            HeaderMap::new(),
            admin_jar,
            Path(admin_id),
            Json(UpdateUserRequest {
                role: Some("viewer".to_string()),
                ..UpdateUserRequest::default()
            }),
        )
        .await
        .unwrap_err();

        match error {
            ApiError::Conflict(message) => {
                assert_eq!(
                    message,
                    "current user cannot change their own role through user management"
                );
            }
            other => panic!("expected conflict error, got {other:?}"),
        }
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn update_user_library_access_replaces_the_viewer_scope(pool: sqlx::postgres::PgPool) {
        let state = build_test_state(pool.clone());
        let first_library_id = seed_library(&pool, "Movies").await;
        let second_library_id = seed_library(&pool, "Series").await;
        let (_admin_id, admin_jar) = seed_user_with_session(
            &pool,
            "admin01",
            UserRole::Admin,
            Vec::new(),
            "admin-session",
        )
        .await;
        let (viewer_id, _viewer_jar) = seed_user_with_session(
            &pool,
            "viewer01",
            UserRole::Viewer,
            vec![first_library_id],
            "viewer-session",
        )
        .await;

        let Json(response) = update_user_library_access(
            State(state),
            HeaderMap::new(),
            admin_jar,
            Path(viewer_id),
            Json(UpdateUserLibraryAccessRequest {
                library_ids: vec![second_library_id],
            }),
        )
        .await
        .unwrap();

        assert_eq!(response.data.id, viewer_id);
        assert_eq!(response.data.library_ids, vec![second_library_id]);
        assert_eq!(
            mova_db::list_library_ids_for_user(&pool, viewer_id)
                .await
                .unwrap(),
            vec![second_library_id]
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn delete_user_rejects_deleting_the_current_admin(pool: sqlx::postgres::PgPool) {
        let state = build_test_state(pool.clone());
        let (admin_id, admin_jar) = seed_user_with_session(
            &pool,
            "admin01",
            UserRole::Admin,
            Vec::new(),
            "admin-session",
        )
        .await;

        let error = delete_user(State(state), HeaderMap::new(), admin_jar, Path(admin_id))
            .await
            .unwrap_err();

        match error {
            ApiError::Conflict(message) => {
                assert_eq!(message, "current user cannot delete themselves");
            }
            other => panic!("expected conflict error, got {other:?}"),
        }
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn delete_user_removes_the_viewer_and_their_sessions(pool: sqlx::postgres::PgPool) {
        let state = build_test_state(pool.clone());
        let library_id = seed_library(&pool, "Movies").await;
        let (_admin_id, admin_jar) = seed_user_with_session(
            &pool,
            "admin01",
            UserRole::Admin,
            Vec::new(),
            "admin-session",
        )
        .await;
        let (viewer_id, _viewer_jar) = seed_user_with_session(
            &pool,
            "viewer01",
            UserRole::Viewer,
            vec![library_id],
            "viewer-session",
        )
        .await;

        let Json(response) =
            delete_user(State(state), HeaderMap::new(), admin_jar, Path(viewer_id))
                .await
                .unwrap();

        assert_eq!(response.message, "user deleted");
        assert!(mova_db::get_user(&pool, viewer_id).await.unwrap().is_none());
        assert!(mova_db::get_user_by_session_token(&pool, "viewer-session")
            .await
            .unwrap()
            .is_none());
    }
}
