use anyhow::{Context, Result};
use mova_domain::MediaCastMember;
use sqlx::{postgres::PgPool, Row};
use time::OffsetDateTime;

#[derive(Debug, Clone)]
pub struct MediaItemCastCacheEntry {
    pub media_item_id: i64,
    pub fetched_at: OffsetDateTime,
    pub expires_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone)]
pub struct ReplaceMediaItemCastParams {
    pub media_item_id: i64,
    pub members: Vec<ReplaceMediaItemCastMember>,
    pub fetched_at: OffsetDateTime,
    pub expires_at: OffsetDateTime,
}

#[derive(Debug, Clone)]
pub struct ReplaceMediaItemCastMember {
    pub person_id: Option<i64>,
    pub sort_order: i32,
    pub name: String,
    pub character_name: Option<String>,
    pub profile_path: Option<String>,
}

pub async fn get_media_item_cast_cache(
    pool: &PgPool,
    media_item_id: i64,
) -> Result<Option<MediaItemCastCacheEntry>> {
    let row = sqlx::query(
        r#"
        select media_item_id, fetched_at, expires_at, updated_at
        from media_item_cast_cache
        where media_item_id = $1
        "#,
    )
    .bind(media_item_id)
    .fetch_optional(pool)
    .await
    .context("failed to get media item cast cache")?;

    Ok(row.map(|row| MediaItemCastCacheEntry {
        media_item_id: row.get("media_item_id"),
        fetched_at: row.get("fetched_at"),
        expires_at: row.get("expires_at"),
        updated_at: row.get("updated_at"),
    }))
}

pub async fn list_media_item_cast_members(
    pool: &PgPool,
    media_item_id: i64,
) -> Result<Vec<MediaCastMember>> {
    let rows = sqlx::query(
        r#"
        select
            media_item_id,
            provider_person_id,
            sort_order,
            name,
            character_name,
            profile_path
        from media_item_cast_members
        where media_item_id = $1
        order by sort_order asc, name asc
        "#,
    )
    .bind(media_item_id)
    .fetch_all(pool)
    .await
    .context("failed to list media item cast members")?;

    Ok(rows
        .into_iter()
        .map(|row| MediaCastMember {
            media_item_id: row.get("media_item_id"),
            person_id: row.get("provider_person_id"),
            sort_order: row.get("sort_order"),
            name: row.get("name"),
            character_name: row.get("character_name"),
            profile_path: row.get("profile_path"),
        })
        .collect())
}

pub async fn replace_media_item_cast(
    pool: &PgPool,
    params: ReplaceMediaItemCastParams,
) -> Result<()> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to start media item cast replacement transaction")?;

    sqlx::query("delete from media_item_cast_members where media_item_id = $1")
        .bind(params.media_item_id)
        .execute(&mut *tx)
        .await
        .context("failed to clear existing media item cast members")?;

    for member in &params.members {
        sqlx::query(
            r#"
            insert into media_item_cast_members (
                media_item_id,
                sort_order,
                provider_person_id,
                name,
                character_name,
                profile_path
            )
            values ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(params.media_item_id)
        .bind(member.sort_order)
        .bind(member.person_id)
        .bind(&member.name)
        .bind(&member.character_name)
        .bind(&member.profile_path)
        .execute(&mut *tx)
        .await
        .context("failed to insert media item cast member")?;
    }

    sqlx::query(
        r#"
        insert into media_item_cast_cache (media_item_id, fetched_at, expires_at)
        values ($1, $2, $3)
        on conflict (media_item_id) do update
        set fetched_at = excluded.fetched_at,
            expires_at = excluded.expires_at,
            updated_at = now()
        "#,
    )
    .bind(params.media_item_id)
    .bind(params.fetched_at)
    .bind(params.expires_at)
    .execute(&mut *tx)
    .await
    .context("failed to upsert media item cast cache")?;

    tx.commit()
        .await
        .context("failed to commit media item cast replacement transaction")?;

    Ok(())
}

pub async fn delete_media_item_cast_cache(pool: &PgPool, media_item_id: i64) -> Result<()> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to start media item cast cache deletion transaction")?;

    sqlx::query("delete from media_item_cast_members where media_item_id = $1")
        .bind(media_item_id)
        .execute(&mut *tx)
        .await
        .context("failed to delete media item cast members")?;

    sqlx::query("delete from media_item_cast_cache where media_item_id = $1")
        .bind(media_item_id)
        .execute(&mut *tx)
        .await
        .context("failed to delete media item cast cache")?;

    tx.commit()
        .await
        .context("failed to commit media item cast cache deletion transaction")?;

    Ok(())
}
