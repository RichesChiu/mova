use crate::{
    error::{ApplicationError, ApplicationResult, AuthTokenErrorCode},
    libraries::get_library,
};
use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use mova_domain::{UserProfile, UserRole};
use rand_core::{OsRng, RngCore};
use sha2::{Digest, Sha256};
use sqlx::postgres::PgPool;
use sqlx::Error as SqlxError;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

const MIN_PASSWORD_LENGTH: usize = 8;
const MAX_USERNAME_LENGTH: usize = 254;
const MAX_NICKNAME_LENGTH: usize = 128;
const NATIVE_TOKEN_BYTES: usize = 32;

#[derive(Debug, Clone)]
pub struct CreateUserInput {
    pub username: String,
    pub password: String,
    pub role: String,
    pub is_enabled: bool,
    pub library_ids: Vec<i64>,
}

#[derive(Debug, Clone, Default)]
pub struct UpdateUserInput {
    pub role: Option<String>,
    pub is_enabled: Option<bool>,
    pub library_ids: Option<Vec<i64>>,
}

#[derive(Debug, Clone)]
pub struct LoginInput {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone)]
pub struct NativeClientLoginInput {
    pub username: String,
    pub password: String,
    pub user_agent: Option<String>,
    pub device_name: Option<String>,
    pub client_type: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RefreshNativeClientSessionInput {
    pub refresh_token: String,
}

#[derive(Debug, Clone)]
pub struct BootstrapAdminInput {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone)]
pub struct ChangeOwnPasswordInput {
    pub current_password: String,
    pub new_password: String,
}

#[derive(Debug, Clone)]
pub struct UpdateOwnProfileInput {
    pub nickname: String,
}

#[derive(Debug, Clone)]
pub struct ResetUserPasswordInput {
    pub new_password: String,
}

#[derive(Debug, Clone)]
pub struct AuthSession {
    pub user: UserProfile,
    pub token: String,
    pub expires_at: OffsetDateTime,
}

#[derive(Debug, Clone)]
pub struct NativeAuthSession {
    pub user: UserProfile,
    pub access_token: String,
    pub access_token_expires_at: OffsetDateTime,
    pub refresh_token: String,
    pub refresh_token_expires_at: OffsetDateTime,
}

pub async fn bootstrap_required(pool: &PgPool) -> ApplicationResult<bool> {
    let count = mova_db::count_admin_users(pool)
        .await
        .map_err(ApplicationError::from)?;

    Ok(count == 0)
}

pub async fn bootstrap_admin(
    pool: &PgPool,
    input: BootstrapAdminInput,
    session_ttl: Duration,
) -> ApplicationResult<AuthSession> {
    if !bootstrap_required(pool).await? {
        return Err(ApplicationError::Conflict(
            "bootstrap is no longer available because an admin account already exists".to_string(),
        ));
    }

    let username = normalize_username(input.username)?;
    let nickname = normalize_nickname(None, &username)?;
    validate_password("password", &input.password)?;
    let password_hash = hash_password(&input.password)?;

    let user = mova_db::create_user(
        pool,
        mova_db::CreateUserParams {
            username,
            nickname,
            password_hash,
            role: UserRole::Admin,
            is_enabled: true,
            library_ids: Vec::new(),
        },
    )
    .await
    .map_err(map_user_write_error)?;

    create_session_for_user(pool, enrich_user_profile(pool, user).await?, session_ttl).await
}

pub async fn login(
    pool: &PgPool,
    input: LoginInput,
    session_ttl: Duration,
) -> ApplicationResult<AuthSession> {
    let user = authenticate_login(pool, input.username, input.password).await?;
    create_session_for_user(pool, user, session_ttl).await
}

pub async fn login_native_client(
    pool: &PgPool,
    input: NativeClientLoginInput,
    access_token_ttl: Duration,
    refresh_token_ttl: Duration,
) -> ApplicationResult<NativeAuthSession> {
    let user = authenticate_login(pool, input.username, input.password).await?;
    create_native_session_for_user(
        pool,
        user,
        access_token_ttl,
        refresh_token_ttl,
        normalize_optional_text(input.user_agent),
        normalize_optional_text(input.device_name),
        normalize_client_type(input.client_type),
    )
    .await
}

