use anyhow::{Context, Result};
use mova_domain::{MediaExternalId, MediaRating};
use sqlx::{postgres::PgPool, Postgres, Row, Transaction};
use std::collections::{HashMap, HashSet};

pub async fn list_media_item_ratings(
    pool: &PgPool,
    media_item_ids: &[i64],
) -> Result<HashMap<i64, Vec<MediaRating>>> {
    if media_item_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let rows = sqlx::query(
        r#"
        select
            media_item_id,
            source,
            kind,
            score::double precision as score,
            scale::double precision as scale,
            rating_count,
            retrieved_via,
            attributes,
            fetched_at
        from media_item_ratings
        where media_item_id = any($1)
        order by
            media_item_id,
            case source when 'tmdb' then 0 else 100 end,
            source,
            kind
        "#,
    )
    .bind(media_item_ids)
    .fetch_all(pool)
    .await
    .context("failed to list media item ratings")?;

    let mut ratings = HashMap::<i64, Vec<MediaRating>>::new();
    for row in rows {
        ratings
            .entry(row.get("media_item_id"))
            .or_default()
            .push(MediaRating {
                source: row.get("source"),
                kind: row.get("kind"),
                score: row.get("score"),
                scale: row.get("scale"),
                rating_count: row.get("rating_count"),
                retrieved_via: row.get("retrieved_via"),
                attributes: row.get("attributes"),
                fetched_at: row.get("fetched_at"),
            });
    }

    Ok(ratings)
}

pub(super) async fn replace_media_item_remote_data(
    tx: &mut Transaction<'_, Postgres>,
    media_item_id: i64,
    metadata_provider: Option<&str>,
    external_ids: &[MediaExternalId],
    ratings: &[MediaRating],
) -> Result<()> {
    let existing_external_ids = sqlx::query(
        r#"
        select provider, external_id
        from media_item_external_ids
        where media_item_id = $1
        "#,
    )
    .bind(media_item_id)
    .fetch_all(&mut **tx)
    .await
    .context("failed to read existing media external ids")?
    .into_iter()
    .map(|row| {
        (
            row.get::<String, _>("provider"),
            row.get::<String, _>("external_id"),
        )
    })
    .collect::<HashSet<_>>();
    let incoming_external_ids = external_ids
        .iter()
        .map(|item| (item.provider.clone(), item.external_id.clone()))
        .collect::<HashSet<_>>();
    let identity_changed = primary_metadata_identity_changed(
        &existing_external_ids,
        &incoming_external_ids,
        metadata_provider,
    );

    sqlx::query("delete from media_item_external_ids where media_item_id = $1")
        .bind(media_item_id)
        .execute(&mut **tx)
        .await
        .context("failed to replace media external ids")?;

    for external_id in external_ids {
        sqlx::query(
            r#"
            insert into media_item_external_ids (
                media_item_id,
                provider,
                external_id
            )
            values ($1, $2, $3)
            on conflict (media_item_id, provider) do update
            set external_id = excluded.external_id,
                updated_at = now()
            "#,
        )
        .bind(media_item_id)
        .bind(&external_id.provider)
        .bind(&external_id.external_id)
        .execute(&mut **tx)
        .await
        .context("failed to upsert media external id")?;
    }

    if identity_changed {
        sqlx::query("delete from media_item_ratings where media_item_id = $1")
            .bind(media_item_id)
            .execute(&mut **tx)
            .await
            .context("failed to clear ratings for replaced media identity")?;
    } else if let Some(metadata_provider) = metadata_provider {
        sqlx::query("delete from media_item_ratings where media_item_id = $1 and source = $2")
            .bind(media_item_id)
            .bind(metadata_provider)
            .execute(&mut **tx)
            .await
            .context("failed to replace media ratings for metadata provider")?;
    }

    for rating in ratings {
        sqlx::query(
            r#"
            insert into media_item_ratings (
                media_item_id,
                source,
                kind,
                score,
                scale,
                rating_count,
                retrieved_via,
                attributes,
                fetched_at
            )
            values (
                $1,
                $2,
                $3,
                $4::double precision::numeric,
                $5::double precision::numeric,
                $6,
                $7,
                $8,
                $9
            )
            on conflict (media_item_id, source, kind) do update
            set score = excluded.score,
                scale = excluded.scale,
                rating_count = excluded.rating_count,
                retrieved_via = excluded.retrieved_via,
                attributes = excluded.attributes,
                fetched_at = excluded.fetched_at,
                updated_at = now()
            where excluded.fetched_at >= media_item_ratings.fetched_at
            "#,
        )
        .bind(media_item_id)
        .bind(&rating.source)
        .bind(&rating.kind)
        .bind(rating.score)
        .bind(rating.scale)
        .bind(rating.rating_count)
        .bind(&rating.retrieved_via)
        .bind(&rating.attributes)
        .bind(rating.fetched_at)
        .execute(&mut **tx)
        .await
        .context("failed to upsert media rating")?;
    }

    Ok(())
}

fn primary_metadata_identity_changed(
    existing_external_ids: &HashSet<(String, String)>,
    incoming_external_ids: &HashSet<(String, String)>,
    metadata_provider: Option<&str>,
) -> bool {
    let Some(metadata_provider) = metadata_provider else {
        return true;
    };

    let existing_provider_id = existing_external_ids
        .iter()
        .find(|(provider, _)| provider == metadata_provider)
        .map(|(_, external_id)| external_id.as_str());
    let incoming_provider_id = incoming_external_ids
        .iter()
        .find(|(provider, _)| provider == metadata_provider)
        .map(|(_, external_id)| external_id.as_str());

    existing_provider_id != incoming_provider_id
}

#[cfg(test)]
mod tests {
    use super::primary_metadata_identity_changed;
    use std::collections::HashSet;

    fn external_ids(values: &[(&str, &str)]) -> HashSet<(String, String)> {
        values
            .iter()
            .map(|(provider, external_id)| (provider.to_string(), external_id.to_string()))
            .collect()
    }

    #[test]
    fn secondary_external_ids_do_not_change_primary_identity() {
        let existing = external_ids(&[("tmdb", "88"), ("imdb", "tt-old")]);
        let incoming = external_ids(&[("tmdb", "88"), ("imdb", "tt-new"), ("wikidata", "Q123")]);

        assert!(!primary_metadata_identity_changed(
            &existing,
            &incoming,
            Some("tmdb")
        ));
    }

    #[test]
    fn primary_provider_id_changes_media_identity() {
        let existing = external_ids(&[("tmdb", "88"), ("imdb", "tt123")]);
        let incoming = external_ids(&[("tmdb", "99"), ("imdb", "tt456")]);

        assert!(primary_metadata_identity_changed(
            &existing,
            &incoming,
            Some("tmdb")
        ));
    }
}
