use anyhow::{Context, Result};
use mova_domain::MediaCastMember;
use sqlx::{
    postgres::{PgPool, Postgres},
    QueryBuilder, Row,
};
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
    pub person_id: Option<String>,
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

    sqlx::query("delete from media_item_cast_members where media_item_id = $1")
        .bind(params.media_item_id)
        .execute(&mut *tx)
        .await
        .context("failed to clear existing media item cast members")?;

    if !params.members.is_empty() {
        let mut query_builder = QueryBuilder::<Postgres>::new(
            r#"
            insert into media_item_cast_members (
                media_item_id,
                sort_order,
                provider_person_id,
                name,
                character_name,
                profile_path
            )
            "#,
        );
        query_builder.push_values(&params.members, |mut row, member| {
            row.push_bind(params.media_item_id)
                .push_bind(member.sort_order)
                .push_bind(member.person_id.as_deref())
                .push_bind(&member.name)
                .push_bind(&member.character_name)
                .push_bind(&member.profile_path);
        });
        query_builder
            .build()
            .execute(&mut *tx)
            .await
            .context("failed to insert media item cast members")?;
    }

    tx.commit()
        .await
        .context("failed to commit media item cast replacement transaction")?;

    Ok(())
}

pub async fn delete_media_item_cast_cache(pool: &PgPool, media_item_id: i64) -> Result<()> {
    sqlx::query("delete from media_item_cast_cache where media_item_id = $1")
        .bind(media_item_id)
        .execute(pool)
        .await
        .context("failed to delete media item cast cache")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        delete_media_item_cast_cache, list_media_item_cast_members, replace_media_item_cast,
        ReplaceMediaItemCastMember, ReplaceMediaItemCastParams,
    };
    use time::{Duration, OffsetDateTime};

    async fn seed_media_item(pool: &sqlx::postgres::PgPool) -> i64 {
        let library_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into libraries (name, root_path)
            values ('Cast', '/media/cast')
            returning id
            "#,
        )
        .fetch_one(pool)
        .await
        .unwrap();

        sqlx::query_scalar::<_, i64>(
            r#"
            insert into media_items (library_id, media_type, title, source_title)
            values ($1, 'movie', 'Movie', 'Movie')
            returning id
            "#,
        )
        .bind(library_id)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn cast_replacement_creates_the_cache_before_members_and_rolls_back_as_one_aggregate(
        pool: sqlx::postgres::PgPool,
    ) {
        let media_item_id = seed_media_item(&pool).await;
        let now = OffsetDateTime::now_utc();
        replace_media_item_cast(
            &pool,
            ReplaceMediaItemCastParams {
                media_item_id,
                members: vec![ReplaceMediaItemCastMember {
                    person_id: Some("person-1".to_string()),
                    sort_order: 0,
                    name: "Original Actor".to_string(),
                    character_name: None,
                    profile_path: None,
                }],
                fetched_at: now,
                expires_at: now + Duration::days(1),
            },
        )
        .await
        .unwrap();
        let original_fetched_at = sqlx::query_scalar::<_, OffsetDateTime>(
            "select fetched_at from media_item_cast_cache where media_item_id = $1",
        )
        .bind(media_item_id)
        .fetch_one(&pool)
        .await
        .unwrap();

        let error = replace_media_item_cast(
            &pool,
            ReplaceMediaItemCastParams {
                media_item_id,
                members: vec![ReplaceMediaItemCastMember {
                    person_id: Some("person-2".to_string()),
                    sort_order: -1,
                    name: "Invalid Actor".to_string(),
                    character_name: None,
                    profile_path: None,
                }],
                fetched_at: now + Duration::hours(1),
                expires_at: now + Duration::days(2),
            },
        )
        .await
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("failed to insert media item cast members"));

        let members = list_media_item_cast_members(&pool, media_item_id)
            .await
            .unwrap();
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].name, "Original Actor");

        let fetched_at = sqlx::query_scalar::<_, OffsetDateTime>(
            "select fetched_at from media_item_cast_cache where media_item_id = $1",
        )
        .bind(media_item_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(fetched_at, original_fetched_at);

        delete_media_item_cast_cache(&pool, media_item_id)
            .await
            .unwrap();
        assert!(list_media_item_cast_members(&pool, media_item_id)
            .await
            .unwrap()
            .is_empty());
        let cache_count = sqlx::query_scalar::<_, i64>(
            "select count(*) from media_item_cast_cache where media_item_id = $1",
        )
        .bind(media_item_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(cache_count, 0);
    }
}