pub async fn get_user_by_native_access_token(
    pool: &PgPool,
    token: &str,
) -> ApplicationResult<UserProfile> {
    let token_hash = hash_token(token);
    let Some(session) = mova_db::get_user_by_native_access_token_hash(pool, &token_hash)
        .await
        .map_err(ApplicationError::from)?
    else {
        return Err(ApplicationError::auth_token(
            AuthTokenErrorCode::InvalidToken,
        ));
    };

    if session.revoked_at.is_some() {
        return Err(ApplicationError::auth_token(
            AuthTokenErrorCode::InvalidToken,
        ));
    }

    if session.access_token_expires_at <= OffsetDateTime::now_utc() {
        return Err(ApplicationError::auth_token(
            AuthTokenErrorCode::TokenExpired,
        ));
    }

    if !session.user.user.is_enabled {
        mova_db::revoke_native_client_sessions_for_user(pool, session.user.user.id)
            .await
            .map_err(ApplicationError::from)?;
        return Err(ApplicationError::auth_token(
            AuthTokenErrorCode::InvalidToken,
        ));
    }

    mova_db::touch_native_client_session(pool, session.session_id)
        .await
        .map_err(ApplicationError::from)?;

    enrich_user_profile(pool, session.user).await
}

pub async fn refresh_native_client_session(
    pool: &PgPool,
    input: RefreshNativeClientSessionInput,
    access_token_ttl: Duration,
    refresh_token_ttl: Duration,
) -> ApplicationResult<NativeAuthSession> {
    let refresh_token = input.refresh_token.trim();
    if refresh_token.is_empty() {
        return Err(ApplicationError::auth_token(
            AuthTokenErrorCode::InvalidRefreshToken,
        ));
    }

    let refresh_token_hash = hash_token(refresh_token);
    let Some(session) =
        mova_db::get_native_client_session_by_refresh_token_hash(pool, &refresh_token_hash)
            .await
            .map_err(ApplicationError::from)?
    else {
        if let Some(used_token) = mova_db::get_used_native_refresh_token(pool, &refresh_token_hash)
            .await
            .map_err(ApplicationError::from)?
        {
            mova_db::revoke_native_client_session(pool, used_token.session_id)
                .await
                .map_err(ApplicationError::from)?;
            return Err(ApplicationError::auth_token(
                AuthTokenErrorCode::SessionRevoked,
            ));
        }

        return Err(ApplicationError::auth_token(
            AuthTokenErrorCode::InvalidRefreshToken,
        ));
    };

    if session.revoked_at.is_some() {
        return Err(ApplicationError::auth_token(
            AuthTokenErrorCode::SessionRevoked,
        ));
    }

    if session.refresh_token_expires_at <= OffsetDateTime::now_utc() {
        return Err(ApplicationError::auth_token(
            AuthTokenErrorCode::RefreshTokenExpired,
        ));
    }

    if !session.user.user.is_enabled {
        mova_db::revoke_native_client_sessions_for_user(pool, session.user.user.id)
            .await
            .map_err(ApplicationError::from)?;
        return Err(ApplicationError::auth_token(
            AuthTokenErrorCode::SessionRevoked,
        ));
    }

    let access_token = generate_native_token();
    let refresh_token = generate_native_token();
    let access_token_expires_at = OffsetDateTime::now_utc() + access_token_ttl;
    let refresh_token_expires_at = OffsetDateTime::now_utc() + refresh_token_ttl;
    let rotated = mova_db::rotate_native_client_session_tokens(
        pool,
        session.session_id,
        &refresh_token_hash,
        &hash_token(&access_token),
        &hash_token(&refresh_token),
        access_token_expires_at,
        refresh_token_expires_at,
    )
    .await
    .map_err(ApplicationError::from)?;

    if !rotated {
        return Err(ApplicationError::auth_token(
            AuthTokenErrorCode::SessionRevoked,
        ));
    }

    Ok(NativeAuthSession {
        user: enrich_user_profile(pool, session.user).await?,
        access_token,
        access_token_expires_at,
        refresh_token,
        refresh_token_expires_at,
    })
}

pub async fn logout_native_client_access_token(
    pool: &PgPool,
    token: &str,
) -> ApplicationResult<()> {
    let token_hash = hash_token(token);
    if let Some(session) = mova_db::get_user_by_native_access_token_hash(pool, &token_hash)
        .await
        .map_err(ApplicationError::from)?
    {
        mova_db::revoke_native_client_session(pool, session.session_id)
            .await
            .map_err(ApplicationError::from)?;
    }

    Ok(())
}

