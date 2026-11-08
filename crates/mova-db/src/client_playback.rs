//! Device sessions reuse the account session lifecycle and permanent progress.
use crate::{CreateSessionParams, UpsertPlaybackProgressParams};
use anyhow::Result;
use mova_domain::PlaybackProgress;
use sqlx::{PgPool, Row};

#[derive(Debug, Clone)]
pub struct Device {
    pub device_id: String,
    pub device_name: String,
    pub client_name: String,
    pub client_version: String,
}

pub async fn server_id(pool: &PgPool, candidate: &str) -> Result<String> {
    sqlx::query(
        "insert into server_identity(singleton,server_id) values(true,$1) on conflict do nothing",
    )
    .bind(candidate)
    .execute(pool)
    .await?;
    Ok(
        sqlx::query_scalar("select server_id from server_identity where singleton")
            .fetch_one(pool)
            .await?,
    )
}

pub async fn create_session(
    pool: &PgPool,
    session: CreateSessionParams,
    device: &Device,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("insert into user_sessions(token_hash,user_id,expires_at) values($1,$2,$3)")
        .bind(&session.token_hash)
        .bind(session.user_id)
        .bind(session.expires_at)
        .execute(&mut *tx)
        .await?;
    sqlx::query("insert into emby_devices(token_hash,device_id,device_name,client_name,client_version) values($1,$2,$3,$4,$5)")
        .bind(&session.token_hash).bind(&device.device_id).bind(&device.device_name).bind(&device.client_name).bind(&device.client_version)
        .execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn get_device(pool: &PgPool, hash: &str) -> Result<Option<Device>> {
    Ok(sqlx::query("select device_id,device_name,client_name,client_version from emby_devices where token_hash=$1")
        .bind(hash).fetch_optional(pool).await?.map(|r| Device {
            device_id:r.get("device_id"), device_name:r.get("device_name"),
            client_name:r.get("client_name"), client_version:r.get("client_version"),
        }))
}

pub async fn create_play_session(
    pool: &PgPool,
    id: &str,
    hash: &str,
    user_id: i64,
    item_id: i64,
    file_id: i64,
) -> Result<()> {
    sqlx::query("insert into playback_sessions(id,token_hash,user_id,media_item_id,media_file_id) values($1,$2,$3,$4,$5)")
        .bind(id).bind(hash).bind(user_id).bind(item_id).bind(file_id).execute(pool).await?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct PlaySession {
    pub id: String,
    pub media_item_id: i64,
    pub media_file_id: i64,
    pub stopped: bool,
}

pub async fn get_play_session(
    pool: &PgPool,
    hash: &str,
    id: Option<&str>,
    item_id: i64,
    file_id: Option<i64>,
) -> Result<Option<PlaySession>> {
    Ok(sqlx::query("select id,media_item_id,media_file_id,stopped_at is not null as stopped from playback_sessions where token_hash=$1 and ($2::text is null or id=$2) and media_item_id=$3 and ($4::bigint is null or media_file_id=$4) and expires_at>clock_timestamp() order by created_at desc,id desc limit 1")
        .bind(hash).bind(id).bind(item_id).bind(file_id).fetch_optional(pool).await?.map(|r| PlaySession {
            id:r.get("id"),media_item_id:r.get("media_item_id"),media_file_id:r.get("media_file_id"),stopped:r.get("stopped"),
        }))
}

/// Session and progress writes share a transaction. Late heartbeats cannot
/// reopen stopped sessions or replace a newer client's playback.
pub async fn report_progress(
    pool: &PgPool,
    hash: &str,
    session: &str,
    stopped: bool,
    params: UpsertPlaybackProgressParams,
) -> Result<Option<PlaybackProgress>> {
    let mut tx = pool.begin().await?;
    sqlx::query("select pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(format!(
            "playback:{}:{}",
            params.user_id, params.media_item_id
        ))
        .execute(&mut *tx)
        .await?;
    let row = sqlx::query("select stopped_at from playback_sessions where id=$1 and token_hash=$2 and user_id=$3 and media_item_id=$4 and media_file_id=$5 and expires_at>clock_timestamp() for update")
        .bind(session).bind(hash).bind(params.user_id).bind(params.media_item_id).bind(params.media_file_id).fetch_optional(&mut *tx).await?;
    let Some(row) = row else {
        return Ok(None);
    };
    if row
        .get::<Option<time::OffsetDateTime>, _>("stopped_at")
        .is_some()
    {
        return Ok(None);
    }
    sqlx::query("update playback_sessions set started_at=coalesce(started_at,clock_timestamp()),updated_at=clock_timestamp(),stopped_at=case when $2 then clock_timestamp() else null end,expires_at=clock_timestamp()+interval '24 hours' where id=$1")
        .bind(session).bind(stopped).execute(&mut *tx).await?;
    let superseded: bool = sqlx::query_scalar("select exists(select 1 from playback_sessions newer join playback_sessions current on current.id=$1 where newer.user_id=current.user_id and newer.media_item_id=current.media_item_id and newer.started_at>current.started_at)")
        .bind(session).fetch_one(&mut *tx).await?;
    let progress = if superseded {
        None
    } else {
        Some(crate::playback_progress::upsert_playback_progress_tx(&mut tx, params).await?)
    };
    tx.commit().await?;
    Ok(progress)
}

pub async fn cleanup_play_sessions(pool: &PgPool) -> Result<()> {
    sqlx::query("delete from playback_sessions where id in (select id from playback_sessions where expires_at<=clock_timestamp() order by expires_at limit 1000)")
        .execute(pool).await?;
    Ok(())
}
