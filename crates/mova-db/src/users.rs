use anyhow::{Context, Result};
use mova_domain::{User, UserProfile, UserRole};
use sqlx::{
    postgres::{PgPool, PgRow},
    Row,
};
use time::OffsetDateTime;

#[derive(Debug, Clone)]
pub struct CreateUserParams {
    pub username: String,
    pub nickname: String,
    pub password_hash: String,
    pub role: UserRole,
    pub is_enabled: bool,
    pub library_ids: Vec<i64>,
}

#[derive(Debug, Clone)]
pub struct UpdateUserParams {
    pub username: String,
    pub nickname: String,
    pub role: UserRole,
    pub is_enabled: bool,
    pub library_ids: Vec<i64>,
}

#[derive(Debug, Clone)]
pub struct CreateSessionParams {
    pub token: String,
    pub user_id: i64,
    pub expires_at: OffsetDateTime,
}

#[derive(Debug, Clone)]
pub struct UserAuthenticationRecord {
    pub user: User,
    pub password_hash: String,
}

pub async fn count_admin_users(pool: &PgPool) -> Result<i64> {
    let row = sqlx::query(
        r#"
        select count(*) as total
        from users
        where role = 'admin'
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
        where role = 'admin'
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
        select id, username, nickname, role, is_enabled, created_at, updated_at
        from users
        order by created_at asc, id asc
        "#,
    )
    .fetch_all(pool)
    .await
    .context("failed to list users")?;

    let mut profiles = Vec::with_capacity(rows.len());
    for row in rows {
        let user = map_user_row(row);
        let library_ids = list_library_ids_for_user(pool, user.id).await?;
        profiles.push(UserProfile { user, library_ids });
    }

    Ok(profiles)
}

pub async fn get_user(pool: &PgPool, user_id: i64) -> Result<Option<UserProfile>> {
    let row = sqlx::query(
        r#"
        select id, username, nickname, role, is_enabled, created_at, updated_at
        from users
        where id = $1
        "#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .context("failed to get user")?;

    let Some(row) = row else {
        return Ok(None);
    };

    let user = map_user_row(row);
    let library_ids = list_library_ids_for_user(pool, user.id).await?;

    Ok(Some(UserProfile { user, library_ids }))
}

pub async fn get_user_by_username(
    pool: &PgPool,
    username: &str,
) -> Result<Option<UserAuthenticationRecord>> {
    let row = sqlx::query(
        r#"
        select id, username, nickname, password_hash, role, is_enabled, created_at, updated_at
        from users
        where username = $1
        "#,
    )
    .bind(username)
    .fetch_optional(pool)
    .await
    .context("failed to get user by username")?;

    Ok(row.map(|row| UserAuthenticationRecord {
        password_hash: row.get("password_hash"),
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
        select id, username, nickname, password_hash, role, is_enabled, created_at, updated_at
        from users
        where id = $1
        "#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .context("failed to get user authentication record")?;

    Ok(row.map(|row| UserAuthenticationRecord {
        password_hash: row.get("password_hash"),
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
        insert into users (username, nickname, password_hash, role, is_enabled)
        values ($1, $2, $3, $4, $5)
        returning id, username, nickname, role, is_enabled, created_at, updated_at
        "#,
    )
    .bind(params.username)
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
        set username = $2,
            nickname = $3,
            role = $4,
            is_enabled = $5,
            updated_at = now()
        where id = $1
        returning id, username, nickname, role, is_enabled, created_at, updated_at
        "#,
    )
    .bind(user_id)
    .bind(params.username)
    .bind(params.nickname)
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

pub async fn replace_user_library_access(
    pool: &PgPool,
    user_id: i64,
    library_ids: &[i64],
) -> Result<Vec<i64>> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to start user access update transaction")?;

    sqlx::query("delete from user_library_access where user_id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .context("failed to clear existing user library access")?;

    write_user_library_access(&mut tx, user_id, library_ids).await?;

    tx.commit()
        .await
        .context("failed to commit user access update transaction")?;

    Ok(library_ids.to_vec())
}

pub async fn update_user_password(pool: &PgPool, user_id: i64, password_hash: &str) -> Result<()> {
    sqlx::query(
        r#"
        update users
        set password_hash = $2,
            updated_at = now()
        where id = $1
        "#,
    )
    .bind(user_id)
    .bind(password_hash)
    .execute(pool)
    .await
    .context("failed to update user password")?;

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
        insert into user_sessions (token, user_id, expires_at)
        values ($1, $2, $3)
        "#,
    )
    .bind(params.token)
    .bind(params.user_id)
    .bind(params.expires_at)
    .execute(pool)
    .await
    .context("failed to create user session")?;

    Ok(())
}

pub async fn get_user_by_session_token(pool: &PgPool, token: &str) -> Result<Option<UserProfile>> {
    let row = sqlx::query(
        r#"
        select
            u.id,
            u.username,
            u.nickname,
            u.role,
            u.is_enabled,
            u.created_at,
            u.updated_at
        from user_sessions s
        join users u on u.id = s.user_id
        where s.token = $1
          and s.expires_at > now()
        "#,
    )
    .bind(token)
    .fetch_optional(pool)
    .await
    .context("failed to get user by session token")?;

    let Some(row) = row else {
        return Ok(None);
    };

    sqlx::query(
        r#"
        update user_sessions
        set last_seen_at = now()
        where token = $1
        "#,
    )
    .bind(token)
    .execute(pool)
    .await
    .context("failed to update user session last_seen_at")?;

    let user = map_user_row(row);
    let library_ids = list_library_ids_for_user(pool, user.id).await?;

    Ok(Some(UserProfile { user, library_ids }))
}

pub async fn delete_session(pool: &PgPool, token: &str) -> Result<()> {
    sqlx::query(
        r#"
        delete from user_sessions
        where token = $1
        "#,
    )
    .bind(token)
    .execute(pool)
    .await
    .context("failed to delete user session")?;

    Ok(())
}

pub async fn delete_sessions_for_user(pool: &PgPool, user_id: i64) -> Result<()> {
    sqlx::query(
        r#"
        delete from user_sessions
        where user_id = $1
        "#,
    )
    .bind(user_id)
    .execute(pool)
    .await
    .context("failed to delete user sessions")?;

    Ok(())
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

fn parse_user_role(value: &str) -> UserRole {
    match value {
        "admin" => UserRole::Admin,
        "viewer" => UserRole::Viewer,
        other => panic!("unexpected user role in database: {}", other),
    }
}