pub async fn logout_native_client_refresh_token(
    pool: &PgPool,
    token: &str,
) -> ApplicationResult<()> {
    let token = token.trim();
    if token.is_empty() {
        return Ok(());
    }

    let token_hash = hash_token(token);
    mova_db::revoke_native_client_session_by_refresh_token_hash(pool, &token_hash)
        .await
        .map_err(ApplicationError::from)?;

    if let Some(used_token) = mova_db::get_used_native_refresh_token(pool, &token_hash)
        .await
        .map_err(ApplicationError::from)?
    {
        mova_db::revoke_native_client_session(pool, used_token.session_id)
            .await
            .map_err(ApplicationError::from)?;
    }

    Ok(())
}

pub async fn get_user_by_session_token(
    pool: &PgPool,
    token: &str,
) -> ApplicationResult<UserProfile> {
    let Some(user) = mova_db::get_user_by_session_token(pool, token)
        .await
        .map_err(ApplicationError::from)?
    else {
        return Err(ApplicationError::Unauthorized(
            "authentication required".to_string(),
        ));
    };

    if !user.user.is_enabled {
        return Err(ApplicationError::Forbidden(format!(
            "user {} is disabled",
            user.user.username
        )));
    }

    enrich_user_profile(pool, user).await
}

pub async fn logout(pool: &PgPool, token: &str) -> ApplicationResult<()> {
    mova_db::delete_session(pool, token)
        .await
        .map_err(ApplicationError::from)?;

    Ok(())
}

pub async fn list_users(pool: &PgPool) -> ApplicationResult<Vec<UserProfile>> {
    let users = mova_db::list_users(pool)
        .await
        .map_err(ApplicationError::from)?;

    let mut enriched = Vec::with_capacity(users.len());
    for user in users {
        enriched.push(enrich_user_profile(pool, user).await?);
    }

    Ok(enriched)
}

pub async fn get_user(pool: &PgPool, user_id: i64) -> ApplicationResult<UserProfile> {
    let user = mova_db::get_user(pool, user_id)
        .await
        .map_err(ApplicationError::from)?;

    let user =
        user.ok_or_else(|| ApplicationError::NotFound(format!("user not found: {}", user_id)))?;

    enrich_user_profile(pool, user).await
}

pub async fn create_user(
    pool: &PgPool,
    actor_user_id: i64,
    input: CreateUserInput,
) -> ApplicationResult<UserProfile> {
    let username = normalize_username(input.username)?;
    let nickname = username.clone();
    validate_password("password", &input.password)?;
    let role = normalize_user_role(input.role)?;
    let actor = get_user(pool, actor_user_id).await?;
    validate_role_assignment(&actor, role)?;
    let library_ids = normalize_library_ids(input.library_ids);
    validate_library_access(pool, role, &library_ids).await?;
    let password_hash = hash_password(&input.password)?;

    let created = mova_db::create_user(
        pool,
        mova_db::CreateUserParams {
            username,
            nickname,
            password_hash,
            role,
            is_enabled: input.is_enabled,
            library_ids: if role.is_admin() {
                Vec::new()
            } else {
                library_ids
            },
        },
    )
    .await
    .map_err(map_user_write_error)?;

    enrich_user_profile(pool, created).await
}

pub async fn update_user(
    pool: &PgPool,
    actor_user_id: i64,
    user_id: i64,
    input: UpdateUserInput,
) -> ApplicationResult<UserProfile> {
    let existing = get_user(pool, user_id).await?;
    let actor = get_user(pool, actor_user_id).await?;
    validate_user_management_scope(&actor, &existing)?;

    let role = match input.role {
        Some(role) => normalize_user_role(role)?,
        None => existing.user.role,
    };
    validate_role_assignment(&actor, role)?;
    let is_enabled = input.is_enabled.unwrap_or(existing.user.is_enabled);
    validate_admin_retention(pool, &existing, role, is_enabled).await?;

    let library_ids = if role.is_admin() {
        Vec::new()
    } else {
        normalize_library_ids(input.library_ids.unwrap_or(existing.library_ids.clone()))
    };
    validate_library_access(pool, role, &library_ids).await?;

    let updated = mova_db::update_user(
        pool,
        user_id,
        mova_db::UpdateUserParams {
            role,
            is_enabled,
            library_ids,
        },
    )
    .await
    .map_err(map_user_write_error)?;

    if !updated.user.is_enabled {
        mova_db::delete_sessions_for_user(pool, updated.user.id)
            .await
            .map_err(ApplicationError::from)?;
        mova_db::revoke_native_client_sessions_for_user(pool, updated.user.id)
            .await
            .map_err(ApplicationError::from)?;
    }

    enrich_user_profile(pool, updated).await
}

