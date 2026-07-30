use anyhow::{bail, Context, Result};
use mova_domain::{User, UserProfile, UserRole};
use sqlx::{
    postgres::{PgPool, PgRow},
    Row,
};
use time::OffsetDateTime;

#[derive(Debug, Clone)]
pub struct CreateUserParams {
    pub username: String,
    pub username_normalized: String,
    pub nickname: String,
    pub password_hash: String,
    pub role: UserRole,
    pub is_enabled: bool,
    pub library_ids: Vec<i64>,
}

#[derive(Debug, Clone)]
pub struct UpdateUserParams {
    pub role: UserRole,
    pub is_enabled: bool,
    pub library_ids: Vec<i64>,
}

#[derive(Debug, Clone)]
pub struct CreateSessionParams {
    pub token_hash: String,
    pub user_id: i64,
    pub expires_at: OffsetDateTime,
}

#[derive(Debug, Clone)]
pub struct UserSessionUser {
    pub user: UserProfile,
    pub expires_at: OffsetDateTime,
}

#[derive(Debug, Clone)]
pub struct CreateNativeClientSessionParams {
    pub user_id: i64,
    pub access_token_hash: String,
    pub refresh_token_hash: String,
    pub access_token_expires_at: OffsetDateTime,
    pub refresh_token_expires_at: OffsetDateTime,
    pub user_agent: Option<String>,
    pub device_name: Option<String>,
    pub client_type: String,
}

#[derive(Debug, Clone)]
pub struct NativeClientSessionUser {
    pub session_id: i64,
    pub user: UserProfile,
    pub access_token_expires_at: OffsetDateTime,
    pub refresh_token_expires_at: OffsetDateTime,
    pub revoked_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone)]
pub struct UsedNativeRefreshToken {
    pub session_id: i64,
    pub expires_at: OffsetDateTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeClientTokenRotationOutcome {
    Rotated,
    Replayed,
    Expired,
    Missing,
}

#[derive(Debug)]
pub struct RotateNativeClientSessionTokensParams<'a> {
    pub session_id: i64,
    pub old_refresh_token_hash: &'a str,
    pub old_refresh_token_expires_at: OffsetDateTime,
    pub new_access_token_hash: &'a str,
    pub new_refresh_token_hash: &'a str,
    pub access_token_expires_at: OffsetDateTime,
    pub refresh_token_expires_at: OffsetDateTime,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AuthSessionCleanupOutcome {
    pub lock_acquired: bool,
    pub deleted_user_sessions: u64,
    pub deleted_native_sessions: u64,
    pub deleted_used_refresh_tokens: u64,
}

impl AuthSessionCleanupOutcome {
    pub fn reached_batch_limit(self, batch_size: i64) -> bool {
        let Ok(batch_size) = u64::try_from(batch_size) else {
            return false;
        };
        if batch_size == 0 {
            return false;
        }

        self.deleted_user_sessions >= batch_size
            || self.deleted_native_sessions >= batch_size
            || self.deleted_used_refresh_tokens >= batch_size
    }
}

#[derive(Debug, Clone)]
pub struct UserAuthenticationRecord {
    pub user: User,
    pub password_hash: String,
    pub library_ids: Vec<i64>,
}

pub async fn count_admin_users(pool: &PgPool) -> Result<i64> {
    let row = sqlx::query(
        r#"
        select count(*) as total
        from users
        where role in ('owner', 'admin')
        "#,
    )
    .fetch_one(pool)
    .await
    .context("failed to count admin users")?;

    Ok(row.get("total"))
}

pub async fn count_enabled_admin_users(pool: &PgPool) -> Result<i64> {
    let row = sqlx::query(
        r#"
        select count(*) as total
        from users
        where role in ('owner', 'admin')
          and is_enabled = true
        "#,
    )
    .fetch_one(pool)
    .await
    .context("failed to count enabled admin users")?;

    Ok(row.get("total"))
}

pub async fn list_users(pool: &PgPool) -> Result<Vec<UserProfile>> {
    let rows = sqlx::query(
        r#"
        select
            u.id,
            u.username,
            u.nickname,
            u.role,
            u.is_enabled,
            u.created_at,
            u.updated_at,
            coalesce(
                array_agg(access.library_id order by access.library_id)
                    filter (where access.library_id is not null),
                array[]::bigint[]
            ) as library_ids
        from users u
        left join user_library_access access on access.user_id = u.id
        group by u.id
        order by u.created_at asc, u.id asc
        "#,
    )
    .fetch_all(pool)
    .await
    .context("failed to list users")?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let library_ids = row.get("library_ids");
            let user = map_user_row(row);
            UserProfile { user, library_ids }
        })
        .collect())
}

