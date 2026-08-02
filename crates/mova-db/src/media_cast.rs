use anyhow::{Context, Result};
use mova_domain::MediaCastMember;
use sqlx::{
    postgres::{PgPool, Postgres},
    QueryBuilder, Row, Transaction,
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
    pub expected_provider_item_id: String,
    pub expected_media_item_updated_at: OffsetDateTime,
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
        select
            cache.media_item_id,
            cache.fetched_at,
            cache.expires_at,
            cache.updated_at
        from media_item_cast_cache cache
        join media_items item on item.id = cache.media_item_id
        where cache.media_item_id = $1
          and cache.metadata_provider = 'tmdb'
          and cache.provider_item_id = item.metadata_provider_item_id
          and cache.source_media_item_updated_at = item.updated_at
          and item.media_type in ('movie', 'series')
          and item.metadata_provider = 'tmdb'
          and item.metadata_status = 'matched'
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
    let local_members =
        crate::local_metadata::list_preferred_local_cast_members(pool, media_item_id).await?;
    if !local_members.is_empty() {
        return Ok(local_members
            .into_iter()
            .map(|member| MediaCastMember {
                media_item_id: member.media_item_id,
                person_id: member.person_id,
                sort_order: member.sort_order,
                name: member.name,
                character_name: member.role,
                profile_path: member.profile_path,
            })
            .collect());
    }

    let rows = sqlx::query(
        r#"
        select
            member.media_item_id,
            member.provider_person_id,
            member.sort_order,
            member.name,
            member.character_name,
            member.profile_path
        from media_item_cast_members member
        join media_item_cast_cache cache
          on cache.media_item_id = member.media_item_id
        join media_items item on item.id = cache.media_item_id
        where member.media_item_id = $1
          and cache.metadata_provider = 'tmdb'
          and cache.provider_item_id = item.metadata_provider_item_id
          and cache.source_media_item_updated_at = item.updated_at
          and item.media_type in ('movie', 'series')
          and item.metadata_provider = 'tmdb'
          and item.metadata_status = 'matched'
        order by member.sort_order asc, member.name asc
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
) -> Result<bool> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to start media item cast replacement transaction")?;

    let replaced = replace_media_item_cast_tx(
        &mut tx,
        params.media_item_id,
        &params.expected_provider_item_id,
        params.expected_media_item_updated_at,
        &params.members,
        params.fetched_at,
        params.expires_at,
    )
    .await?;

    tx.commit()
        .await
        .context("failed to commit media item cast replacement transaction")?;

    Ok(replaced)
}

pub(crate) async fn replace_media_item_cast_tx(
    tx: &mut Transaction<'_, Postgres>,
    media_item_id: i64,
    expected_provider_item_id: &str,
    expected_media_item_updated_at: OffsetDateTime,
    members: &[ReplaceMediaItemCastMember],
    fetched_at: OffsetDateTime,
    expires_at: OffsetDateTime,
) -> Result<bool> {
    let current = sqlx::query_scalar::<_, i64>(
        r#"
        select id
        from media_items
        where id = $1
          and media_type in ('movie', 'series')
          and metadata_provider = 'tmdb'
          and metadata_provider_item_id = $2
          and metadata_status = 'matched'
          and updated_at = $3
        for update
        "#,
    )
    .bind(media_item_id)
    .bind(expected_provider_item_id)
    .bind(expected_media_item_updated_at)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to lock the current TMDB cast binding")?;
    if current.is_none() {
        return Ok(false);
    }

    sqlx::query(
        r#"
        insert into media_item_cast_cache (
            media_item_id,
            metadata_provider,
            provider_item_id,
            source_media_item_updated_at,
            fetched_at,
            expires_at
        )
        values ($1, 'tmdb', $2, $3, $4, $5)
        on conflict (media_item_id) do update
        set metadata_provider = excluded.metadata_provider,
            provider_item_id = excluded.provider_item_id,
            source_media_item_updated_at = excluded.source_media_item_updated_at,
            fetched_at = excluded.fetched_at,
            expires_at = excluded.expires_at,
            updated_at = now()
        "#,
    )
    .bind(media_item_id)
    .bind(expected_provider_item_id)
    .bind(expected_media_item_updated_at)
    .bind(fetched_at)
    .bind(expires_at)
    .execute(&mut **tx)
    .await
    .context("failed to upsert media item cast cache")?;

    sqlx::query("delete from media_item_cast_members where media_item_id = $1")
        .bind(media_item_id)
        .execute(&mut **tx)
        .await
        .context("failed to clear existing media item cast members")?;

    if !members.is_empty() {
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
        query_builder.push_values(members, |mut row, member| {
            row.push_bind(media_item_id)
                .push_bind(member.sort_order)
                .push_bind(member.person_id.as_deref())
                .push_bind(&member.name)
                .push_bind(&member.character_name)
                .push_bind(&member.profile_path);
        });
        query_builder
            .build()
            .execute(&mut **tx)
            .await
            .context("failed to insert media item cast members")?;
    }

    Ok(true)
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

    async fn seed_media_item(pool: &sqlx::postgres::PgPool) -> (i64, OffsetDateTime) {
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

        sqlx::query_as::<_, (i64, OffsetDateTime)>(
            r#"
            insert into media_items (
                library_id,
                media_type,
                title,
                source_title,
                metadata_provider,
                metadata_provider_item_id,
                metadata_status
            )
            values ($1, 'movie', 'Movie', 'Movie', 'tmdb', '42', 'matched')
            returning id, updated_at
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
        let (media_item_id, media_item_updated_at) = seed_media_item(&pool).await;
        let now = OffsetDateTime::now_utc();
        replace_media_item_cast(
            &pool,
            ReplaceMediaItemCastParams {
                media_item_id,
                expected_provider_item_id: "42".to_string(),
                expected_media_item_updated_at: media_item_updated_at,
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
                expected_provider_item_id: "42".to_string(),
                expected_media_item_updated_at: media_item_updated_at,
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

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn stale_cast_response_cannot_recreate_cache_after_binding_changes(
        pool: sqlx::postgres::PgPool,
    ) {
        let (media_item_id, observed_updated_at) = seed_media_item(&pool).await;
        sqlx::query(
            r#"
            update media_items
            set metadata_provider_item_id = '84',
                updated_at = clock_timestamp()
            where id = $1
            "#,
        )
        .bind(media_item_id)
        .execute(&pool)
        .await
        .unwrap();

        let replaced = replace_media_item_cast(
            &pool,
            ReplaceMediaItemCastParams {
                media_item_id,
                expected_provider_item_id: "42".to_string(),
                expected_media_item_updated_at: observed_updated_at,
                members: vec![ReplaceMediaItemCastMember {
                    person_id: Some("late-person".to_string()),
                    sort_order: 0,
                    name: "Late Actor".to_string(),
                    character_name: None,
                    profile_path: None,
                }],
                fetched_at: OffsetDateTime::now_utc(),
                expires_at: OffsetDateTime::now_utc(),
            },
        )
        .await
        .unwrap();

        assert!(!replaced);
        assert!(list_media_item_cast_members(&pool, media_item_id)
            .await
            .unwrap()
            .is_empty());
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "select count(*) from media_item_cast_cache where media_item_id = $1",
            )
            .bind(media_item_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
            0
        );
    }
}