pub async fn delete_user(pool: &PgPool, actor_user_id: i64, user_id: i64) -> ApplicationResult<()> {
    if actor_user_id == user_id {
        return Err(ApplicationError::Conflict(
            "current user cannot delete themselves".to_string(),
        ));
    }

    let existing = get_user(pool, user_id).await?;
    let actor = get_user(pool, actor_user_id).await?;
    validate_user_management_scope(&actor, &existing)?;
    validate_admin_retention(pool, &existing, existing.user.role, false).await?;

    let deleted = mova_db::delete_user(pool, user_id)
        .await
        .map_err(ApplicationError::from)?;

    if !deleted {
        return Err(ApplicationError::NotFound(format!(
            "user not found: {}",
            user_id
        )));
    }

    Ok(())
}

pub async fn reset_user_password(
    pool: &PgPool,
    actor_user_id: i64,
    user_id: i64,
    input: ResetUserPasswordInput,
) -> ApplicationResult<()> {
    if actor_user_id == user_id {
        return Err(ApplicationError::Conflict(
            "current user must use the personal password endpoint to change their own password"
                .to_string(),
        ));
    }

    let target_user = get_user(pool, user_id).await?;
    let actor = get_user(pool, actor_user_id).await?;
    validate_user_management_scope(&actor, &target_user)?;
    validate_password("new_password", &input.new_password)?;
    let password_hash = hash_password(&input.new_password)?;

    mova_db::update_user_password(pool, user_id, &password_hash)
        .await
        .map_err(ApplicationError::from)?;
    mova_db::delete_sessions_for_user(pool, user_id)
        .await
        .map_err(ApplicationError::from)?;
    mova_db::revoke_native_client_sessions_for_user(pool, user_id)
        .await
        .map_err(ApplicationError::from)?;

    Ok(())
}

pub async fn update_own_profile(
    pool: &PgPool,
    user_id: i64,
    input: UpdateOwnProfileInput,
) -> ApplicationResult<UserProfile> {
    let existing = get_user(pool, user_id).await?;
    let nickname = normalize_nickname(Some(input.nickname), &existing.user.username)?;

    let updated = mova_db::update_user_nickname(pool, user_id, &nickname)
        .await
        .map_err(map_user_write_error)?;

    enrich_user_profile(pool, updated).await
}

pub async fn change_own_password(
    pool: &PgPool,
    user_id: i64,
    input: ChangeOwnPasswordInput,
    session_ttl: Duration,
) -> ApplicationResult<AuthSession> {
    validate_password("new_password", &input.new_password)?;
    if input.current_password == input.new_password {
        return Err(ApplicationError::Validation(
            "new_password must be different from current_password".to_string(),
        ));
    }

    let Some(record) = mova_db::get_user_authentication_record(pool, user_id)
        .await
        .map_err(ApplicationError::from)?
    else {
        return Err(ApplicationError::NotFound(format!(
            "user not found: {}",
            user_id
        )));
    };

    if !verify_password(&record.password_hash, &input.current_password)? {
        return Err(ApplicationError::Unauthorized(
            "current password is invalid".to_string(),
        ));
    }

    let password_hash = hash_password(&input.new_password)?;
    mova_db::update_user_password(pool, user_id, &password_hash)
        .await
        .map_err(ApplicationError::from)?;
    mova_db::delete_sessions_for_user(pool, user_id)
        .await
        .map_err(ApplicationError::from)?;
    mova_db::revoke_native_client_sessions_for_user(pool, user_id)
        .await
        .map_err(ApplicationError::from)?;

    let user = get_user(pool, user_id).await?;
    create_session_for_user(pool, user, session_ttl).await
}