pub async fn get_user(pool: &PgPool, user_id: i64) -> Result<Option<UserProfile>> {
    let row = sqlx::query(
        r#"
        select
            u.id,
            u.username,
            u.nickname,
            u.role,
            u.is_enabled,
            u.created_at,
            u.updated_at,
            coalesce(
                (
                    select array_agg(access.library_id order by access.library_id)
                    from user_library_access access
                    where access.user_id = u.id
                ),
                array[]::bigint[]
            ) as library_ids
        from users u
        where u.id = $1
        "#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .context("failed to get user")?;

    let Some(row) = row else {
        return Ok(None);
    };

    let library_ids = row.get("library_ids");
    let user = map_user_row(row);

    Ok(Some(UserProfile { user, library_ids }))
}

pub async fn get_user_by_username(
    pool: &PgPool,
    username: &str,
) -> Result<Option<UserAuthenticationRecord>> {
    let row = sqlx::query(
        r#"
        select
            u.id,
            u.username,
            u.nickname,
            u.password_hash,
            u.role,
            u.is_enabled,
            u.created_at,
            u.updated_at,
            coalesce(
                (
                    select array_agg(access.library_id order by access.library_id)
                    from user_library_access access
                    where access.user_id = u.id
                ),
                array[]::bigint[]
            ) as library_ids
        from users u
        where u.username_normalized = $1
        "#,
    )
    .bind(username)
    .fetch_optional(pool)
    .await
    .context("failed to get user by username")?;

    Ok(row.map(|row| UserAuthenticationRecord {
        password_hash: row.get("password_hash"),
        library_ids: row.get("library_ids"),
        user: User {
            id: row.get("id"),
            username: row.get("username"),
            nickname: row.get("nickname"),
            role: parse_user_role(row.get::<String, _>("role").as_str()),
            is_enabled: row.get("is_enabled"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        },
    }))
}

pub async fn get_user_authentication_record(
    pool: &PgPool,
    user_id: i64,
) -> Result<Option<UserAuthenticationRecord>> {
    let row = sqlx::query(
        r#"
        select
            u.id,
            u.username,
            u.nickname,
            u.password_hash,
            u.role,
            u.is_enabled,
            u.created_at,
            u.updated_at,
            coalesce(
                (
                    select array_agg(access.library_id order by access.library_id)
                    from user_library_access access
                    where access.user_id = u.id
                ),
                array[]::bigint[]
            ) as library_ids
        from users u
        where u.id = $1
        "#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .context("failed to get user authentication record")?;

    Ok(row.map(|row| UserAuthenticationRecord {
        password_hash: row.get("password_hash"),
        library_ids: row.get("library_ids"),
        user: User {
            id: row.get("id"),
            username: row.get("username"),
            nickname: row.get("nickname"),
            role: parse_user_role(row.get::<String, _>("role").as_str()),
            is_enabled: row.get("is_enabled"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        },
    }))
}

pub async fn create_user(pool: &PgPool, params: CreateUserParams) -> Result<UserProfile> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to start user creation transaction")?;

    let row = sqlx::query(
        r#"
        insert into users (
            username,
            username_normalized,
            nickname,
            password_hash,
            role,
            is_enabled
        )
        values ($1, $2, $3, $4, $5, $6)
        returning id, username, nickname, role, is_enabled, created_at, updated_at
        "#,
    )
    .bind(params.username)
    .bind(params.username_normalized)
    .bind(params.nickname)
    .bind(params.password_hash)
    .bind(params.role.as_str())
    .bind(params.is_enabled)
    .fetch_one(&mut *tx)
    .await
    .context("failed to create user")?;

    let user = map_user_row(row);
    write_user_library_access(&mut tx, user.id, &params.library_ids).await?;

    tx.commit()
        .await
        .context("failed to commit user creation transaction")?;

    Ok(UserProfile {
        user,
        library_ids: params.library_ids,
    })
}

pub async fn update_user(
    pool: &PgPool,
    user_id: i64,
    params: UpdateUserParams,
) -> Result<UserProfile> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to start user update transaction")?;

    let row = sqlx::query(
        r#"
        update users
        set role = $2,
            is_enabled = $3,
            updated_at = now()
        where id = $1
        returning id, username, nickname, role, is_enabled, created_at, updated_at
        "#,
    )
    .bind(user_id)
    .bind(params.role.as_str())
    .bind(params.is_enabled)
    .fetch_one(&mut *tx)
    .await
    .context("failed to update user")?;

    sqlx::query("delete from user_library_access where user_id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .context("failed to clear existing user library access during update")?;

    write_user_library_access(&mut tx, user_id, &params.library_ids).await?;

    if !params.is_enabled {
        sqlx::query("delete from user_sessions where user_id = $1")
            .bind(user_id)
            .execute(&mut *tx)
            .await
            .context("failed to revoke disabled user's sessions")?;
        sqlx::query(
            r#"
            update native_client_sessions
            set revoked_at = coalesce(revoked_at, clock_timestamp())
            where user_id = $1
            "#,
        )
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .context("failed to revoke disabled user's native client sessions")?;
    }

    tx.commit()
        .await
        .context("failed to commit user update transaction")?;

    Ok(UserProfile {
        user: map_user_row(row),
        library_ids: params.library_ids,
    })
}

pub async fn update_user_nickname(
    pool: &PgPool,
    user_id: i64,
    nickname: &str,
) -> Result<UserProfile> {
    let row = sqlx::query(
        r#"
        update users
        set nickname = $2,
            updated_at = now()
        where id = $1
        returning id, username, nickname, role, is_enabled, created_at, updated_at
        "#,
    )
    .bind(user_id)
    .bind(nickname)
    .fetch_one(pool)
    .await
    .context("failed to update user nickname")?;

    let user = map_user_row(row);
    let library_ids = list_library_ids_for_user(pool, user.id).await?;

    Ok(UserProfile { user, library_ids })
}

pub async fn update_user_password_and_revoke_sessions(
    pool: &PgPool,
    user_id: i64,
    password_hash: &str,
    replacement_session: Option<CreateSessionParams>,
) -> Result<()> {
    if replacement_session
        .as_ref()
        .is_some_and(|session| session.user_id != user_id)
    {
        anyhow::bail!("replacement session must belong to the password owner");
    }

    let mut tx = pool
        .begin()
        .await
        .context("failed to begin password replacement transaction")?;

    let updated = sqlx::query(
        r#"
        update users
        set password_hash = $2,
            updated_at = now()
        where id = $1
        "#,
    )
    .bind(user_id)
    .bind(password_hash)
    .execute(&mut *tx)
    .await
    .context("failed to update user password")?
    .rows_affected();
    if updated != 1 {
        anyhow::bail!("user not found while replacing password: {user_id}");
    }

    sqlx::query(
        r#"
        delete from user_sessions
        where user_id = $1
        "#,
    )
    .bind(user_id)
    .execute(&mut *tx)
    .await
    .context("failed to revoke user sessions after password replacement")?;

    sqlx::query(
        r#"
        update native_client_sessions
        set revoked_at = coalesce(revoked_at, clock_timestamp())
        where user_id = $1
        "#,
    )
    .bind(user_id)
    .execute(&mut *tx)
    .await
    .context("failed to revoke native client sessions after password replacement")?;

    if let Some(session) = replacement_session {
        sqlx::query(
            r#"
            insert into user_sessions (token_hash, user_id, expires_at)
            values ($1, $2, $3)
            "#,
        )
        .bind(session.token_hash)
        .bind(session.user_id)
        .bind(session.expires_at)
        .execute(&mut *tx)
        .await
        .context("failed to create replacement user session")?;
    }

    tx.commit()
        .await
        .context("failed to commit password replacement transaction")?;

    Ok(())
}

pub async fn list_library_ids_for_user(pool: &PgPool, user_id: i64) -> Result<Vec<i64>> {
    let rows = sqlx::query(
        r#"
        select library_id
        from user_library_access
        where user_id = $1
        order by library_id asc
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .context("failed to list user library access")?;

    Ok(rows
        .into_iter()
        .map(|row| row.get::<i64, _>("library_id"))
        .collect())
}

pub async fn create_session(pool: &PgPool, params: CreateSessionParams) -> Result<()> {
    sqlx::query(
        r#"
        insert into user_sessions (token_hash, user_id, expires_at)
        values ($1, $2, $3)
        "#,
    )
    .bind(params.token_hash)
    .bind(params.user_id)
    .bind(params.expires_at)
    .execute(pool)
    .await
    .context("failed to create user session")?;

    Ok(())
}

pub async fn get_user_by_session_token_hash(
    pool: &PgPool,
    token_hash: &str,
) -> Result<Option<UserSessionUser>> {
    let row = sqlx::query(
        r#"
        select
            u.id,
            u.username,
            u.nickname,
            u.role,
            u.is_enabled,
            u.created_at,
            u.updated_at,
            s.expires_at as session_expires_at,
            coalesce(
                (
                    select array_agg(access.library_id order by access.library_id)
                    from user_library_access access
                    where access.user_id = u.id
                ),
                array[]::bigint[]
            ) as library_ids
        from user_sessions s
        join users u on u.id = s.user_id
        where s.token_hash = $1
          and s.expires_at > now()
        "#,
    )
    .bind(token_hash)
    .fetch_optional(pool)
    .await
    .context("failed to get user by session token")?;

    let Some(row) = row else {
        return Ok(None);
    };

    sqlx::query(
        r#"
        update user_sessions
        set last_seen_at = clock_timestamp()
        where token_hash = $1
          and last_seen_at <= clock_timestamp() - interval '5 minutes'
        "#,
    )
    .bind(token_hash)
    .execute(pool)
    .await
    .context("failed to update user session last_seen_at")?;

    let expires_at = row.get("session_expires_at");
    let library_ids = row.get("library_ids");
    let user = map_user_row(row);

    Ok(Some(UserSessionUser {
        user: UserProfile { user, library_ids },
        expires_at,
    }))
}

pub async fn delete_session_by_token_hash(pool: &PgPool, token_hash: &str) -> Result<()> {
    sqlx::query(
        r#"
        delete from user_sessions
        where token_hash = $1
        "#,
    )
    .bind(token_hash)
    .execute(pool)
    .await
    .context("failed to delete user session")?;

    Ok(())
}

pub async fn create_native_client_session(
    pool: &PgPool,
    params: CreateNativeClientSessionParams,
) -> Result<()> {
    sqlx::query(
        r#"
        insert into native_client_sessions (
            user_id,
            access_token_hash,
            refresh_token_hash,
            access_token_expires_at,
            refresh_token_expires_at,
            user_agent,
            device_name,
            client_type
        )
        values ($1, $2, $3, $4, $5, $6, $7, $8)
        "#,
    )
    .bind(params.user_id)
    .bind(params.access_token_hash)
    .bind(params.refresh_token_hash)
    .bind(params.access_token_expires_at)
    .bind(params.refresh_token_expires_at)
    .bind(params.user_agent)
    .bind(params.device_name)
    .bind(params.client_type)
    .execute(pool)
    .await
    .context("failed to create native client session")?;

    Ok(())
}

pub async fn get_user_by_native_access_token_hash(
    pool: &PgPool,
    token_hash: &str,
) -> Result<Option<NativeClientSessionUser>> {
    let query = native_client_session_user_select_sql("s.access_token_hash = $1");
    let row = sqlx::query(&query)
        .bind(token_hash)
        .fetch_optional(pool)
        .await
        .context("failed to get native client session by access token hash")?;

    Ok(map_native_client_session_user(row))
}

pub async fn get_native_client_session_by_refresh_token_hash(
    pool: &PgPool,
    token_hash: &str,
) -> Result<Option<NativeClientSessionUser>> {
    let query = native_client_session_user_select_sql("s.refresh_token_hash = $1");
    let row = sqlx::query(&query)
        .bind(token_hash)
        .fetch_optional(pool)
        .await
        .context("failed to get native client session by refresh token hash")?;

    Ok(map_native_client_session_user(row))
}

pub async fn get_used_native_refresh_token(
    pool: &PgPool,
    token_hash: &str,
) -> Result<Option<UsedNativeRefreshToken>> {
    let row = sqlx::query(
        r#"
        select session_id, expires_at
        from native_client_used_refresh_tokens
        where token_hash = $1
        "#,
    )
    .bind(token_hash)
    .fetch_optional(pool)
    .await
    .context("failed to get used native refresh token")?;

    Ok(row.map(|row| UsedNativeRefreshToken {
        session_id: row.get("session_id"),
        expires_at: row.get("expires_at"),
    }))
}

pub async fn touch_native_client_session(pool: &PgPool, session_id: i64) -> Result<()> {
    sqlx::query(
        r#"
        update native_client_sessions
        set last_used_at = clock_timestamp()
        where id = $1
          and last_used_at <= clock_timestamp() - interval '5 minutes'
        "#,
    )
    .bind(session_id)
    .execute(pool)
    .await
    .context("failed to touch native client session")?;

    Ok(())
}

pub async fn rotate_native_client_session_tokens(
    pool: &PgPool,
    params: RotateNativeClientSessionTokensParams<'_>,
) -> Result<NativeClientTokenRotationOutcome> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to begin native token rotation")?;

    let result = sqlx::query(
        r#"
        update native_client_sessions
        set access_token_hash = $3,
            refresh_token_hash = $4,
            access_token_expires_at = $5,
            refresh_token_expires_at = $6,
            last_used_at = clock_timestamp()
        where id = $1
          and refresh_token_hash = $2
          and revoked_at is null
          and refresh_token_expires_at > clock_timestamp()
        "#,
    )
    .bind(params.session_id)
    .bind(params.old_refresh_token_hash)
    .bind(params.new_access_token_hash)
    .bind(params.new_refresh_token_hash)
    .bind(params.access_token_expires_at)
    .bind(params.refresh_token_expires_at)
    .execute(&mut *tx)
    .await
    .context("failed to rotate native client session tokens")?;

    let outcome = if result.rows_affected() == 1 {
        sqlx::query(
            r#"
            insert into native_client_used_refresh_tokens (token_hash, session_id, expires_at)
            select $1, $2, $3
            where $3 > clock_timestamp()
            on conflict (token_hash) do nothing
            "#,
        )
        .bind(params.old_refresh_token_hash)
        .bind(params.session_id)
        .bind(params.old_refresh_token_expires_at)
        .execute(&mut *tx)
        .await
        .context("failed to record used native refresh token")?;

        NativeClientTokenRotationOutcome::Rotated
    } else {
        let current = sqlx::query(
            r#"
            select
                refresh_token_hash,
                refresh_token_expires_at <= clock_timestamp() as refresh_token_expired,
                revoked_at
            from native_client_sessions
            where id = $1
            for update
            "#,
        )
        .bind(params.session_id)
        .fetch_optional(&mut *tx)
        .await
        .context("failed to inspect failed native token rotation")?;

        match current {
            None => NativeClientTokenRotationOutcome::Missing,
            Some(row)
                if row.get::<String, _>("refresh_token_hash") == params.old_refresh_token_hash
                    && row.get::<bool, _>("refresh_token_expired")
                    && row.get::<Option<OffsetDateTime>, _>("revoked_at").is_none() =>
            {
                NativeClientTokenRotationOutcome::Expired
            }
            Some(_) => {
                sqlx::query(
                    r#"
                    update native_client_sessions
                    set revoked_at = coalesce(revoked_at, clock_timestamp())
                    where id = $1
                    "#,
                )
                .bind(params.session_id)
                .execute(&mut *tx)
                .await
                .context("failed to revoke replayed native client session")?;

                NativeClientTokenRotationOutcome::Replayed
            }
        }
    };

    tx.commit()
        .await
        .context("failed to commit native token rotation")?;

    Ok(outcome)
}

pub async fn revoke_native_client_session(pool: &PgPool, session_id: i64) -> Result<()> {
    sqlx::query(
        r#"
        update native_client_sessions
        set revoked_at = coalesce(revoked_at, clock_timestamp())
        where id = $1
        "#,
    )
    .bind(session_id)
    .execute(pool)
    .await
    .context("failed to revoke native client session")?;

    Ok(())
}

pub async fn revoke_native_client_session_by_refresh_token_hash(
    pool: &PgPool,
    token_hash: &str,
) -> Result<()> {
    sqlx::query(
        r#"
        update native_client_sessions
        set revoked_at = coalesce(revoked_at, clock_timestamp())
        where refresh_token_hash = $1
        "#,
    )
    .bind(token_hash)
    .execute(pool)
    .await
    .context("failed to revoke native client session by refresh token hash")?;

    Ok(())
}

pub async fn revoke_native_client_sessions_for_user(pool: &PgPool, user_id: i64) -> Result<()> {
    sqlx::query(
        r#"
        update native_client_sessions
        set revoked_at = coalesce(revoked_at, clock_timestamp())
        where user_id = $1
        "#,
    )
    .bind(user_id)
    .execute(pool)
    .await
    .context("failed to revoke native client sessions for user")?;

    Ok(())
}

pub async fn cleanup_auth_sessions(
    pool: &PgPool,
    batch_size: i64,
    revoked_retention_seconds: i64,
) -> Result<AuthSessionCleanupOutcome> {
    if batch_size <= 0 {
        bail!("auth session cleanup batch size must be positive");
    }
    if revoked_retention_seconds < 0 {
        bail!("revoked auth session retention cannot be negative");
    }

    const AUTH_SESSION_CLEANUP_LOCK_KEY: i64 = 0x4D4F_5641_4155_5448;

    let mut tx = pool
        .begin()
        .await
        .context("failed to begin auth session cleanup")?;
    let lock_acquired: bool = sqlx::query_scalar("select pg_try_advisory_xact_lock($1)")
        .bind(AUTH_SESSION_CLEANUP_LOCK_KEY)
        .fetch_one(&mut *tx)
        .await
        .context("failed to acquire auth session cleanup lock")?;

    if !lock_acquired {
        tx.commit()
            .await
            .context("failed to finish skipped auth session cleanup")?;
        return Ok(AuthSessionCleanupOutcome {
            lock_acquired: false,
            ..AuthSessionCleanupOutcome::default()
        });
    }

    let deleted_user_sessions = sqlx::query(
        r#"
        with candidates as (
            select token_hash
            from user_sessions
            where expires_at <= clock_timestamp()
            order by expires_at, token_hash
            limit $1
            for update skip locked
        )
        delete from user_sessions sessions
        using candidates
        where sessions.token_hash = candidates.token_hash
        "#,
    )
    .bind(batch_size)
    .execute(&mut *tx)
    .await
    .context("failed to delete expired user sessions")?
    .rows_affected();

    let deleted_native_sessions = sqlx::query(
        r#"
        with candidates as (
            select id
            from native_client_sessions
            where refresh_token_expires_at <= clock_timestamp()
               or (
                    revoked_at is not null
                    and revoked_at <= clock_timestamp()
                        - make_interval(secs => $2::double precision)
               )
            order by coalesce(revoked_at, refresh_token_expires_at), id
            limit $1
            for update skip locked
        )
        delete from native_client_sessions sessions
        using candidates
        where sessions.id = candidates.id
        "#,
    )
    .bind(batch_size)
    .bind(revoked_retention_seconds)
    .execute(&mut *tx)
    .await
    .context("failed to delete expired or retired native client sessions")?
    .rows_affected();

    let deleted_used_refresh_tokens = sqlx::query(
        r#"
        with candidates as (
            select token_hash
            from native_client_used_refresh_tokens
            where expires_at <= clock_timestamp()
            order by expires_at, token_hash
            limit $1
            for update skip locked
        )
        delete from native_client_used_refresh_tokens tokens
        using candidates
        where tokens.token_hash = candidates.token_hash
        "#,
    )
    .bind(batch_size)
    .execute(&mut *tx)
    .await
    .context("failed to delete expired used native refresh tokens")?
    .rows_affected();

    tx.commit()
        .await
        .context("failed to commit auth session cleanup")?;

    Ok(AuthSessionCleanupOutcome {
        lock_acquired: true,
        deleted_user_sessions,
        deleted_native_sessions,
        deleted_used_refresh_tokens,
    })
}

pub async fn delete_user(pool: &PgPool, user_id: i64) -> Result<bool> {
    let result = sqlx::query(
        r#"
        delete from users
        where id = $1
        "#,
    )
    .bind(user_id)
    .execute(pool)
    .await
    .context("failed to delete user")?;

    Ok(result.rows_affected() > 0)
}

async fn write_user_library_access(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: i64,
    library_ids: &[i64],
) -> Result<()> {
    for library_id in library_ids {
        sqlx::query(
            r#"
            insert into user_library_access (user_id, library_id)
            values ($1, $2)
            "#,
        )
        .bind(user_id)
        .bind(*library_id)
        .execute(&mut **tx)
        .await
        .with_context(|| {
            format!(
                "failed to grant library {} access to user {}",
                library_id, user_id
            )
        })?;
    }

    Ok(())
}

fn map_user_row(row: PgRow) -> User {
    User {
        id: row.get("id"),
        username: row.get("username"),
        nickname: row.get("nickname"),
        role: parse_user_role(row.get::<String, _>("role").as_str()),
        is_enabled: row.get("is_enabled"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn native_client_session_user_select_sql(predicate: &str) -> String {
    format!(
        r#"
        select
            s.id as session_id,
            s.access_token_expires_at,
            s.refresh_token_expires_at,
            s.revoked_at,
            u.id,
            u.username,
            u.nickname,
            u.role,
            u.is_enabled,
            u.created_at,
            u.updated_at,
            coalesce(
                (
                    select array_agg(access.library_id order by access.library_id)
                    from user_library_access access
                    where access.user_id = u.id
                ),
                array[]::bigint[]
            ) as library_ids
        from native_client_sessions s
        join users u on u.id = s.user_id
        where {}
        "#,
        predicate
    )
}

fn map_native_client_session_user(row: Option<PgRow>) -> Option<NativeClientSessionUser> {
    let row = row?;

    let session_id = row.get("session_id");
    let access_token_expires_at = row.get("access_token_expires_at");
    let refresh_token_expires_at = row.get("refresh_token_expires_at");
    let revoked_at = row.get("revoked_at");
    let library_ids = row.get("library_ids");
    let user = map_user_row(row);

    Some(NativeClientSessionUser {
        session_id,
        user: UserProfile { user, library_ids },
        access_token_expires_at,
        refresh_token_expires_at,
        revoked_at,
    })
}

fn parse_user_role(value: &str) -> UserRole {
    match value {
        "owner" => UserRole::Owner,
        "admin" => UserRole::Admin,
        "viewer" => UserRole::Viewer,
        other => panic!("unexpected user role in database: {}", other),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        cleanup_auth_sessions, get_user_by_session_token_hash, rotate_native_client_session_tokens,
        touch_native_client_session, update_user_password_and_revoke_sessions,
        AuthSessionCleanupOutcome, NativeClientTokenRotationOutcome,
        RotateNativeClientSessionTokensParams,
    };
    use sqlx::{PgPool, Row};
    use time::{Duration, OffsetDateTime};

    async fn seed_user(pool: &PgPool, username: &str) -> i64 {
        sqlx::query_scalar(
            r#"
            insert into users (
                username,
                username_normalized,
                nickname,
                password_hash,
                role
            )
            values ($1, lower($1), $1, 'hash', 'viewer')
            returning id
            "#,
        )
        .bind(username)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    #[test]
    fn cleanup_outcome_requests_another_batch_when_any_category_reaches_the_limit() {
        assert!(AuthSessionCleanupOutcome {
            lock_acquired: true,
            deleted_user_sessions: 10,
            ..AuthSessionCleanupOutcome::default()
        }
        .reached_batch_limit(10));
        assert!(!AuthSessionCleanupOutcome {
            lock_acquired: true,
            deleted_user_sessions: 9,
            deleted_native_sessions: 9,
            deleted_used_refresh_tokens: 9,
        }
        .reached_batch_limit(10));
        assert!(!AuthSessionCleanupOutcome::default().reached_batch_limit(0));
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn password_replacement_rolls_back_when_replacement_session_fails(pool: PgPool) {
        let user_id = seed_user(&pool, "password-rollback-user").await;
        let now = OffsetDateTime::now_utc();
        let web_hash = "a".repeat(64);
        sqlx::query(
            r#"
            insert into user_sessions (token_hash, user_id, expires_at)
            values ($1, $2, $3)
            "#,
        )
        .bind(&web_hash)
        .bind(user_id)
        .bind(now + Duration::hours(1))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            insert into native_client_sessions (
                user_id,
                access_token_hash,
                refresh_token_hash,
                access_token_expires_at,
                refresh_token_expires_at
            )
            values ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(user_id)
        .bind("b".repeat(64))
        .bind("c".repeat(64))
        .bind(now + Duration::hours(1))
        .bind(now + Duration::hours(2))
        .execute(&pool)
        .await
        .unwrap();
        let error = update_user_password_and_revoke_sessions(
            &pool,
            user_id,
            "replacement-password-hash",
            Some(super::CreateSessionParams {
                token_hash: "not-a-valid-sha256-token-hash".to_string(),
                user_id,
                expires_at: now + Duration::hours(1),
            }),
        )
        .await
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("failed to create replacement user session"));

        let password_hash: String =
            sqlx::query_scalar("select password_hash from users where id = $1")
                .bind(user_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(password_hash, "hash");
        let web_session_count: i64 =
            sqlx::query_scalar("select count(*) from user_sessions where token_hash = $1")
                .bind(web_hash)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(web_session_count, 1);
        let native_revoked_at: Option<OffsetDateTime> =
            sqlx::query_scalar("select revoked_at from native_client_sessions where user_id = $1")
                .bind(user_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(native_revoked_at.is_none());
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn auth_cleanup_is_batched_and_preserves_live_sessions(pool: PgPool) {
        let user_id = seed_user(&pool, "cleanup-user").await;
        let now = OffsetDateTime::now_utc();
        let expired_web_hash = "1".repeat(64);
        let active_web_hash = "2".repeat(64);

        sqlx::query(
            r#"
            insert into user_sessions (
                token_hash,
                user_id,
                expires_at,
                created_at,
                last_seen_at
            )
            values
                ($1, $3, $4, $5, $5),
                ($2, $3, $6, $5, $5)
            "#,
        )
        .bind(&expired_web_hash)
        .bind(&active_web_hash)
        .bind(user_id)
        .bind(now - Duration::hours(1))
        .bind(now - Duration::hours(2))
        .bind(now + Duration::hours(1))
        .execute(&pool)
        .await
        .unwrap();

        let expired_native_id: i64 = sqlx::query_scalar(
            r#"
            insert into native_client_sessions (
                user_id,
                access_token_hash,
                refresh_token_hash,
                access_token_expires_at,
                refresh_token_expires_at,
                created_at,
                last_used_at
            )
            values ($1, $2, $3, $4, $5, $6, $6)
            returning id
            "#,
        )
        .bind(user_id)
        .bind("3".repeat(64))
        .bind("4".repeat(64))
        .bind(now - Duration::hours(2))
        .bind(now - Duration::hours(1))
        .bind(now - Duration::hours(3))
        .fetch_one(&pool)
        .await
        .unwrap();

        let revoked_native_id: i64 = sqlx::query_scalar(
            r#"
            insert into native_client_sessions (
                user_id,
                access_token_hash,
                refresh_token_hash,
                access_token_expires_at,
                refresh_token_expires_at,
                revoked_at,
                created_at,
                last_used_at
            )
            values ($1, $2, $3, $4, $5, $6, $7, $7)
            returning id
            "#,
        )
        .bind(user_id)
        .bind("5".repeat(64))
        .bind("6".repeat(64))
        .bind(now + Duration::hours(1))
        .bind(now + Duration::hours(2))
        .bind(now - Duration::days(8))
        .bind(now - Duration::days(10))
        .fetch_one(&pool)
        .await
        .unwrap();

        let active_native_id: i64 = sqlx::query_scalar(
            r#"
            insert into native_client_sessions (
                user_id,
                access_token_hash,
                refresh_token_hash,
                access_token_expires_at,
                refresh_token_expires_at
            )
            values ($1, $2, $3, $4, $5)
            returning id
            "#,
        )
        .bind(user_id)
        .bind("7".repeat(64))
        .bind("8".repeat(64))
        .bind(now + Duration::hours(1))
        .bind(now + Duration::hours(2))
        .fetch_one(&pool)
        .await
        .unwrap();

        sqlx::query(
            r#"
            insert into native_client_used_refresh_tokens (
                token_hash,
                session_id,
                expires_at,
                created_at
            )
            values ($1, $2, $3, $4)
            "#,
        )
        .bind("9".repeat(64))
        .bind(active_native_id)
        .bind(now - Duration::hours(1))
        .bind(now - Duration::hours(2))
        .execute(&pool)
        .await
        .unwrap();

        let outcome = cleanup_auth_sessions(&pool, 100, 7 * 24 * 60 * 60)
            .await
            .unwrap();
        assert!(outcome.lock_acquired);
        assert_eq!(outcome.deleted_user_sessions, 1);
        assert_eq!(outcome.deleted_native_sessions, 2);
        assert_eq!(outcome.deleted_used_refresh_tokens, 1);

        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "select count(*) from user_sessions where token_hash = $1"
            )
            .bind(active_web_hash)
            .fetch_one(&pool)
            .await
            .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "select count(*) from native_client_sessions where id = $1"
            )
            .bind(active_native_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "select count(*) from native_client_sessions where id = any($1)"
            )
            .bind(vec![expired_native_id, revoked_native_id])
            .fetch_one(&pool)
            .await
            .unwrap(),
            0
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn replayed_refresh_rotation_revokes_the_session_in_the_same_transaction(pool: PgPool) {
        let user_id = seed_user(&pool, "rotation-user").await;
        let now = OffsetDateTime::now_utc();
        let old_refresh_hash = "a".repeat(64);
        let old_refresh_expires_at = now + Duration::days(30);
        let session_id: i64 = sqlx::query_scalar(
            r#"
            insert into native_client_sessions (
                user_id,
                access_token_hash,
                refresh_token_hash,
                access_token_expires_at,
                refresh_token_expires_at
            )
            values ($1, $2, $3, $4, $5)
            returning id
            "#,
        )
        .bind(user_id)
        .bind("b".repeat(64))
        .bind(&old_refresh_hash)
        .bind(now + Duration::hours(2))
        .bind(old_refresh_expires_at)
        .fetch_one(&pool)
        .await
        .unwrap();

        let first = rotate_native_client_session_tokens(
            &pool,
            RotateNativeClientSessionTokensParams {
                session_id,
                old_refresh_token_hash: &old_refresh_hash,
                old_refresh_token_expires_at: old_refresh_expires_at,
                new_access_token_hash: &"c".repeat(64),
                new_refresh_token_hash: &"d".repeat(64),
                access_token_expires_at: now + Duration::hours(2),
                refresh_token_expires_at: now + Duration::days(30),
            },
        )
        .await
        .unwrap();
        assert_eq!(first, NativeClientTokenRotationOutcome::Rotated);

        let replay = rotate_native_client_session_tokens(
            &pool,
            RotateNativeClientSessionTokensParams {
                session_id,
                old_refresh_token_hash: &old_refresh_hash,
                old_refresh_token_expires_at: old_refresh_expires_at,
                new_access_token_hash: &"e".repeat(64),
                new_refresh_token_hash: &"f".repeat(64),
                access_token_expires_at: now + Duration::hours(2),
                refresh_token_expires_at: now + Duration::days(30),
            },
        )
        .await
        .unwrap();
        assert_eq!(replay, NativeClientTokenRotationOutcome::Replayed);

        let row = sqlx::query(
            r#"
            select refresh_token_hash, revoked_at
            from native_client_sessions
            where id = $1
            "#,
        )
        .bind(session_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.get::<String, _>("refresh_token_hash"), "d".repeat(64));
        assert!(row.get::<Option<OffsetDateTime>, _>("revoked_at").is_some());
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn session_activity_touches_are_throttled(pool: PgPool) {
        let user_id = seed_user(&pool, "touch-user").await;
        let now = OffsetDateTime::now_utc();
        let web_hash = "0".repeat(64);
        sqlx::query(
            r#"
            insert into user_sessions (
                token_hash,
                user_id,
                expires_at,
                created_at,
                last_seen_at
            )
            values ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(&web_hash)
        .bind(user_id)
        .bind(now + Duration::hours(1))
        .bind(now - Duration::minutes(20))
        .bind(now - Duration::minutes(1))
        .execute(&pool)
        .await
        .unwrap();

        let before: OffsetDateTime =
            sqlx::query_scalar("select last_seen_at from user_sessions where token_hash = $1")
                .bind(&web_hash)
                .fetch_one(&pool)
                .await
                .unwrap();
        get_user_by_session_token_hash(&pool, &web_hash)
            .await
            .unwrap()
            .unwrap();
        let unchanged: OffsetDateTime =
            sqlx::query_scalar("select last_seen_at from user_sessions where token_hash = $1")
                .bind(&web_hash)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(unchanged, before);

        sqlx::query(
            "update user_sessions set last_seen_at = clock_timestamp() - interval '10 minutes' where token_hash = $1",
        )
        .bind(&web_hash)
        .execute(&pool)
        .await
        .unwrap();
        let stale: OffsetDateTime =
            sqlx::query_scalar("select last_seen_at from user_sessions where token_hash = $1")
                .bind(&web_hash)
                .fetch_one(&pool)
                .await
                .unwrap();
        get_user_by_session_token_hash(&pool, &web_hash)
            .await
            .unwrap()
            .unwrap();
        let touched: OffsetDateTime =
            sqlx::query_scalar("select last_seen_at from user_sessions where token_hash = $1")
                .bind(&web_hash)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(touched > stale);

        let native_id: i64 = sqlx::query_scalar(
            r#"
            insert into native_client_sessions (
                user_id,
                access_token_hash,
                refresh_token_hash,
                access_token_expires_at,
                refresh_token_expires_at,
                created_at,
                last_used_at
            )
            values ($1, $2, $3, $4, $5, $6, $7)
            returning id
            "#,
        )
        .bind(user_id)
        .bind("1a".repeat(32))
        .bind("2b".repeat(32))
        .bind(now + Duration::hours(1))
        .bind(now + Duration::hours(2))
        .bind(now - Duration::minutes(20))
        .bind(now - Duration::minutes(1))
        .fetch_one(&pool)
        .await
        .unwrap();
        let native_before: OffsetDateTime =
            sqlx::query_scalar("select last_used_at from native_client_sessions where id = $1")
                .bind(native_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        touch_native_client_session(&pool, native_id).await.unwrap();
        let native_unchanged: OffsetDateTime =
            sqlx::query_scalar("select last_used_at from native_client_sessions where id = $1")
                .bind(native_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(native_unchanged, native_before);
    }
}