fn normalize_username(value: String) -> ApplicationResult<String> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(ApplicationError::Validation(
            "username cannot be empty".to_string(),
        ));
    }

    if value.chars().count() > MAX_USERNAME_LENGTH {
        return Err(ApplicationError::Validation(format!(
            "username must be at most {} characters long",
            MAX_USERNAME_LENGTH
        )));
    }

    Ok(value)
}

fn normalize_nickname(value: Option<String>, fallback_username: &str) -> ApplicationResult<String> {
    let nickname = value
        .map(|entry| entry.trim().to_string())
        .filter(|entry| !entry.is_empty())
        .unwrap_or_else(|| fallback_username.to_string());

    if nickname.chars().count() > MAX_NICKNAME_LENGTH {
        return Err(ApplicationError::Validation(format!(
            "nickname must be at most {} characters long",
            MAX_NICKNAME_LENGTH
        )));
    }

    Ok(nickname)
}

fn validate_password(field_name: &str, value: &str) -> ApplicationResult<()> {
    if value.len() < MIN_PASSWORD_LENGTH {
        return Err(ApplicationError::Validation(format!(
            "{} must be at least {} characters long",
            field_name, MIN_PASSWORD_LENGTH
        )));
    }

    Ok(())
}

fn normalize_user_role(value: String) -> ApplicationResult<UserRole> {
    match value.trim().to_ascii_lowercase().as_str() {
        "admin" => Ok(UserRole::Admin),
        "viewer" => Ok(UserRole::Viewer),
        _ => Err(ApplicationError::Validation(
            "user role must be `admin` or `viewer`".to_string(),
        )),
    }
}

fn normalize_library_ids(library_ids: Vec<i64>) -> Vec<i64> {
    let mut ids = library_ids
        .into_iter()
        .filter(|library_id| *library_id > 0)
        .collect::<Vec<_>>();
    ids.sort_unstable();
    ids.dedup();
    ids
}

async fn validate_library_access(
    pool: &PgPool,
    role: UserRole,
    library_ids: &[i64],
) -> ApplicationResult<()> {
    if role.is_admin() {
        return Ok(());
    }

    for library_id in library_ids {
        get_library(pool, *library_id).await?;
    }

    Ok(())
}

async fn is_primary_admin_user(pool: &PgPool, user_id: i64) -> ApplicationResult<bool> {
    let primary_admin_user_id = mova_db::get_primary_admin_user_id(pool)
        .await
        .map_err(ApplicationError::from)?;

    Ok(primary_admin_user_id == Some(user_id))
}

fn user_management_level(user: &UserProfile) -> u8 {
    if user.is_primary_admin {
        2
    } else if user.user.role.is_admin() {
        1
    } else {
        0
    }
}

fn validate_user_management_scope(
    actor: &UserProfile,
    target: &UserProfile,
) -> ApplicationResult<()> {
    if actor.user.id == target.user.id {
        return Err(ApplicationError::Conflict(
            "current user cannot manage themselves through user management".to_string(),
        ));
    }

    if user_management_level(actor) <= user_management_level(target) {
        return Err(ApplicationError::Forbidden(
            "users can only manage accounts with a lower privilege level".to_string(),
        ));
    }

    Ok(())
}

fn validate_role_assignment(actor: &UserProfile, next_role: UserRole) -> ApplicationResult<()> {
    if !actor.user.role.is_admin() {
        return Err(ApplicationError::Forbidden(
            "administrator permission required".to_string(),
        ));
    }

    if next_role.is_admin() && !actor.is_primary_admin {
        return Err(ApplicationError::Forbidden(
            "only the primary admin can assign administrator role".to_string(),
        ));
    }

    Ok(())
}

async fn validate_admin_retention(
    pool: &PgPool,
    existing: &UserProfile,
    next_role: UserRole,
    next_is_enabled: bool,
) -> ApplicationResult<()> {
    if !enabled_admin_is_removed(
        existing.user.role,
        existing.user.is_enabled,
        next_role,
        next_is_enabled,
    ) {
        return Ok(());
    }

    let enabled_admin_count = mova_db::count_enabled_admin_users(pool)
        .await
        .map_err(ApplicationError::from)?;
    if enabled_admin_count <= 1 {
        return Err(ApplicationError::Conflict(
            "cannot remove or disable the last enabled admin".to_string(),
        ));
    }

    Ok(())
}

fn enabled_admin_is_removed(
    existing_role: UserRole,
    existing_is_enabled: bool,
    next_role: UserRole,
    next_is_enabled: bool,
) -> bool {
    existing_role.is_admin() && existing_is_enabled && (!next_role.is_admin() || !next_is_enabled)
}

fn hash_password(password: &str) -> ApplicationResult<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|error| {
            ApplicationError::Unexpected(anyhow::anyhow!("failed to hash password: {}", error))
        })
}

fn verify_password(password_hash: &str, password: &str) -> ApplicationResult<bool> {
    let parsed_hash = PasswordHash::new(password_hash).map_err(|error| {
        ApplicationError::Unexpected(anyhow::anyhow!(
            "failed to parse stored password hash: {}",
            error
        ))
    })?;

    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
}

async fn authenticate_login(
    pool: &PgPool,
    username: String,
    password: String,
) -> ApplicationResult<UserProfile> {
    let username = normalize_username(username)?;
    validate_password("password", &password)?;

    let Some(record) = mova_db::get_user_by_username(pool, &username)
        .await
        .map_err(ApplicationError::from)?
    else {
        return Err(ApplicationError::Unauthorized(
            "invalid username or password".to_string(),
        ));
    };

    if !verify_password(&record.password_hash, &password)? {
        return Err(ApplicationError::Unauthorized(
            "invalid username or password".to_string(),
        ));
    }

    if !record.user.is_enabled {
        return Err(ApplicationError::Forbidden(format!(
            "user {} is disabled",
            record.user.username
        )));
    }

    let library_ids = mova_db::list_library_ids_for_user(pool, record.user.id)
        .await
        .map_err(ApplicationError::from)?;

    enrich_user_profile(
        pool,
        UserProfile {
            user: record.user,
            is_primary_admin: false,
            library_ids,
        },
    )
    .await
}

async fn create_session_for_user(
    pool: &PgPool,
    user: UserProfile,
    session_ttl: Duration,
) -> ApplicationResult<AuthSession> {
    let token = Uuid::new_v4().to_string();
    let expires_at = OffsetDateTime::now_utc() + session_ttl;

    mova_db::create_session(
        pool,
        mova_db::CreateSessionParams {
            token: token.clone(),
            user_id: user.user.id,
            expires_at,
        },
    )
    .await
    .map_err(ApplicationError::from)?;

    Ok(AuthSession {
        user,
        token,
        expires_at,
    })
}

async fn create_native_session_for_user(
    pool: &PgPool,
    user: UserProfile,
    access_token_ttl: Duration,
    refresh_token_ttl: Duration,
    user_agent: Option<String>,
    device_name: Option<String>,
    client_type: String,
) -> ApplicationResult<NativeAuthSession> {
    let access_token = generate_native_token();
    let refresh_token = generate_native_token();
    let access_token_expires_at = OffsetDateTime::now_utc() + access_token_ttl;
    let refresh_token_expires_at = OffsetDateTime::now_utc() + refresh_token_ttl;

    mova_db::create_native_client_session(
        pool,
        mova_db::CreateNativeClientSessionParams {
            user_id: user.user.id,
            access_token_hash: hash_token(&access_token),
            refresh_token_hash: hash_token(&refresh_token),
            access_token_expires_at,
            refresh_token_expires_at,
            user_agent,
            device_name,
            client_type,
        },
    )
    .await
    .map_err(ApplicationError::from)?;

    Ok(NativeAuthSession {
        user,
        access_token,
        access_token_expires_at,
        refresh_token,
        refresh_token_expires_at,
    })
}

fn generate_native_token() -> String {
    let mut bytes = [0_u8; NATIVE_TOKEN_BYTES];
    OsRng.fill_bytes(&mut bytes);
    hex_encode(&bytes)
}

fn hash_token(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    hex_encode(&digest)
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);

    for byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }

    encoded
}

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim();
        if value.is_empty() {
            None
        } else {
            Some(value.to_string())
        }
    })
}

fn normalize_client_type(value: Option<String>) -> String {
    normalize_optional_text(value).unwrap_or_else(|| "native".to_string())
}

async fn enrich_user_profile(
    pool: &PgPool,
    mut user: UserProfile,
) -> ApplicationResult<UserProfile> {
    user.is_primary_admin = is_primary_admin_user(pool, user.user.id).await?;
    Ok(user)
}

fn map_user_write_error(error: anyhow::Error) -> ApplicationError {
    if let Some(sqlx_error) = error.downcast_ref::<SqlxError>() {
        if is_unique_violation(sqlx_error) {
            return ApplicationError::Conflict("username already exists".to_string());
        }
    }

    ApplicationError::Unexpected(error)
}

fn is_unique_violation(error: &SqlxError) -> bool {
    match error {
        SqlxError::Database(database_error) => database_error.code().as_deref() == Some("23505"),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        enabled_admin_is_removed, normalize_library_ids, normalize_nickname, normalize_user_role,
        normalize_username, user_management_level, validate_role_assignment,
        validate_user_management_scope, MAX_USERNAME_LENGTH,
    };
    use crate::error::ApplicationError;
    use mova_domain::{User, UserProfile, UserRole};
    use time::OffsetDateTime;

    fn test_user(id: i64, role: UserRole, is_primary_admin: bool) -> UserProfile {
        UserProfile {
            user: User {
                id,
                username: format!("user-{id}"),
                nickname: format!("User {id}"),
                role,
                is_enabled: true,
                created_at: OffsetDateTime::UNIX_EPOCH,
                updated_at: OffsetDateTime::UNIX_EPOCH,
            },
            is_primary_admin,
            library_ids: Vec::new(),
        }
    }

    #[test]
    fn normalize_user_role_accepts_admin() {
        assert_eq!(
            normalize_user_role("admin".to_string()).unwrap(),
            UserRole::Admin
        );
    }

    #[test]
    fn normalize_user_role_rejects_unknown_value() {
        let error = normalize_user_role("operator".to_string()).unwrap_err();

        assert!(matches!(error, ApplicationError::Validation(_)));
    }

    #[test]
    fn normalize_library_ids_deduplicates_and_sorts() {
        assert_eq!(normalize_library_ids(vec![3, 2, 3, -1, 1]), vec![1, 2, 3]);
    }

    #[test]
    fn normalize_nickname_falls_back_to_username_when_blank() {
        assert_eq!(
            normalize_nickname(Some("   ".to_string()), "viewer01").unwrap(),
            "viewer01"
        );
    }

    #[test]
    fn normalize_username_rejects_values_over_the_account_limit() {
        let error = normalize_username("a".repeat(MAX_USERNAME_LENGTH + 1)).unwrap_err();

        assert!(matches!(error, ApplicationError::Validation(_)));
    }

    #[test]
    fn enabled_admin_is_removed_detects_demote() {
        assert!(enabled_admin_is_removed(
            UserRole::Admin,
            true,
            UserRole::Viewer,
            true
        ));
    }

    #[test]
    fn enabled_admin_is_removed_ignores_stable_admin() {
        assert!(!enabled_admin_is_removed(
            UserRole::Admin,
            true,
            UserRole::Admin,
            true
        ));
    }

    #[test]
    fn management_levels_order_primary_admin_admin_and_viewer() {
        assert_eq!(
            user_management_level(&test_user(1, UserRole::Admin, true)),
            2
        );
        assert_eq!(
            user_management_level(&test_user(2, UserRole::Admin, false)),
            1
        );
        assert_eq!(
            user_management_level(&test_user(3, UserRole::Viewer, false)),
            0
        );
    }

    #[test]
    fn user_management_requires_strictly_higher_privilege() {
        let primary_admin = test_user(1, UserRole::Admin, true);
        let admin = test_user(2, UserRole::Admin, false);
        let peer_admin = test_user(3, UserRole::Admin, false);
        let viewer = test_user(4, UserRole::Viewer, false);

        validate_user_management_scope(&primary_admin, &admin).unwrap();
        validate_user_management_scope(&admin, &viewer).unwrap();

        assert!(matches!(
            validate_user_management_scope(&admin, &peer_admin),
            Err(ApplicationError::Forbidden(_))
        ));
        assert!(matches!(
            validate_user_management_scope(&primary_admin, &primary_admin),
            Err(ApplicationError::Conflict(_))
        ));
    }

    #[test]
    fn only_primary_admin_can_assign_admin_role() {
        let primary_admin = test_user(1, UserRole::Admin, true);
        let admin = test_user(2, UserRole::Admin, false);

        validate_role_assignment(&primary_admin, UserRole::Admin).unwrap();
        let error = validate_role_assignment(&admin, UserRole::Admin).unwrap_err();

        assert!(matches!(error, ApplicationError::Forbidden(_)));
    }
}
