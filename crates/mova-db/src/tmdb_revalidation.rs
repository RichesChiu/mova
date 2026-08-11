use crate::{
    background_jobs::{lock_background_job_fence, BackgroundJobFence},
    media_cast::{replace_media_item_cast_tx, ReplaceMediaItemCastMember},
    media_items::replace_media_item_remote_data,
};
use anyhow::{bail, Context, Result};
use mova_domain::{MediaExternalId, MediaRating};
use serde_json::json;
use sqlx::{ConnectOptions, Connection, PgConnection, PgPool, Postgres, Row, Transaction};
use time::OffsetDateTime;

pub const TMDB_REVALIDATION_JOB_TYPE: &str = "metadata.tmdb.revalidate";
pub const TMDB_ARTWORK_CLEANUP_JOB_TYPE: &str = "metadata.tmdb.artwork.cleanup";
pub const TMDB_REVALIDATION_INTERVAL_DAYS: i64 = 150;
pub const TMDB_ARTWORK_RETENTION_DAYS: i64 = 180;

async fn lock_library_tmdb_artwork_publication_shared(
    connection: &mut PgConnection,
    library_id: i64,
) -> Result<()> {
    sqlx::query(
        r#"
        select pg_advisory_lock_shared(
            hashtextextended('mova:tmdb-artwork-publication:library:' || $1::text, 0)
        )
        "#,
    )
    .bind(library_id)
    .fetch_one(&mut *connection)
    .await
    .context("failed to share-lock library TMDB artwork publication")?;
    Ok(())
}

async fn lock_library_tmdb_artwork_reference_shared(
    tx: &mut Transaction<'_, Postgres>,
    library_id: i64,
) -> Result<()> {
    sqlx::query(
        r#"
        select pg_advisory_xact_lock_shared(
            hashtextextended('mova:tmdb-artwork:library:' || $1::text, 0)
        )
        "#,
    )
    .bind(library_id)
    .fetch_one(&mut **tx)
    .await
    .context("failed to share-lock library TMDB artwork references")?;
    Ok(())
}

/// Holds the library artwork namespace from the point at which a writer may
/// observe or publish a cache file until every database reference is durable.
///
/// The session is opened outside the application pool so a burst of guards
/// cannot occupy every pooled connection and then deadlock waiting to commit
/// its references. Dropping the guard closes that standalone session, which
/// also releases the advisory lock after cancellation.
pub struct TmdbArtworkPublicationGuard {
    connection: PgConnection,
    library_id: i64,
}

impl TmdbArtworkPublicationGuard {
    pub async fn acquire(pool: &PgPool, library_id: i64) -> Result<Self> {
        if library_id <= 0 {
            bail!("TMDB artwork publication requires a positive library id");
        }

        let mut connection = pool
            .connect_options()
            .connect()
            .await
            .context("failed to open TMDB artwork publication guard session")?;
        lock_library_tmdb_artwork_publication_shared(&mut connection, library_id).await?;
        Ok(Self {
            connection,
            library_id,
        })
    }

    pub async fn release(mut self) -> Result<()> {
        let unlock = sqlx::query_scalar::<_, bool>(
            r#"
            select pg_advisory_unlock_shared(
                hashtextextended('mova:tmdb-artwork-publication:library:' || $1::text, 0)
            )
            "#,
        )
        .bind(self.library_id)
        .fetch_one(&mut self.connection)
        .await
        .context("failed to unlock TMDB artwork publication guard");
        let close = self
            .connection
            .close()
            .await
            .context("failed to close TMDB artwork publication guard session");

        if !unlock? {
            bail!("TMDB artwork publication guard was not held by its session");
        }
        close
    }
}

pub async fn lock_library_tmdb_artwork_reference_write(
    tx: &mut Transaction<'_, Postgres>,
    library_id: i64,
) -> Result<()> {
    lock_library_tmdb_artwork_reference_shared(tx, library_id).await
}

/// Cleanup exclusively locks publication first and references second.
///
/// Publication and reference writes intentionally use separate keys. If they
/// shared one key, an exclusive cleanup waiter could queue between the
/// publication guard and the writer transaction's shared acquisition, causing
/// that writer to deadlock behind cleanup while still blocking cleanup itself.
pub async fn lock_library_tmdb_artwork(
    tx: &mut Transaction<'_, Postgres>,
    library_id: i64,
) -> Result<()> {
    sqlx::query(
        r#"
        select pg_advisory_xact_lock(
            hashtextextended('mova:tmdb-artwork-publication:library:' || $1::text, 0)
        )
        "#,
    )
    .bind(library_id)
    .fetch_one(&mut **tx)
    .await
    .context("failed to exclusively lock library TMDB artwork publication")?;
    sqlx::query(
        r#"
        select pg_advisory_xact_lock(
            hashtextextended('mova:tmdb-artwork:library:' || $1::text, 0)
        )
        "#,
    )
    .bind(library_id)
    .fetch_one(&mut **tx)
    .await
    .context("failed to exclusively lock library TMDB artwork references")?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct TmdbMetadataRevalidationTarget {
    pub media_item_id: i64,
    pub library_id: i64,
    pub media_type: String,
    pub title: String,
    pub source_title: String,
    pub original_title: Option<String>,
    pub year: Option<i32>,
    pub country: Option<String>,
    pub genres: Option<String>,
    pub studio: Option<String>,
    pub overview: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub logo_path: Option<String>,
    pub provider_item_id: String,
    pub metadata_language: String,
    pub remote_snapshot_json: String,
    pub has_cast_cache: bool,
    pub database_now: OffsetDateTime,
    pub next_attempt_at: OffsetDateTime,
    pub retain_until: OffsetDateTime,
    pub observed_media_item_updated_at: OffsetDateTime,
    pub observed_revalidation_updated_at: OffsetDateTime,
    pub seasons: Vec<TmdbRevalidationSeason>,
    pub episodes: Vec<TmdbRevalidationEpisode>,
}

#[derive(Debug, Clone)]
pub struct TmdbRevalidationSeason {
    pub season_id: i64,
    pub season_number: i32,
    pub title: Option<String>,
    pub overview: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub observed_updated_at: OffsetDateTime,
}

#[derive(Debug, Clone)]
pub struct TmdbRevalidationEpisode {
    pub media_item_id: i64,
    pub season_number: i32,
    pub episode_number: i32,
    pub title: String,
    pub source_title: String,
    pub overview: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub observed_updated_at: OffsetDateTime,
}

#[derive(Debug, Clone)]
pub struct ReplaceTmdbRevalidationSeason {
    pub season_id: i64,
    pub observed_updated_at: OffsetDateTime,
    pub title: Option<String>,
    pub overview: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ReplaceTmdbRevalidationEpisode {
    pub media_item_id: i64,
    pub observed_updated_at: OffsetDateTime,
    pub title: String,
    pub overview: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CompleteTmdbMetadataRevalidationParams {
    pub media_item_id: i64,
    pub library_id: i64,
    pub provider_item_id: String,
    pub observed_media_item_updated_at: OffsetDateTime,
    pub observed_revalidation_updated_at: OffsetDateTime,
    pub title: String,
    pub original_title: Option<String>,
    pub year: Option<i32>,
    pub country: Option<String>,
    pub genres: Option<String>,
    pub studio: Option<String>,
    pub overview: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub logo_path: Option<String>,
    pub external_ids: Vec<MediaExternalId>,
    pub ratings: Vec<MediaRating>,
    pub cast_members: Option<Vec<ReplaceMediaItemCastMember>>,
    pub seasons: Vec<ReplaceTmdbRevalidationSeason>,
    pub episodes: Vec<ReplaceTmdbRevalidationEpisode>,
    pub series_outline_json: Option<String>,
    pub artwork_cleanup_paths: Vec<String>,
    pub remote_snapshot_json: String,
}

#[derive(Debug, Clone)]
pub struct ExpireTmdbMetadataRetentionParams {
    pub media_item_id: i64,
    pub library_id: i64,
    pub provider_item_id: String,
    pub observed_media_item_updated_at: OffsetDateTime,
    pub observed_revalidation_updated_at: OffsetDateTime,
    pub title: String,
    pub original_title: Option<String>,
    pub year: Option<i32>,
    pub country: Option<String>,
    pub genres: Option<String>,
    pub studio: Option<String>,
    pub overview: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub logo_path: Option<String>,
    pub seasons: Vec<ReplaceTmdbRevalidationSeason>,
    pub episodes: Vec<ReplaceTmdbRevalidationEpisode>,
    pub artwork_cleanup_paths: Vec<String>,
}

/// Enqueues at most one globally active revalidation. This is both a
/// cross-process rate limit and a guard against metadata refresh work
/// consuming the normal scan worker pool.
pub async fn enqueue_due_tmdb_metadata_revalidation(
    pool: &PgPool,
    provider_enabled: bool,
) -> Result<bool> {
    let inserted = sqlx::query_scalar::<_, i64>(
        r#"
        with candidate as (
            select
                revalidation.media_item_id,
                revalidation.provider_item_id,
                item.library_id,
                revalidation.retain_until <= now() as retention_expired
            from tmdb_metadata_revalidations revalidation
            join media_items item
              on item.id = revalidation.media_item_id
             and item.media_type in ('movie', 'series')
             and item.metadata_provider = 'tmdb'
             and item.metadata_provider_item_id = revalidation.provider_item_id
            where (
                    revalidation.retain_until <= now()
                    or (
                        $1
                        and item.metadata_status = 'matched'
                        and revalidation.next_attempt_at <= now()
                    )
                  )
              and not exists (
                    select 1
                    from background_jobs active
                    where active.job_type = 'metadata.tmdb.revalidate'
                      and active.status in ('pending', 'running')
                  )
              and not exists (
                    select 1
                    from background_jobs scan
                    where scan.job_type = 'library.scan'
                      and scan.scope_type = 'library'
                      and scan.scope_id = item.library_id
                      and (
                            scan.status in ('running', 'cancel_requested')
                            or (
                                scan.status = 'pending'
                                and revalidation.retain_until > now()
                            )
                          )
                  )
            order by
                case when revalidation.retain_until <= now() then 0 else 1 end,
                revalidation.next_attempt_at asc,
                revalidation.media_item_id asc
            for update of revalidation skip locked
            limit 1
        ),
        inserted as (
            insert into background_jobs (
                job_type,
                scope_type,
                scope_id,
                payload,
                max_attempts
            )
            select
                'metadata.tmdb.revalidate',
                'media_item',
                candidate.media_item_id,
                jsonb_build_object(
                    'media_item_id', candidate.media_item_id,
                    'library_id', candidate.library_id,
                    'provider_item_id', candidate.provider_item_id,
                    'retention_expired', candidate.retention_expired
                ),
                1
            from candidate
            on conflict do nothing
            returning id
        )
        select id from inserted
        "#,
    )
    .bind(provider_enabled)
    .fetch_optional(pool)
    .await
    .context("failed to enqueue due TMDB metadata revalidation")?;

    Ok(inserted.is_some())
}

/// Enqueues bounded, library-scoped orphan sweeps once per day.
///
/// A sweep is a crash-recovery safety net: normal writers clean failed
/// publications immediately, while this durable job removes cache files whose
/// publication process exited before a database reference could be committed.
pub async fn enqueue_due_tmdb_artwork_orphan_sweeps(pool: &PgPool) -> Result<u64> {
    let inserted = sqlx::query_scalar::<_, i64>(
        r#"
        with candidates as (
            select library.id
            from libraries library
            where not exists (
                    select 1
                    from background_jobs active
                    where active.job_type = 'metadata.tmdb.artwork.cleanup'
                      and active.scope_type = 'library'
                      and active.scope_id = library.id
                      and active.payload ->> 'maintenance' = 'orphan_sweep'
                      and active.status in ('pending', 'running', 'cancel_requested')
                  )
              and not exists (
                    select 1
                    from background_jobs previous
                    where previous.job_type = 'metadata.tmdb.artwork.cleanup'
                      and previous.scope_type = 'library'
                      and previous.scope_id = library.id
                      and previous.payload ->> 'maintenance' = 'orphan_sweep'
                      and previous.status = 'succeeded'
                      and previous.finished_at >= now() - interval '1 day'
                  )
            order by library.id
            limit 8
        ),
        inserted as (
            insert into background_jobs (
                job_type,
                scope_type,
                scope_id,
                payload,
                max_attempts
            )
            select
                'metadata.tmdb.artwork.cleanup',
                'library',
                candidate.id,
                jsonb_build_object(
                    'library_id', candidate.id,
                    'maintenance', 'orphan_sweep'
                ),
                2147483647
            from candidates candidate
            on conflict do nothing
            returning id
        )
        select count(*)::bigint from inserted
        "#,
    )
    .fetch_one(pool)
    .await
    .context("failed to enqueue due TMDB artwork orphan sweeps")?;

    Ok(u64::try_from(inserted).unwrap_or_default())
}

async fn enqueue_tmdb_artwork_cleanup_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    library_id: i64,
    media_item_id: i64,
    artwork_paths: &[String],
) -> Result<()> {
    if artwork_paths.is_empty() {
        return Ok(());
    }

    sqlx::query(
        r#"
        insert into background_jobs (
            job_type,
            scope_type,
            scope_id,
            payload,
            max_attempts
        )
        values (
            'metadata.tmdb.artwork.cleanup',
            'library',
            $1,
            $2,
            2147483647
        )
        "#,
    )
    .bind(library_id)
    .bind(json!({
        "library_id": library_id,
        "media_item_id": media_item_id,
        "artwork_paths": artwork_paths,
    }))
    .execute(&mut **tx)
    .await
    .context("failed to enqueue durable TMDB artwork cleanup")?;

    Ok(())
}

pub(crate) async fn record_authoritative_tmdb_snapshot_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    media_item_id: i64,
    metadata_provider: Option<&str>,
    remote_snapshot_json: Option<&str>,
    renew_retention: bool,
) -> Result<()> {
    if metadata_provider != Some("tmdb") {
        return Ok(());
    }
    if renew_retention {
        bail!("TMDB retention clocks may only be renewed by dedicated direct-ID revalidation");
    }
    let Some(remote_snapshot_json) = remote_snapshot_json else {
        return Ok(());
    };
    let mut snapshot = serde_json::from_str::<serde_json::Value>(remote_snapshot_json)
        .context("failed to parse authoritative TMDB remote snapshot")?;
    if !snapshot.is_object() {
        bail!("authoritative TMDB remote snapshot must be a JSON object");
    }
    if !renew_retention {
        let previous_snapshot = sqlx::query_scalar::<_, serde_json::Value>(
            r#"
            select revalidation.remote_snapshot
            from tmdb_metadata_revalidations revalidation
            join media_items item on item.id = revalidation.media_item_id
            where revalidation.media_item_id = $1
              and item.media_type in ('movie', 'series')
              and item.metadata_provider = 'tmdb'
              and item.metadata_provider_item_id = revalidation.provider_item_id
              and item.metadata_status = 'matched'
            for update of revalidation, item
            "#,
        )
        .bind(media_item_id)
        .fetch_optional(&mut **tx)
        .await
        .context("failed to lock partial TMDB ownership snapshot")?
        .ok_or_else(|| {
            anyhow::anyhow!("TMDB snapshot target {media_item_id} is no longer eligible")
        })?;
        snapshot = merge_partial_tmdb_ownership_snapshot(previous_snapshot, snapshot);
    }

    let updated = sqlx::query(
        r#"
        with verification_clock as materialized (
            select clock_timestamp() as verified_at
        )
        update tmdb_metadata_revalidations revalidation
        set verified_at = case
                when $3 then verification_clock.verified_at
                else revalidation.verified_at
            end,
            last_attempt_at = case
                when $3 then verification_clock.verified_at
                else revalidation.last_attempt_at
            end,
            next_attempt_at = case
                when $3 then verification_clock.verified_at + interval '150 days'
                else revalidation.next_attempt_at
            end,
            retain_until = case
                when $3 then verification_clock.verified_at + interval '180 days'
                else revalidation.retain_until
            end,
            consecutive_failures = case
                when $3 then 0
                else revalidation.consecutive_failures
            end,
            remote_snapshot = $2,
            updated_at = verification_clock.verified_at
        from media_items item, verification_clock
        where revalidation.media_item_id = $1
          and item.id = revalidation.media_item_id
          and item.media_type in ('movie', 'series')
          and item.metadata_provider = 'tmdb'
          and item.metadata_provider_item_id = revalidation.provider_item_id
          and item.metadata_status = 'matched'
        "#,
    )
    .bind(media_item_id)
    .bind(snapshot)
    .bind(renew_retention)
    .execute(&mut **tx)
    .await
    .context("failed to persist authoritative TMDB remote snapshot")?;
    if updated.rows_affected() != 1 {
        bail!("TMDB snapshot target {media_item_id} is no longer eligible");
    }

    Ok(())
}

fn merge_partial_tmdb_ownership_snapshot(
    previous: serde_json::Value,
    mut incoming: serde_json::Value,
) -> serde_json::Value {
    let previous_outline = previous.get("series_outline").cloned();
    let Some(incoming_object) = incoming.as_object_mut() else {
        return incoming;
    };
    let incoming_outline = incoming_object.get_mut("series_outline");
    let Some(serde_json::Value::Object(incoming_outline)) = incoming_outline else {
        if previous_outline
            .as_ref()
            .is_some_and(serde_json::Value::is_object)
        {
            incoming_object.insert(
                "series_outline".to_string(),
                previous_outline.expect("checked previous TMDB outline object"),
            );
        }
        return incoming;
    };
    let Some(serde_json::Value::Array(incoming_seasons)) = incoming_outline.get_mut("seasons")
    else {
        return incoming;
    };
    let Some(previous_seasons) = previous_outline
        .as_ref()
        .and_then(|outline| outline.get("seasons"))
        .and_then(serde_json::Value::as_array)
    else {
        return incoming;
    };
    let incoming_numbers = incoming_seasons
        .iter()
        .filter_map(|season| season.get("season_number"))
        .filter_map(serde_json::Value::as_i64)
        .collect::<std::collections::HashSet<_>>();
    incoming_seasons.extend(previous_seasons.iter().filter_map(|season| {
        let season_number = season
            .get("season_number")
            .and_then(serde_json::Value::as_i64)?;
        (!incoming_numbers.contains(&season_number)).then(|| season.clone())
    }));
    incoming_seasons.sort_by_key(|season| {
        season
            .get("season_number")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(i64::MAX)
    });
    incoming
}

pub async fn get_tmdb_metadata_revalidation_target(
    pool: &PgPool,
    media_item_id: i64,
    expected_provider_item_id: &str,
) -> Result<Option<TmdbMetadataRevalidationTarget>> {
    let row = sqlx::query(
        r#"
        select
            item.id as media_item_id,
            item.library_id,
            item.media_type,
            item.title,
            item.source_title,
            item.original_title,
            item.year,
            item.country,
            item.genres,
            item.studio,
            item.overview,
            item.poster_path,
            item.backdrop_path,
            item.logo_path,
            item.metadata_provider_item_id as provider_item_id,
            library.metadata_language,
            revalidation.remote_snapshot::text as remote_snapshot_json,
            clock_timestamp() as database_now,
            revalidation.next_attempt_at,
            revalidation.retain_until,
            item.updated_at as media_item_updated_at,
            revalidation.updated_at as revalidation_updated_at,
            exists (
                select 1
                from media_item_cast_cache cast_cache
                where cast_cache.media_item_id = item.id
            ) as has_cast_cache
        from media_items item
        join libraries library on library.id = item.library_id
        join tmdb_metadata_revalidations revalidation
          on revalidation.media_item_id = item.id
         and revalidation.provider_item_id = item.metadata_provider_item_id
        where item.id = $1
          and item.media_type in ('movie', 'series')
          and item.metadata_provider = 'tmdb'
          and item.metadata_provider_item_id = $2
          and (
                item.metadata_status = 'matched'
                or revalidation.retain_until <= clock_timestamp()
              )
        "#,
    )
    .bind(media_item_id)
    .bind(expected_provider_item_id)
    .fetch_optional(pool)
    .await
    .context("failed to load TMDB metadata revalidation target")?;

    let Some(row) = row else {
        return Ok(None);
    };
    let mut target = TmdbMetadataRevalidationTarget {
        media_item_id: row.get("media_item_id"),
        library_id: row.get("library_id"),
        media_type: row.get("media_type"),
        title: row.get("title"),
        source_title: row.get("source_title"),
        original_title: row.get("original_title"),
        year: row.get("year"),
        country: row.get("country"),
        genres: row.get("genres"),
        studio: row.get("studio"),
        overview: row.get("overview"),
        poster_path: row.get("poster_path"),
        backdrop_path: row.get("backdrop_path"),
        logo_path: row.get("logo_path"),
        provider_item_id: row.get("provider_item_id"),
        metadata_language: row.get("metadata_language"),
        remote_snapshot_json: row.get("remote_snapshot_json"),
        has_cast_cache: row.get("has_cast_cache"),
        database_now: row.get("database_now"),
        next_attempt_at: row.get("next_attempt_at"),
        retain_until: row.get("retain_until"),
        observed_media_item_updated_at: row.get("media_item_updated_at"),
        observed_revalidation_updated_at: row.get("revalidation_updated_at"),
        seasons: Vec::new(),
        episodes: Vec::new(),
    };

    if target.media_type == "series" {
        target.seasons = sqlx::query(
            r#"
            select
                season.id,
                season.season_number,
                season.title,
                season.overview,
                season.poster_path,
                season.backdrop_path,
                season.updated_at
            from seasons season
            where season.series_id = $1
            order by season.season_number, season.id
            "#,
        )
        .bind(target.media_item_id)
        .fetch_all(pool)
        .await
        .context("failed to load seasons for TMDB metadata revalidation")?
        .into_iter()
        .map(|row| TmdbRevalidationSeason {
            season_id: row.get("id"),
            season_number: row.get("season_number"),
            title: row.get("title"),
            overview: row.get("overview"),
            poster_path: row.get("poster_path"),
            backdrop_path: row.get("backdrop_path"),
            observed_updated_at: row.get("updated_at"),
        })
        .collect();

        target.episodes = sqlx::query(
            r#"
            select
                item.id,
                season.season_number,
                episode.episode_number,
                item.title,
                item.source_title,
                item.overview,
                item.poster_path,
                item.backdrop_path,
                item.updated_at
            from seasons season
            join episodes episode on episode.season_id = season.id
            join media_items item on item.id = episode.media_item_id
            where season.series_id = $1
            order by season.season_number, episode.episode_number, item.id
            "#,
        )
        .bind(target.media_item_id)
        .fetch_all(pool)
        .await
        .context("failed to load episodes for TMDB metadata revalidation")?
        .into_iter()
        .map(|row| TmdbRevalidationEpisode {
            media_item_id: row.get("id"),
            season_number: row.get("season_number"),
            episode_number: row.get("episode_number"),
            title: row.get("title"),
            source_title: row.get("source_title"),
            overview: row.get("overview"),
            poster_path: row.get("poster_path"),
            backdrop_path: row.get("backdrop_path"),
            observed_updated_at: row.get("updated_at"),
        })
        .collect();
    }

    Ok(Some(target))
}

pub async fn complete_tmdb_metadata_revalidation(
    pool: &PgPool,
    fence: &BackgroundJobFence,
    params: CompleteTmdbMetadataRevalidationParams,
) -> Result<bool> {
    let snapshot = serde_json::from_str::<serde_json::Value>(&params.remote_snapshot_json)
        .context("failed to parse TMDB remote metadata snapshot")?;
    if !snapshot.is_object() {
        bail!("TMDB remote metadata snapshot must be a JSON object");
    }
    if let Some(outline_json) = params.series_outline_json.as_deref() {
        let outline = serde_json::from_str::<serde_json::Value>(outline_json)
            .context("failed to parse revalidated TMDB series outline")?;
        if !outline.is_object() {
            bail!("TMDB series outline must be a JSON object");
        }
    }

    let mut tx = pool
        .begin()
        .await
        .context("failed to start TMDB metadata revalidation transaction")?;
    lock_background_job_fence(&mut tx, fence).await?;
    lock_library_tmdb_artwork_reference_write(&mut tx, params.library_id).await?;

    let verification = sqlx::query(
        r#"
        with verification_clock as materialized (
            select clock_timestamp() as verified_at
        )
        select
            item.library_id,
            verification_clock.verified_at
        from tmdb_metadata_revalidations revalidation
        join media_items item on item.id = revalidation.media_item_id
        cross join verification_clock
        where revalidation.media_item_id = $1
          and revalidation.provider_item_id = $2
          and revalidation.updated_at = $3
          and item.updated_at = $4
          and item.media_type in ('movie', 'series')
          and item.library_id = $5
          and item.metadata_provider = 'tmdb'
          and item.metadata_provider_item_id = $2
          and item.metadata_status = 'matched'
          and verification_clock.verified_at < revalidation.retain_until
        for update of revalidation, item
        "#,
    )
    .bind(params.media_item_id)
    .bind(&params.provider_item_id)
    .bind(params.observed_revalidation_updated_at)
    .bind(params.observed_media_item_updated_at)
    .bind(params.library_id)
    .fetch_optional(&mut *tx)
    .await
    .context("failed to lock current TMDB metadata verification state")?;
    let Some(verification) = verification else {
        tx.rollback()
            .await
            .context("failed to roll back superseded TMDB metadata revalidation")?;
        return Ok(false);
    };
    let library_id = verification.get::<i64, _>("library_id");
    debug_assert_eq!(library_id, params.library_id);
    let verified_at = verification.get::<OffsetDateTime, _>("verified_at");

    sqlx::query("select set_config('mova.defer_catalog_revision', 'on', true)")
        .fetch_one(&mut *tx)
        .await
        .context("failed to defer catalog revision for TMDB metadata revalidation")?;

    if params.series_outline_json.is_some() {
        sqlx::query("select id from seasons where series_id = $1 for update")
            .bind(params.media_item_id)
            .fetch_all(&mut *tx)
            .await
            .context("failed to lock seasons for TMDB metadata revalidation")?;
        let counts = sqlx::query(
            r#"
            select
                (select count(*) from seasons where series_id = $1) as season_count,
                (
                    select count(*)
                    from episodes episode
                    join seasons season on season.id = episode.season_id
                    where season.series_id = $1
                ) as episode_count
            "#,
        )
        .bind(params.media_item_id)
        .fetch_one(&mut *tx)
        .await
        .context("failed to count series rows for TMDB metadata revalidation")?;
        if counts.get::<i64, _>("season_count") != params.seasons.len() as i64
            || counts.get::<i64, _>("episode_count") != params.episodes.len() as i64
        {
            tx.rollback()
                .await
                .context("failed to roll back changed TMDB series structure")?;
            return Ok(false);
        }
    }

    sqlx::query(
        r#"
        update media_items
        set title = $3,
            original_title = $4,
            metadata_status = 'matched',
            metadata_failure_reason = null,
            year = $5,
            country = $6,
            genres = $7,
            studio = $8,
            overview = $9,
            poster_path = $10,
            backdrop_path = $11,
            logo_path = $12,
            updated_at = $13
        where id = $1
          and media_type in ('movie', 'series')
          and metadata_provider = 'tmdb'
          and metadata_provider_item_id = $2
        "#,
    )
    .bind(params.media_item_id)
    .bind(&params.provider_item_id)
    .bind(&params.title)
    .bind(&params.original_title)
    .bind(params.year)
    .bind(&params.country)
    .bind(&params.genres)
    .bind(&params.studio)
    .bind(&params.overview)
    .bind(&params.poster_path)
    .bind(&params.backdrop_path)
    .bind(&params.logo_path)
    .bind(verified_at)
    .execute(&mut *tx)
    .await
    .context("failed to update revalidated TMDB metadata")?;

    for season in &params.seasons {
        let updated = sqlx::query(
            r#"
            update seasons
            set title = $4,
                overview = $5,
                poster_path = $6,
                backdrop_path = $7,
                updated_at = $8
            where id = $1
              and series_id = $2
              and updated_at = $3
            "#,
        )
        .bind(season.season_id)
        .bind(params.media_item_id)
        .bind(season.observed_updated_at)
        .bind(&season.title)
        .bind(&season.overview)
        .bind(&season.poster_path)
        .bind(&season.backdrop_path)
        .bind(verified_at)
        .execute(&mut *tx)
        .await
        .context("failed to update revalidated TMDB season metadata")?;
        if updated.rows_affected() != 1 {
            tx.rollback()
                .await
                .context("failed to roll back changed TMDB season metadata")?;
            return Ok(false);
        }
    }

    for episode in &params.episodes {
        let updated = sqlx::query(
            r#"
            update media_items item
            set title = $4,
                overview = $5,
                poster_path = $6,
                backdrop_path = $7,
                updated_at = $8
            from episodes episode_structure
            join seasons season on season.id = episode_structure.season_id
            where item.id = $1
              and item.media_type = 'episode'
              and item.updated_at = $2
              and episode_structure.media_item_id = item.id
              and season.series_id = $3
            "#,
        )
        .bind(episode.media_item_id)
        .bind(episode.observed_updated_at)
        .bind(params.media_item_id)
        .bind(&episode.title)
        .bind(&episode.overview)
        .bind(&episode.poster_path)
        .bind(&episode.backdrop_path)
        .bind(verified_at)
        .execute(&mut *tx)
        .await
        .context("failed to update revalidated TMDB episode metadata")?;
        if updated.rows_affected() != 1 {
            tx.rollback()
                .await
                .context("failed to roll back changed TMDB episode metadata")?;
            return Ok(false);
        }
    }

    replace_media_item_remote_data(
        &mut tx,
        params.media_item_id,
        Some("tmdb"),
        &params.external_ids,
        &params.ratings,
    )
    .await?;

    if let Some(cast_members) = params.cast_members.as_ref() {
        let replaced = replace_media_item_cast_tx(
            &mut tx,
            params.media_item_id,
            &params.provider_item_id,
            verified_at,
            cast_members,
            verified_at,
            verified_at,
        )
        .await?;
        if !replaced {
            bail!(
                "TMDB cast binding changed for media item {}",
                params.media_item_id
            );
        }
    }

    if let Some(outline_json) = params.series_outline_json.as_deref() {
        sqlx::query(
            r#"
            insert into series_episode_outline_cache (
                series_media_item_id,
                library_id,
                metadata_provider,
                provider_item_id,
                source_media_item_updated_at,
                outline_json,
                fetched_at,
                expires_at
            )
            values ($1, $2, 'tmdb', $3, $4, cast($5 as jsonb), $4, $4 + interval '24 hours')
            on conflict (series_media_item_id)
            do update set
                library_id = excluded.library_id,
                metadata_provider = excluded.metadata_provider,
                provider_item_id = excluded.provider_item_id,
                source_media_item_updated_at = excluded.source_media_item_updated_at,
                outline_json = excluded.outline_json,
                fetched_at = excluded.fetched_at,
                expires_at = excluded.expires_at,
                updated_at = $4
            "#,
        )
        .bind(params.media_item_id)
        .bind(library_id)
        .bind(&params.provider_item_id)
        .bind(verified_at)
        .bind(outline_json)
        .execute(&mut *tx)
        .await
        .context("failed to replace revalidated TMDB series outline cache")?;
    }

    let updated = sqlx::query(
        r#"
        update tmdb_metadata_revalidations
        set verified_at = $3,
            last_attempt_at = $3,
            next_attempt_at = $3 + interval '150 days',
            retain_until = $3 + interval '180 days',
            consecutive_failures = 0,
            remote_snapshot = $4,
            updated_at = $3
        where media_item_id = $1
          and provider_item_id = $2
        "#,
    )
    .bind(params.media_item_id)
    .bind(&params.provider_item_id)
    .bind(verified_at)
    .bind(snapshot)
    .execute(&mut *tx)
    .await
    .context("failed to persist TMDB metadata verification state")?;
    if updated.rows_affected() != 1 {
        bail!(
            "TMDB metadata verification state changed for media item {}",
            params.media_item_id
        );
    }

    enqueue_tmdb_artwork_cleanup_tx(
        &mut tx,
        library_id,
        params.media_item_id,
        &params.artwork_cleanup_paths,
    )
    .await?;

    sqlx::query("select mova_bump_realtime_revision($1)")
        .bind(format!("library:{library_id}:catalog"))
        .fetch_one(&mut *tx)
        .await
        .context("failed to bump revalidated catalog revision")?;

    tx.commit()
        .await
        .context("failed to commit TMDB metadata revalidation transaction")?;
    Ok(true)
}

pub async fn defer_tmdb_revalidation_until_retention_deadline(
    pool: &PgPool,
    fence: &BackgroundJobFence,
    media_item_id: i64,
    provider_item_id: &str,
    observed_media_item_updated_at: OffsetDateTime,
    observed_revalidation_updated_at: OffsetDateTime,
) -> Result<bool> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to start disabled TMDB revalidation transaction")?;
    lock_background_job_fence(&mut tx, fence).await?;

    let updated = sqlx::query(
        r#"
        update tmdb_metadata_revalidations revalidation
        set next_attempt_at = retain_until,
            updated_at = clock_timestamp()
        from media_items item
        where revalidation.media_item_id = $1
          and revalidation.provider_item_id = $2
          and revalidation.updated_at = $3
          and item.id = revalidation.media_item_id
          and item.updated_at = $4
          and item.media_type in ('movie', 'series')
          and item.metadata_provider = 'tmdb'
          and item.metadata_provider_item_id = revalidation.provider_item_id
          and item.metadata_status = 'matched'
        "#,
    )
    .bind(media_item_id)
    .bind(provider_item_id)
    .bind(observed_revalidation_updated_at)
    .bind(observed_media_item_updated_at)
    .execute(&mut *tx)
    .await
    .context("failed to defer disabled TMDB revalidation")?;

    tx.commit()
        .await
        .context("failed to commit disabled TMDB revalidation deferral")?;
    Ok(updated.rows_affected() == 1)
}

pub async fn expire_tmdb_metadata_retention(
    pool: &PgPool,
    fence: &BackgroundJobFence,
    params: ExpireTmdbMetadataRetentionParams,
) -> Result<bool> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to start TMDB retention expiry transaction")?;
    lock_background_job_fence(&mut tx, fence).await?;
    lock_library_tmdb_artwork_reference_write(&mut tx, params.library_id).await?;

    let expiration = sqlx::query(
        r#"
        with expiration_clock as materialized (
            select clock_timestamp() as expired_at
        )
        select
            item.library_id,
            item.media_type,
            expiration_clock.expired_at
        from tmdb_metadata_revalidations revalidation
        join media_items item on item.id = revalidation.media_item_id
        cross join expiration_clock
        where revalidation.media_item_id = $1
          and revalidation.provider_item_id = $2
          and revalidation.updated_at = $3
          and item.updated_at = $4
          and item.media_type in ('movie', 'series')
          and item.library_id = $5
          and item.metadata_provider = 'tmdb'
          and item.metadata_provider_item_id = $2
          and expiration_clock.expired_at >= revalidation.retain_until
        for update of revalidation, item
        "#,
    )
    .bind(params.media_item_id)
    .bind(&params.provider_item_id)
    .bind(params.observed_revalidation_updated_at)
    .bind(params.observed_media_item_updated_at)
    .bind(params.library_id)
    .fetch_optional(&mut *tx)
    .await
    .context("failed to lock current TMDB retention state")?;
    let Some(expiration) = expiration else {
        tx.rollback()
            .await
            .context("failed to roll back superseded TMDB retention expiry")?;
        return Ok(false);
    };
    let library_id = expiration.get::<i64, _>("library_id");
    debug_assert_eq!(library_id, params.library_id);
    let media_type = expiration.get::<String, _>("media_type");
    let expired_at = expiration.get::<OffsetDateTime, _>("expired_at");

    sqlx::query("select set_config('mova.defer_catalog_revision', 'on', true)")
        .fetch_one(&mut *tx)
        .await
        .context("failed to defer catalog revision for TMDB retention expiry")?;

    if media_type == "series" {
        sqlx::query("select id from seasons where series_id = $1 for update")
            .bind(params.media_item_id)
            .fetch_all(&mut *tx)
            .await
            .context("failed to lock seasons for TMDB retention expiry")?;
        let counts = sqlx::query(
            r#"
            select
                (select count(*) from seasons where series_id = $1) as season_count,
                (
                    select count(*)
                    from episodes episode
                    join seasons season on season.id = episode.season_id
                    where season.series_id = $1
                ) as episode_count
            "#,
        )
        .bind(params.media_item_id)
        .fetch_one(&mut *tx)
        .await
        .context("failed to count series rows for TMDB retention expiry")?;
        if counts.get::<i64, _>("season_count") != params.seasons.len() as i64
            || counts.get::<i64, _>("episode_count") != params.episodes.len() as i64
        {
            tx.rollback()
                .await
                .context("failed to roll back changed TMDB series retention state")?;
            return Ok(false);
        }
    }

    for season in &params.seasons {
        let updated = sqlx::query(
            r#"
            update seasons
            set title = $4,
                overview = $5,
                poster_path = $6,
                backdrop_path = $7,
                updated_at = $8
            where id = $1
              and series_id = $2
              and updated_at = $3
            "#,
        )
        .bind(season.season_id)
        .bind(params.media_item_id)
        .bind(season.observed_updated_at)
        .bind(&season.title)
        .bind(&season.overview)
        .bind(&season.poster_path)
        .bind(&season.backdrop_path)
        .bind(expired_at)
        .execute(&mut *tx)
        .await
        .context("failed to expire retained TMDB season metadata")?;
        if updated.rows_affected() != 1 {
            tx.rollback()
                .await
                .context("failed to roll back changed TMDB season retention state")?;
            return Ok(false);
        }
    }

    sqlx::query(
        r#"
        delete from media_item_external_ids external_id
        using media_items item, episodes episode_structure, seasons season
        where external_id.media_item_id = item.id
          and episode_structure.media_item_id = item.id
          and season.id = episode_structure.season_id
          and season.series_id = $1
          and item.metadata_provider = 'tmdb'
          and item.metadata_provider_item_id = $2
          and external_id.retrieved_via not in ('nfo', 'manual')
        "#,
    )
    .bind(params.media_item_id)
    .bind(&params.provider_item_id)
    .execute(&mut *tx)
    .await
    .context("failed to clear retained TMDB episode external ids")?;
    sqlx::query(
        r#"
        delete from media_item_ratings rating
        using media_items item, episodes episode_structure, seasons season
        where rating.media_item_id = item.id
          and episode_structure.media_item_id = item.id
          and season.id = episode_structure.season_id
          and season.series_id = $1
          and item.metadata_provider = 'tmdb'
          and item.metadata_provider_item_id = $2
          and rating.retrieved_via not in ('nfo', 'manual')
        "#,
    )
    .bind(params.media_item_id)
    .bind(&params.provider_item_id)
    .execute(&mut *tx)
    .await
    .context("failed to clear retained TMDB episode ratings")?;

    for episode in &params.episodes {
        let updated = sqlx::query(
            r#"
            update media_items item
            set title = $4,
                metadata_provider = case
                    when item.metadata_provider = 'tmdb'
                         and item.metadata_provider_item_id = $9
                        then null
                    else item.metadata_provider
                end,
                metadata_provider_item_id = case
                    when item.metadata_provider = 'tmdb'
                         and item.metadata_provider_item_id = $9
                        then null
                    else item.metadata_provider_item_id
                end,
                metadata_status = case
                    when item.metadata_provider = 'tmdb'
                         and item.metadata_provider_item_id = $9
                        then 'pending'
                    else item.metadata_status
                end,
                metadata_failure_reason = case
                    when item.metadata_provider = 'tmdb'
                         and item.metadata_provider_item_id = $9
                        then null
                    else item.metadata_failure_reason
                end,
                remote_media_type = case
                    when item.metadata_provider = 'tmdb'
                         and item.metadata_provider_item_id = $9
                        then null
                    else item.remote_media_type
                end,
                overview = $5,
                poster_path = $6,
                backdrop_path = $7,
                updated_at = $8
            from episodes episode_structure
            join seasons season on season.id = episode_structure.season_id
            where item.id = $1
              and item.media_type = 'episode'
              and item.updated_at = $2
              and episode_structure.media_item_id = item.id
              and season.series_id = $3
            "#,
        )
        .bind(episode.media_item_id)
        .bind(episode.observed_updated_at)
        .bind(params.media_item_id)
        .bind(&episode.title)
        .bind(&episode.overview)
        .bind(&episode.poster_path)
        .bind(&episode.backdrop_path)
        .bind(expired_at)
        .bind(&params.provider_item_id)
        .execute(&mut *tx)
        .await
        .context("failed to expire retained TMDB episode metadata")?;
        if updated.rows_affected() != 1 {
            tx.rollback()
                .await
                .context("failed to roll back changed TMDB episode retention state")?;
            return Ok(false);
        }
    }

    sqlx::query("select set_config('mova.tmdb_retention_expiry', 'on', true)")
        .fetch_one(&mut *tx)
        .await
        .context("failed to mark the transaction as managed TMDB retention expiry")?;

    let row = sqlx::query(
        r#"
        update media_items
        set title = $3,
            original_title = $4,
            metadata_provider = null,
            metadata_provider_item_id = null,
            metadata_status = 'pending',
            metadata_failure_reason = null,
            remote_media_type = null,
            year = $5,
            country = $6,
            genres = $7,
            studio = $8,
            overview = $9,
            poster_path = $10,
            backdrop_path = $11,
            logo_path = $12,
            updated_at = $13
        where id = $1
          and media_type in ('movie', 'series')
          and metadata_provider = 'tmdb'
          and metadata_provider_item_id = $2
        returning library_id, title
        "#,
    )
    .bind(params.media_item_id)
    .bind(&params.provider_item_id)
    .bind(&params.title)
    .bind(&params.original_title)
    .bind(params.year)
    .bind(&params.country)
    .bind(&params.genres)
    .bind(&params.studio)
    .bind(&params.overview)
    .bind(&params.poster_path)
    .bind(&params.backdrop_path)
    .bind(&params.logo_path)
    .bind(expired_at)
    .fetch_optional(&mut *tx)
    .await
    .context("failed to expire retained TMDB metadata")?;
    let Some(row) = row else {
        tx.rollback()
            .await
            .context("failed to roll back changed TMDB retention binding")?;
        return Ok(false);
    };
    let retained_title = row.get::<String, _>("title");

    sqlx::query(
        r#"
        delete from media_item_external_ids
        where media_item_id = $1
          and retrieved_via not in ('nfo', 'manual')
        "#,
    )
    .bind(params.media_item_id)
    .execute(&mut *tx)
    .await
    .context("failed to clear retained TMDB external ids")?;
    sqlx::query(
        r#"
        delete from media_item_ratings
        where media_item_id = $1
          and retrieved_via not in ('nfo', 'manual')
        "#,
    )
    .bind(params.media_item_id)
    .execute(&mut *tx)
    .await
    .context("failed to clear retained TMDB ratings")?;
    sqlx::query("delete from media_item_cast_cache where media_item_id = $1")
        .bind(params.media_item_id)
        .execute(&mut *tx)
        .await
        .context("failed to clear retained TMDB cast")?;
    sqlx::query("delete from series_episode_outline_cache where series_media_item_id = $1")
        .bind(params.media_item_id)
        .execute(&mut *tx)
        .await
        .context("failed to clear retained TMDB series outline")?;

    enqueue_tmdb_artwork_cleanup_tx(
        &mut tx,
        library_id,
        params.media_item_id,
        &params.artwork_cleanup_paths,
    )
    .await?;

    let notification_payload = json!({
        "media_item_id": params.media_item_id,
        "library_id": library_id,
        "title": retained_title,
        "provider": "tmdb",
        "reason_code": "tmdb_retention_expired",
        "reason_params": {},
        "diagnostic_message": "TMDB metadata could not be revalidated within 180 days and was cleared.",
    });
    sqlx::query(
        r#"
        insert into notifications (
            category,
            notification_type,
            severity,
            audience,
            library_id,
            source_key,
            payload
        )
        values (
            'library',
            'metadata.tmdb.retention_expired',
            'warning',
            'library',
            $1,
            $2,
            $3
        )
        on conflict (source_key) do update
        set severity = excluded.severity,
            payload = excluded.payload,
            updated_at = now()
        "#,
    )
    .bind(library_id)
    .bind(format!("tmdb-retention-expired:{}", params.media_item_id))
    .bind(notification_payload)
    .execute(&mut *tx)
    .await
    .context("failed to persist TMDB retention expiry notification")?;

    sqlx::query("select mova_bump_realtime_revision($1)")
        .bind(format!("library:{library_id}:catalog"))
        .fetch_one(&mut *tx)
        .await
        .context("failed to bump expired TMDB catalog revision")?;
    sqlx::query("select mova_bump_realtime_revision($1)")
        .bind(format!("library:{library_id}:notifications"))
        .fetch_one(&mut *tx)
        .await
        .context("failed to bump TMDB retention notification revision")?;

    tx.commit()
        .await
        .context("failed to commit TMDB retention expiry")?;
    Ok(true)
}

pub async fn is_artwork_path_referenced_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    path: &str,
) -> Result<bool> {
    sqlx::query_scalar::<_, bool>(
        r#"
        select
            exists (
                select 1
                from media_items item
                where item.poster_path = $1
                   or item.backdrop_path = $1
                   or item.logo_path = $1
            )
            or exists (
                select 1
                from seasons season
                where season.poster_path = $1
                   or season.backdrop_path = $1
            )
            or exists (
                select 1
                from series_episode_outline_cache outline_cache
                where position($1 in outline_cache.outline_json::text) > 0
            )
        "#,
    )
    .bind(path)
    .fetch_one(&mut **tx)
    .await
    .context("failed to check retained artwork references")
}

pub async fn record_tmdb_metadata_revalidation_failure(
    pool: &PgPool,
    fence: &BackgroundJobFence,
    media_item_id: i64,
    provider_item_id: &str,
) -> Result<()> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to start TMDB revalidation failure transaction")?;
    lock_background_job_fence(&mut tx, fence).await?;

    sqlx::query(
        r#"
        with failure_clock as materialized (
            select clock_timestamp() as failed_at
        )
        update tmdb_metadata_revalidations revalidation
        set last_attempt_at = failure_clock.failed_at,
            next_attempt_at = failure_clock.failed_at + case
                when consecutive_failures = 0 then interval '15 minutes'
                when consecutive_failures = 1 then interval '1 hour'
                when consecutive_failures = 2 then interval '6 hours'
                else interval '24 hours'
            end,
            consecutive_failures = consecutive_failures + 1,
            updated_at = failure_clock.failed_at
        from background_jobs job, media_items item, failure_clock
        where revalidation.media_item_id = $1
          and revalidation.provider_item_id = $2
          and job.id = $3
          and item.id = revalidation.media_item_id
          and revalidation.updated_at <= job.locked_at
          and item.updated_at <= job.locked_at
        "#,
    )
    .bind(media_item_id)
    .bind(provider_item_id)
    .bind(fence.job_id)
    .execute(&mut *tx)
    .await
    .context("failed to record TMDB metadata revalidation failure")?;

    tx.commit()
        .await
        .context("failed to commit TMDB metadata revalidation failure")?;
    Ok(())
}

pub async fn discard_ineligible_tmdb_metadata_revalidation(
    pool: &PgPool,
    fence: &BackgroundJobFence,
    media_item_id: i64,
    provider_item_id: &str,
) -> Result<()> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to start stale TMDB revalidation cleanup transaction")?;
    lock_background_job_fence(&mut tx, fence).await?;

    sqlx::query(
        r#"
        delete from tmdb_metadata_revalidations revalidation
        where revalidation.media_item_id = $1
          and revalidation.provider_item_id = $2
          and not exists (
                select 1
                from media_items item
                where item.id = revalidation.media_item_id
                  and item.media_type in ('movie', 'series')
                  and item.metadata_provider = 'tmdb'
                  and item.metadata_provider_item_id = revalidation.provider_item_id
              )
        "#,
    )
    .bind(media_item_id)
    .bind(provider_item_id)
    .execute(&mut *tx)
    .await
    .context("failed to discard stale TMDB metadata revalidation state")?;

    tx.commit()
        .await
        .context("failed to commit stale TMDB revalidation cleanup")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        defer_tmdb_revalidation_until_retention_deadline, enqueue_due_tmdb_artwork_orphan_sweeps,
        enqueue_due_tmdb_metadata_revalidation, expire_tmdb_metadata_retention,
        get_tmdb_metadata_revalidation_target, is_artwork_path_referenced_tx,
        lock_library_tmdb_artwork, lock_library_tmdb_artwork_reference_write,
        merge_partial_tmdb_ownership_snapshot, record_authoritative_tmdb_snapshot_tx,
        record_tmdb_metadata_revalidation_failure, ExpireTmdbMetadataRetentionParams,
        ReplaceTmdbRevalidationEpisode, ReplaceTmdbRevalidationSeason, TmdbArtworkPublicationGuard,
        TMDB_ARTWORK_CLEANUP_JOB_TYPE, TMDB_REVALIDATION_JOB_TYPE,
    };
    use crate::background_jobs::{TMDB_MAINTENANCE_FAILED_ERROR, TMDB_MAINTENANCE_RETRY_ERROR};
    use crate::{claim_background_job, complete_background_job, retry_or_fail_background_job};
    use sqlx::Row;
    use time::OffsetDateTime;

    async fn seed_bound_tmdb_movie(pool: &sqlx::PgPool) -> (i64, i64) {
        let library_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into libraries (name, root_path)
            values ('Revalidation', '/media/revalidation')
            returning id
            "#,
        )
        .fetch_one(pool)
        .await
        .unwrap();
        let media_item_id = sqlx::query_scalar::<_, i64>(
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
            returning id
            "#,
        )
        .bind(library_id)
        .fetch_one(pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            insert into media_item_external_ids (media_item_id, provider, external_id)
            values ($1, 'tmdb', '42')
            "#,
        )
        .bind(media_item_id)
        .execute(pool)
        .await
        .unwrap();

        (library_id, media_item_id)
    }

    async fn seed_bound_tmdb_series_with_supporting_rows(
        pool: &sqlx::PgPool,
    ) -> (i64, i64, String) {
        let library_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into libraries (name, root_path)
            values ('Lifecycle Series', '/media/lifecycle-series')
            returning id
            "#,
        )
        .fetch_one(pool)
        .await
        .unwrap();
        let artwork_path =
            format!("/cache/libraries/{library_id}/artwork/tmdb/poster/old-binding.jpg");
        let series_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into media_items (
                library_id,
                media_type,
                title,
                source_title,
                metadata_provider,
                metadata_provider_item_id,
                metadata_status,
                poster_path
            )
            values ($1, 'series', 'Remote Series', 'Local Series', 'tmdb', '42', 'matched', $2)
            returning id
            "#,
        )
        .bind(library_id)
        .bind(&artwork_path)
        .fetch_one(pool)
        .await
        .unwrap();
        seed_tmdb_supporting_rows(pool, library_id, series_id, "42", &artwork_path).await;

        (library_id, series_id, artwork_path)
    }

    async fn seed_tmdb_supporting_rows(
        pool: &sqlx::PgPool,
        library_id: i64,
        media_item_id: i64,
        provider_item_id: &str,
        artwork_path: &str,
    ) {
        let generation = sqlx::query_scalar::<_, OffsetDateTime>(
            "select updated_at from media_items where id = $1",
        )
        .bind(media_item_id)
        .fetch_one(pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            insert into media_item_external_ids (media_item_id, provider, external_id)
            values
                ($1, 'tmdb', $2),
                ($1, 'imdb', $3)
            on conflict (media_item_id, provider, retrieved_via) do update
            set external_id = excluded.external_id,
                updated_at = now()
            "#,
        )
        .bind(media_item_id)
        .bind(provider_item_id)
        .bind(format!("tt-{provider_item_id}"))
        .execute(pool)
        .await
        .unwrap();
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
                fetched_at
            )
            values ($1, 'tmdb', 'user', 8, 10, 100, 'api', now())
            on conflict (media_item_id, source, kind, retrieved_via) do update
            set score = excluded.score,
                scale = excluded.scale,
                rating_count = excluded.rating_count,
                retrieved_via = excluded.retrieved_via,
                fetched_at = excluded.fetched_at,
                updated_at = now()
            "#,
        )
        .bind(media_item_id)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            insert into media_item_cast_cache (
                media_item_id,
                metadata_provider,
                provider_item_id,
                source_media_item_updated_at,
                expires_at
            )
            values ($1, 'tmdb', $2, $3, now() + interval '1 day')
            "#,
        )
        .bind(media_item_id)
        .bind(provider_item_id)
        .bind(generation)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            insert into media_item_cast_members (
                media_item_id,
                sort_order,
                name,
                profile_path
            )
            values ($1, 0, 'Actor', $2)
            "#,
        )
        .bind(media_item_id)
        .bind(artwork_path)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            insert into series_episode_outline_cache (
                series_media_item_id,
                library_id,
                metadata_provider,
                provider_item_id,
                source_media_item_updated_at,
                outline_json,
                expires_at
            )
            values ($1, $2, 'tmdb', $3, $4, cast($5 as jsonb), now() + interval '1 day')
            "#,
        )
        .bind(media_item_id)
        .bind(library_id)
        .bind(provider_item_id)
        .bind(generation)
        .bind(
            serde_json::json!({
                "seasons": [{
                    "season_number": 1,
                    "poster_path": format!("{artwork_path}.outline-only"),
                    "episodes": []
                }]
            })
            .to_string(),
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            update tmdb_metadata_revalidations
            set remote_snapshot = $2,
                updated_at = clock_timestamp()
            where media_item_id = $1
            "#,
        )
        .bind(media_item_id)
        .bind(serde_json::json!({
            "version": 1,
            "title": "Remote Series",
            "poster_path": artwork_path,
            "series_outline": {
                "seasons": [{
                    "season_number": 1,
                    "poster_path": artwork_path,
                    "episodes": []
                }]
            }
        }))
        .execute(pool)
        .await
        .unwrap();
    }

    #[test]
    fn partial_snapshot_preserves_unverified_seasons_from_previous_ownership() {
        let merged = merge_partial_tmdb_ownership_snapshot(
            serde_json::json!({
                "version": 1,
                "title": "Old title",
                "series_outline": {
                    "seasons": [
                        {"season_number": 1, "title": "Old S1"},
                        {
                            "season_number": 2,
                            "title": "Old S2",
                            "episodes": [
                                {"episode_number": 1, "title": "Old S2E1"}
                            ]
                        }
                    ]
                }
            }),
            serde_json::json!({
                "version": 1,
                "title": "New title",
                "series_outline": {
                    "seasons": [
                        {"season_number": 1, "title": "New S1"}
                    ]
                }
            }),
        );

        assert_eq!(merged["title"], "New title");
        assert_eq!(
            merged["series_outline"]["seasons"],
            serde_json::json!([
                {"season_number": 1, "title": "New S1"},
                {
                    "season_number": 2,
                    "title": "Old S2",
                    "episodes": [
                        {"episode_number": 1, "title": "Old S2E1"}
                    ]
                }
            ])
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn normal_authoritative_tmdb_write_cannot_renew_the_revalidation_window(
        pool: sqlx::PgPool,
    ) {
        let (_, media_item_id) = seed_bound_tmdb_movie(&pool).await;
        let mut tx = pool.begin().await.unwrap();
        let error = record_authoritative_tmdb_snapshot_tx(
            &mut tx,
            media_item_id,
            Some("tmdb"),
            Some(r#"{"version":1,"title":"Movie"}"#),
            true,
        )
        .await
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("only be renewed by dedicated direct-ID revalidation"));
        tx.rollback().await.unwrap();
        let row = sqlx::query(
            r#"
            select verified_at, consecutive_failures, remote_snapshot
            from tmdb_metadata_revalidations
            where media_item_id = $1
            "#,
        )
        .bind(media_item_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.get::<Option<OffsetDateTime>, _>("verified_at"), None);
        assert_eq!(row.get::<i32, _>("consecutive_failures"), 0);
        assert_eq!(
            row.get::<serde_json::Value, _>("remote_snapshot"),
            serde_json::json!({})
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn publication_guard_closes_the_materialization_to_reference_commit_gap(
        pool: sqlx::PgPool,
    ) {
        let (library_id, media_item_id) = seed_bound_tmdb_movie(&pool).await;
        let artwork_path = "/cache/libraries/1/artwork/tmdb/poster/serialized.jpg";
        let publication = TmdbArtworkPublicationGuard::acquire(&pool, library_id)
            .await
            .unwrap();

        let cleanup_pool = pool.clone();
        let (attempted_tx, attempted_rx) = tokio::sync::oneshot::channel();
        let mut cleanup = tokio::spawn(async move {
            let mut tx = cleanup_pool.begin().await.unwrap();
            attempted_tx.send(()).unwrap();
            lock_library_tmdb_artwork(&mut tx, library_id)
                .await
                .unwrap();
            let referenced = is_artwork_path_referenced_tx(&mut tx, artwork_path)
                .await
                .unwrap();
            tx.commit().await.unwrap();
            referenced
        });
        attempted_rx.await.unwrap();
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(100), &mut cleanup)
                .await
                .is_err(),
            "cleanup reference check must wait for the artwork writer transaction"
        );

        let mut writer = pool.begin().await.unwrap();
        lock_library_tmdb_artwork_reference_write(&mut writer, library_id)
            .await
            .unwrap();
        sqlx::query("update media_items set poster_path = $2 where id = $1")
            .bind(media_item_id)
            .bind(artwork_path)
            .execute(&mut *writer)
            .await
            .unwrap();
        writer.commit().await.unwrap();
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(100), &mut cleanup)
                .await
                .is_err(),
            "cleanup must remain blocked until the publication guard is released"
        );

        publication.release().await.unwrap();
        assert!(cleanup.await.unwrap());
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn partial_snapshot_records_ownership_without_renewing_window(pool: sqlx::PgPool) {
        let (_, media_item_id) = seed_bound_tmdb_movie(&pool).await;
        let before = sqlx::query(
            r#"
            select verified_at, next_attempt_at, retain_until
            from tmdb_metadata_revalidations
            where media_item_id = $1
            "#,
        )
        .bind(media_item_id)
        .fetch_one(&pool)
        .await
        .unwrap();

        let mut tx = pool.begin().await.unwrap();
        record_authoritative_tmdb_snapshot_tx(
            &mut tx,
            media_item_id,
            Some("tmdb"),
            Some(r#"{"version":1,"title":"Partial"}"#),
            false,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();

        let after = sqlx::query(
            r#"
            select verified_at, next_attempt_at, retain_until, remote_snapshot
            from tmdb_metadata_revalidations
            where media_item_id = $1
            "#,
        )
        .bind(media_item_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(after.get::<Option<OffsetDateTime>, _>("verified_at"), None);
        assert_eq!(
            after.get::<OffsetDateTime, _>("next_attempt_at"),
            before.get::<OffsetDateTime, _>("next_attempt_at")
        );
        assert_eq!(
            after.get::<OffsetDateTime, _>("retain_until"),
            before.get::<OffsetDateTime, _>("retain_until")
        );
        assert_eq!(
            after.get::<serde_json::Value, _>("remote_snapshot"),
            serde_json::json!({"version": 1, "title": "Partial"})
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn scheduler_enqueues_only_one_global_low_volume_job(pool: sqlx::PgPool) {
        let (_, media_item_id) = seed_bound_tmdb_movie(&pool).await;
        sqlx::query(
            "update tmdb_metadata_revalidations set next_attempt_at = now() where media_item_id = $1",
        )
        .bind(media_item_id)
        .execute(&pool)
        .await
        .unwrap();

        assert!(enqueue_due_tmdb_metadata_revalidation(&pool, true)
            .await
            .unwrap());
        assert!(!enqueue_due_tmdb_metadata_revalidation(&pool, true)
            .await
            .unwrap());

        let job = claim_background_job(&pool, "revalidation-worker", 60)
            .await
            .unwrap()
            .claimed_job
            .unwrap();
        assert_eq!(job.job_type, TMDB_REVALIDATION_JOB_TYPE);
        assert_eq!(job.scope_type, "media_item");
        assert_eq!(job.scope_id, media_item_id);
        assert_eq!(job.max_attempts, 1);

        complete_background_job(&pool, &job.execution_fence().unwrap())
            .await
            .unwrap();
        let terminal_payload = sqlx::query_scalar::<_, serde_json::Value>(
            "select payload from background_jobs where id = $1",
        )
        .bind(job.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(terminal_payload.get("provider_item_id").is_none());
        assert_eq!(
            terminal_payload
                .get("media_item_id")
                .and_then(serde_json::Value::as_i64),
            Some(media_item_id)
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn orphan_sweep_scheduler_is_durable_and_runs_at_most_daily(pool: sqlx::PgPool) {
        let (library_id, _) = seed_bound_tmdb_movie(&pool).await;

        assert_eq!(
            enqueue_due_tmdb_artwork_orphan_sweeps(&pool).await.unwrap(),
            1
        );
        assert_eq!(
            enqueue_due_tmdb_artwork_orphan_sweeps(&pool).await.unwrap(),
            0
        );

        let job = claim_background_job(&pool, "orphan-sweep-worker", 60)
            .await
            .unwrap()
            .claimed_job
            .unwrap();
        assert_eq!(job.job_type, TMDB_ARTWORK_CLEANUP_JOB_TYPE);
        assert_eq!(job.scope_type, "library");
        assert_eq!(job.scope_id, library_id);
        let active_payload = serde_json::from_str::<serde_json::Value>(&job.payload_json).unwrap();
        assert_eq!(
            active_payload
                .get("maintenance")
                .and_then(serde_json::Value::as_str),
            Some("orphan_sweep")
        );

        complete_background_job(&pool, &job.execution_fence().unwrap())
            .await
            .unwrap();
        assert_eq!(
            enqueue_due_tmdb_artwork_orphan_sweeps(&pool).await.unwrap(),
            0
        );
        let terminal_payload = sqlx::query_scalar::<_, serde_json::Value>(
            "select payload from background_jobs where id = $1",
        )
        .bind(job.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            terminal_payload
                .get("maintenance")
                .and_then(serde_json::Value::as_str),
            Some("orphan_sweep")
        );

        sqlx::query(
            r#"
            update background_jobs
            set finished_at = now() - interval '2 days',
                updated_at = now() - interval '2 days'
            where id = $1
            "#,
        )
        .bind(job.id)
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(
            enqueue_due_tmdb_artwork_orphan_sweeps(&pool).await.unwrap(),
            1
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn pending_tmdb_binding_keeps_retention_and_expires_without_network(pool: sqlx::PgPool) {
        let (_, series_id, artwork_path) = seed_bound_tmdb_series_with_supporting_rows(&pool).await;
        let before = sqlx::query(
            r#"
            select provider_item_id, retain_until, remote_snapshot
            from tmdb_metadata_revalidations
            where media_item_id = $1
            "#,
        )
        .bind(series_id)
        .fetch_one(&pool)
        .await
        .unwrap();

        sqlx::query(
            r#"
            update media_items
            set metadata_status = 'pending',
                updated_at = clock_timestamp()
            where id = $1
            "#,
        )
        .bind(series_id)
        .execute(&pool)
        .await
        .unwrap();

        let after = sqlx::query(
            r#"
            select provider_item_id, retain_until, remote_snapshot
            from tmdb_metadata_revalidations
            where media_item_id = $1
            "#,
        )
        .bind(series_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(after.get::<String, _>("provider_item_id"), "42");
        assert_eq!(
            after.get::<OffsetDateTime, _>("retain_until"),
            before.get::<OffsetDateTime, _>("retain_until")
        );
        assert_eq!(
            after.get::<serde_json::Value, _>("remote_snapshot"),
            before.get::<serde_json::Value, _>("remote_snapshot")
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                r#"
                select
                    (select count(*) from media_item_cast_cache where media_item_id = $1)
                    + (
                        select count(*)
                        from series_episode_outline_cache
                        where series_media_item_id = $1
                    )
                "#,
            )
            .bind(series_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
            0
        );
        let outline_only_path = format!("{artwork_path}.outline-only");
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                r#"
                select count(*)
                from background_jobs
                where job_type = 'metadata.tmdb.artwork.cleanup'
                  and payload -> 'artwork_paths' ? $1
                "#,
            )
            .bind(&outline_only_path)
            .fetch_one(&pool)
            .await
            .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                r#"
                select count(*)
                from background_jobs
                where job_type = 'metadata.tmdb.artwork.cleanup'
                "#,
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                r#"
                select count(*)
                from background_jobs
                where job_type = 'metadata.tmdb.artwork.cleanup'
                  and payload -> 'artwork_paths' ? $1
                "#,
            )
            .bind(&artwork_path)
            .fetch_one(&pool)
            .await
            .unwrap(),
            1
        );

        sqlx::query(
            r#"
            update tmdb_metadata_revalidations
            set retain_until = now(),
                next_attempt_at = now() + interval '365 days',
                updated_at = clock_timestamp()
            where media_item_id = $1
            "#,
        )
        .bind(series_id)
        .execute(&pool)
        .await
        .unwrap();
        assert!(enqueue_due_tmdb_metadata_revalidation(&pool, false)
            .await
            .unwrap());
        let job = claim_background_job(&pool, "pending-retention-worker", 60)
            .await
            .unwrap()
            .claimed_job
            .unwrap();
        assert_eq!(job.job_type, TMDB_REVALIDATION_JOB_TYPE);
        assert_eq!(job.scope_id, series_id);
        assert!(
            get_tmdb_metadata_revalidation_target(&pool, series_id, "42")
                .await
                .unwrap()
                .is_some()
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn binding_change_clears_old_tmdb_supporting_state_and_queues_artwork(
        pool: sqlx::PgPool,
    ) {
        let (library_id, series_id, old_artwork_path) =
            seed_bound_tmdb_series_with_supporting_rows(&pool).await;
        let rebound_artwork_path =
            format!("/cache/libraries/{library_id}/artwork/tmdb/poster/rebound.jpg");

        sqlx::query(
            r#"
            update media_items
            set metadata_provider_item_id = '84',
                poster_path = $2,
                updated_at = clock_timestamp()
            where id = $1
            "#,
        )
        .bind(series_id)
        .bind(&rebound_artwork_path)
        .execute(&pool)
        .await
        .unwrap();

        let rebound = sqlx::query(
            r#"
            select provider_item_id, remote_snapshot
            from tmdb_metadata_revalidations
            where media_item_id = $1
            "#,
        )
        .bind(series_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(rebound.get::<String, _>("provider_item_id"), "84");
        assert_eq!(
            rebound.get::<serde_json::Value, _>("remote_snapshot"),
            serde_json::json!({})
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                r#"
                select
                    (select count(*) from media_item_cast_cache where media_item_id = $1)
                    + (
                        select count(*)
                        from series_episode_outline_cache
                        where series_media_item_id = $1
                    )
                    + (
                        select count(*)
                        from media_item_external_ids
                        where media_item_id = $1
                    )
                    + (
                        select count(*)
                        from media_item_ratings
                        where media_item_id = $1 and source = 'tmdb'
                    )
                "#,
            )
            .bind(series_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                r#"
                select count(*)
                from background_jobs
                where job_type = 'metadata.tmdb.artwork.cleanup'
                  and payload -> 'artwork_paths' ? $1
                "#,
            )
            .bind(&old_artwork_path)
            .fetch_one(&pool)
            .await
            .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                r#"
                select count(*)
                from background_jobs
                where job_type = 'metadata.tmdb.artwork.cleanup'
                "#,
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            1
        );

        let new_artwork_path =
            format!("/cache/libraries/{library_id}/artwork/tmdb/poster/new-binding.jpg");
        seed_tmdb_supporting_rows(&pool, library_id, series_id, "84", &new_artwork_path).await;
        sqlx::query(
            r#"
            update media_items
            set metadata_provider = 'other',
                metadata_provider_item_id = 'other-1',
                updated_at = clock_timestamp()
            where id = $1
            "#,
        )
        .bind(series_id)
        .execute(&pool)
        .await
        .unwrap();

        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "select count(*) from tmdb_metadata_revalidations where media_item_id = $1",
            )
            .bind(series_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                r#"
                select
                    (select count(*) from media_item_cast_cache where media_item_id = $1)
                    + (
                        select count(*)
                        from series_episode_outline_cache
                        where series_media_item_id = $1
                    )
                    + (
                        select count(*)
                        from media_item_external_ids
                        where media_item_id = $1
                    )
                    + (
                        select count(*)
                        from media_item_ratings
                        where media_item_id = $1 and source = 'tmdb'
                    )
                "#,
            )
            .bind(series_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                r#"
                select count(*)
                from background_jobs
                where job_type = 'metadata.tmdb.artwork.cleanup'
                  and payload -> 'artwork_paths' ? $1
                "#,
            )
            .bind(&new_artwork_path)
            .fetch_one(&pool)
            .await
            .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                r#"
                select count(*)
                from background_jobs
                where job_type = 'metadata.tmdb.artwork.cleanup'
                "#,
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            2
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn terminal_revalidation_error_does_not_retain_provider_identity(pool: sqlx::PgPool) {
        let (_, media_item_id) = seed_bound_tmdb_movie(&pool).await;
        sqlx::query(
            "update tmdb_metadata_revalidations set next_attempt_at = now() where media_item_id = $1",
        )
        .bind(media_item_id)
        .execute(&pool)
        .await
        .unwrap();
        assert!(enqueue_due_tmdb_metadata_revalidation(&pool, true)
            .await
            .unwrap());

        let job = claim_background_job(&pool, "terminal-error-worker", 60)
            .await
            .unwrap()
            .claimed_job
            .unwrap();
        let sensitive_error = "TMDB revalidation returned provider id 84 for expected id 42";
        let outcome = retry_or_fail_background_job(
            &pool,
            &job.execution_fence().unwrap(),
            sensitive_error,
            60,
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(outcome.status, "failed");

        let terminal = sqlx::query("select payload, last_error from background_jobs where id = $1")
            .bind(job.id)
            .fetch_one(&pool)
            .await
            .unwrap();
        let payload = terminal.get::<serde_json::Value, _>("payload");
        let last_error = terminal.get::<String, _>("last_error");
        assert_eq!(last_error, TMDB_MAINTENANCE_FAILED_ERROR);
        assert!(!last_error.contains("42"));
        assert!(!last_error.contains("84"));
        assert!(payload.get("provider_item_id").is_none());
        assert_eq!(
            payload
                .get("media_item_id")
                .and_then(serde_json::Value::as_i64),
            Some(media_item_id)
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn abandoned_revalidation_records_durable_backoff(pool: sqlx::PgPool) {
        let (_, media_item_id) = seed_bound_tmdb_movie(&pool).await;
        sqlx::query(
            "update tmdb_metadata_revalidations set next_attempt_at = now() where media_item_id = $1",
        )
        .bind(media_item_id)
        .execute(&pool)
        .await
        .unwrap();
        assert!(enqueue_due_tmdb_metadata_revalidation(&pool, true)
            .await
            .unwrap());

        let job = claim_background_job(&pool, "abandoned-revalidation-worker", 60)
            .await
            .unwrap()
            .claimed_job
            .unwrap();
        sqlx::query(
            "update background_jobs set lease_expires_at = now() - interval '1 second' where id = $1",
        )
        .bind(job.id)
        .execute(&pool)
        .await
        .unwrap();

        let outcome = claim_background_job(&pool, "recovery-worker", 60)
            .await
            .unwrap();
        assert_eq!(outcome.terminalized_jobs.len(), 1);
        assert_eq!(outcome.terminalized_jobs[0].job.status, "failed");
        assert!(outcome.claimed_job.is_none());

        let state = sqlx::query(
            r#"
            select last_attempt_at, next_attempt_at, consecutive_failures
            from tmdb_metadata_revalidations
            where media_item_id = $1
            "#,
        )
        .bind(media_item_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        let last_attempt_at = state.get::<OffsetDateTime, _>("last_attempt_at");
        let next_attempt_at = state.get::<OffsetDateTime, _>("next_attempt_at");
        assert!(next_attempt_at > last_attempt_at);
        assert_eq!(state.get::<i32, _>("consecutive_failures"), 1);
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn stale_failure_and_defer_do_not_override_a_concurrent_manual_write(pool: sqlx::PgPool) {
        let (_, media_item_id) = seed_bound_tmdb_movie(&pool).await;
        sqlx::query(
            "update tmdb_metadata_revalidations set next_attempt_at = now() where media_item_id = $1",
        )
        .bind(media_item_id)
        .execute(&pool)
        .await
        .unwrap();
        assert!(enqueue_due_tmdb_metadata_revalidation(&pool, true)
            .await
            .unwrap());
        let job = claim_background_job(&pool, "concurrent-manual-worker", 60)
            .await
            .unwrap()
            .claimed_job
            .unwrap();
        let fence = job.execution_fence().unwrap();
        let target = get_tmdb_metadata_revalidation_target(&pool, media_item_id, "42")
            .await
            .unwrap()
            .unwrap();

        sqlx::query(
            r#"
            update media_items
            set overview = 'manual override',
                updated_at = clock_timestamp()
            where id = $1
            "#,
        )
        .bind(media_item_id)
        .execute(&pool)
        .await
        .unwrap();

        record_tmdb_metadata_revalidation_failure(&pool, &fence, media_item_id, "42")
            .await
            .unwrap();
        assert!(!defer_tmdb_revalidation_until_retention_deadline(
            &pool,
            &fence,
            media_item_id,
            "42",
            target.observed_media_item_updated_at,
            target.observed_revalidation_updated_at,
        )
        .await
        .unwrap());

        assert_eq!(
            sqlx::query_scalar::<_, i32>(
                "select consecutive_failures from tmdb_metadata_revalidations where media_item_id = $1",
            )
            .bind(media_item_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
            0
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn disabled_provider_only_enqueues_local_retention_expiry(pool: sqlx::PgPool) {
        let (_, media_item_id) = seed_bound_tmdb_movie(&pool).await;
        sqlx::query(
            r#"
            update tmdb_metadata_revalidations
            set next_attempt_at = now(),
                retain_until = now() + interval '30 days'
            where media_item_id = $1
            "#,
        )
        .bind(media_item_id)
        .execute(&pool)
        .await
        .unwrap();

        assert!(!enqueue_due_tmdb_metadata_revalidation(&pool, false)
            .await
            .unwrap());

        sqlx::query(
            "update tmdb_metadata_revalidations set retain_until = now() where media_item_id = $1",
        )
        .bind(media_item_id)
        .execute(&pool)
        .await
        .unwrap();
        assert!(enqueue_due_tmdb_metadata_revalidation(&pool, false)
            .await
            .unwrap());
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn retention_expiry_removes_provider_state_without_deleting_local_item(
        pool: sqlx::PgPool,
    ) {
        let (_, media_item_id) = seed_bound_tmdb_movie(&pool).await;
        sqlx::query(
            "update tmdb_metadata_revalidations set retain_until = now() where media_item_id = $1",
        )
        .bind(media_item_id)
        .execute(&pool)
        .await
        .unwrap();
        assert!(enqueue_due_tmdb_metadata_revalidation(&pool, false)
            .await
            .unwrap());
        let job = claim_background_job(&pool, "retention-worker", 60)
            .await
            .unwrap()
            .claimed_job
            .unwrap();
        let target = get_tmdb_metadata_revalidation_target(&pool, media_item_id, "42")
            .await
            .unwrap()
            .unwrap();

        assert!(expire_tmdb_metadata_retention(
            &pool,
            &job.execution_fence().unwrap(),
            ExpireTmdbMetadataRetentionParams {
                media_item_id,
                library_id: target.library_id,
                provider_item_id: "42".to_string(),
                observed_media_item_updated_at: target.observed_media_item_updated_at,
                observed_revalidation_updated_at: target.observed_revalidation_updated_at,
                title: "Local Movie".to_string(),
                original_title: None,
                year: None,
                country: None,
                genres: None,
                studio: None,
                overview: None,
                poster_path: None,
                backdrop_path: None,
                logo_path: None,
                seasons: Vec::new(),
                episodes: Vec::new(),
                artwork_cleanup_paths: vec![
                    "/cache/libraries/1/artwork/tmdb/poster/expired.jpg".to_string(),
                ],
            },
        )
        .await
        .unwrap());

        let row = sqlx::query(
            r#"
            select title, metadata_provider, metadata_provider_item_id, metadata_status
            from media_items
            where id = $1
            "#,
        )
        .bind(media_item_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.get::<String, _>("title"), "Local Movie");
        assert_eq!(row.get::<Option<String>, _>("metadata_provider"), None);
        assert_eq!(
            row.get::<Option<String>, _>("metadata_provider_item_id"),
            None
        );
        assert_eq!(row.get::<String, _>("metadata_status"), "pending");
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "select count(*) from tmdb_metadata_revalidations where media_item_id = $1",
            )
            .bind(media_item_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "select count(*) from media_item_external_ids where media_item_id = $1",
            )
            .bind(media_item_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "select count(*) from notifications where notification_type = 'metadata.tmdb.retention_expired'",
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            1
        );
        let notification_payload = sqlx::query_scalar::<_, serde_json::Value>(
            "select payload from notifications where notification_type = 'metadata.tmdb.retention_expired'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(notification_payload.get("provider_item_id").is_none());

        let cleanup = sqlx::query(
            r#"
            select id, status, max_attempts, payload
            from background_jobs
            where job_type = $1
            "#,
        )
        .bind(TMDB_ARTWORK_CLEANUP_JOB_TYPE)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "select count(*) from background_jobs where job_type = $1",
            )
            .bind(TMDB_ARTWORK_CLEANUP_JOB_TYPE)
            .fetch_one(&pool)
            .await
            .unwrap(),
            1
        );
        assert_eq!(cleanup.get::<String, _>("status"), "pending");
        assert_eq!(cleanup.get::<i32, _>("max_attempts"), i32::MAX);
        assert_eq!(
            cleanup
                .get::<serde_json::Value, _>("payload")
                .get("artwork_paths")
                .and_then(serde_json::Value::as_array)
                .map(Vec::len),
            Some(1)
        );

        complete_background_job(&pool, &job.execution_fence().unwrap())
            .await
            .unwrap();
        let cleanup_job_id = cleanup.get::<i64, _>("id");
        sqlx::query(
            r#"
            update background_jobs
            set attempt_count = max_attempts - 1,
                run_after = now()
            where id = $1
            "#,
        )
        .bind(cleanup_job_id)
        .execute(&pool)
        .await
        .unwrap();
        let cleanup_job = claim_background_job(&pool, "cleanup-worker", 60)
            .await
            .unwrap()
            .claimed_job
            .unwrap();
        assert_eq!(cleanup_job.id, cleanup_job_id);
        assert_eq!(cleanup_job.attempt_count, i32::MAX);
        let sensitive_cleanup_error =
            "provider 42 cleanup failed at /cache/libraries/1/artwork/tmdb/poster/expired.jpg";
        let retried = retry_or_fail_background_job(
            &pool,
            &cleanup_job.execution_fence().unwrap(),
            sensitive_cleanup_error,
            86_400,
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(retried.status, "pending");
        let retry_state = sqlx::query(
            "select attempt_count, payload, last_error from background_jobs where id = $1",
        )
        .bind(cleanup_job_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(retry_state.get::<i32, _>("attempt_count"), i32::MAX - 1);
        assert_eq!(
            retry_state.get::<String, _>("last_error"),
            TMDB_MAINTENANCE_RETRY_ERROR
        );
        assert!(!retry_state
            .get::<String, _>("last_error")
            .contains("/cache"));
        assert!(retry_state
            .get::<serde_json::Value, _>("payload")
            .get("artwork_paths")
            .is_some());

        sqlx::query("update background_jobs set run_after = now() where id = $1")
            .bind(cleanup_job_id)
            .execute(&pool)
            .await
            .unwrap();
        let final_cleanup_job = claim_background_job(&pool, "cleanup-worker", 60)
            .await
            .unwrap()
            .claimed_job
            .unwrap();
        complete_background_job(&pool, &final_cleanup_job.execution_fence().unwrap())
            .await
            .unwrap();
        let terminal = sqlx::query("select payload, last_error from background_jobs where id = $1")
            .bind(cleanup_job_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        let terminal_payload = terminal.get::<serde_json::Value, _>("payload");
        assert_eq!(terminal.get::<Option<String>, _>("last_error"), None);
        assert!(terminal_payload.get("artwork_paths").is_none());
        assert_eq!(
            terminal_payload
                .get("media_item_id")
                .and_then(serde_json::Value::as_i64),
            Some(media_item_id)
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn series_retention_expiry_clears_persisted_season_episode_and_outline_state(
        pool: sqlx::PgPool,
    ) {
        let library_id = sqlx::query_scalar::<_, i64>(
            "insert into libraries (name, root_path) values ('Series', '/media/series') returning id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let series_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into media_items (
                library_id, media_type, title, source_title,
                metadata_provider, metadata_provider_item_id, metadata_status,
                remote_media_type, overview, poster_path
            )
            values (
                $1, 'series', 'TMDB Series', 'Local Series',
                'tmdb', '77', 'matched', 'series', 'TMDB overview',
                '/cache/libraries/1/artwork/tmdb/poster/series.jpg'
            )
            returning id
            "#,
        )
        .bind(library_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        let season_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into seasons (
                library_id, series_id, season_number, title, overview, poster_path
            )
            values (
                $1, $2, 1, 'TMDB Season', 'TMDB season overview',
                '/cache/libraries/1/artwork/tmdb/poster/season.jpg'
            )
            returning id
            "#,
        )
        .bind(library_id)
        .bind(series_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        let episode_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into media_items (
                library_id, media_type, title, source_title,
                metadata_provider, metadata_provider_item_id, metadata_status,
                remote_media_type, overview, poster_path
            )
            values (
                $1, 'episode', 'TMDB Episode', 'Episode 01',
                'tmdb', '77', 'matched', 'series', 'TMDB episode overview',
                '/cache/libraries/1/artwork/tmdb/poster/episode.jpg'
            )
            returning id
            "#,
        )
        .bind(library_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            insert into episodes (
                media_item_id, library_id, season_id, episode_number
            )
            values ($1, $2, $3, 1)
            "#,
        )
        .bind(episode_id)
        .bind(library_id)
        .bind(season_id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            insert into media_item_external_ids (media_item_id, provider, external_id)
            values
                ($1, 'tmdb', '77'),
                ($1, 'imdb', 'tt-series-episode')
            "#,
        )
        .bind(episode_id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            insert into media_item_ratings (
                media_item_id,
                source,
                kind,
                score,
                scale,
                retrieved_via,
                fetched_at
            )
            values ($1, 'tmdb', 'user', 8, 10, 'api', now())
            "#,
        )
        .bind(episode_id)
        .execute(&pool)
        .await
        .unwrap();
        let nfo_episode_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into media_items (
                library_id, media_type, title, source_title,
                metadata_provider, metadata_provider_item_id, metadata_status,
                overview, poster_path
            )
            values (
                $1, 'episode', 'NFO Episode', 'Episode 02',
                'nfo', 'local-episode-2', 'matched',
                'NFO overview', '/media/series/episode-02.jpg'
            )
            returning id
            "#,
        )
        .bind(library_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            insert into episodes (
                media_item_id, library_id, season_id, episode_number
            )
            values ($1, $2, $3, 2)
            "#,
        )
        .bind(nfo_episode_id)
        .bind(library_id)
        .bind(season_id)
        .execute(&pool)
        .await
        .unwrap();
        let nfo_source_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into media_local_metadata_sources (
                library_id, media_item_id, source_path, document_type, is_selected, payload
            )
            values (
                $1, $2, '/media/series/episode-02.nfo', 'episodedetails', true, '{}'
            )
            returning id
            "#,
        )
        .bind(library_id)
        .bind(nfo_episode_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            insert into media_item_external_ids (
                media_item_id, provider, external_id, retrieved_via,
                local_metadata_source_id
            )
            values ($1, 'tmdb', 'nfo-authored-tmdb-id', 'nfo', $2)
            "#,
        )
        .bind(nfo_episode_id)
        .bind(nfo_source_id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            insert into media_item_ratings (
                media_item_id,
                source,
                kind,
                score,
                scale,
                retrieved_via,
                local_metadata_source_id,
                fetched_at
            )
            values ($1, 'tmdb', 'user', 9, 10, 'nfo', $2, now())
            "#,
        )
        .bind(nfo_episode_id)
        .bind(nfo_source_id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            insert into series_episode_outline_cache (
                series_media_item_id, library_id, outline_json, expires_at
            )
            values ($1, $2, '{"seasons":[]}', now() + interval '1 day')
            "#,
        )
        .bind(series_id)
        .bind(library_id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            update tmdb_metadata_revalidations
            set remote_snapshot = $2,
                retain_until = now(),
                next_attempt_at = now(),
                updated_at = clock_timestamp()
            where media_item_id = $1
            "#,
        )
        .bind(series_id)
        .bind(serde_json::json!({
            "version": 1,
            "title": "TMDB Series",
            "overview": "TMDB overview",
            "poster_path": "/cache/libraries/1/artwork/tmdb/poster/series.jpg",
            "series_outline": {
                "seasons": [{
                    "season_number": 1,
                    "title": "TMDB Season",
                    "overview": "TMDB season overview",
                    "poster_path": "/cache/libraries/1/artwork/tmdb/poster/season.jpg",
                    "episodes": [{
                        "episode_number": 1,
                        "title": "TMDB Episode",
                        "overview": "TMDB episode overview",
                        "poster_path": "/cache/libraries/1/artwork/tmdb/poster/episode.jpg"
                    }]
                }]
            }
        }))
        .execute(&pool)
        .await
        .unwrap();
        assert!(enqueue_due_tmdb_metadata_revalidation(&pool, false)
            .await
            .unwrap());
        let job = claim_background_job(&pool, "series-retention-worker", 60)
            .await
            .unwrap()
            .claimed_job
            .unwrap();
        let target = get_tmdb_metadata_revalidation_target(&pool, series_id, "77")
            .await
            .unwrap()
            .unwrap();

        assert!(expire_tmdb_metadata_retention(
            &pool,
            &job.execution_fence().unwrap(),
            ExpireTmdbMetadataRetentionParams {
                media_item_id: series_id,
                library_id: target.library_id,
                provider_item_id: "77".to_string(),
                observed_media_item_updated_at: target.observed_media_item_updated_at,
                observed_revalidation_updated_at: target.observed_revalidation_updated_at,
                title: "Local Series".to_string(),
                original_title: None,
                year: None,
                country: None,
                genres: None,
                studio: None,
                overview: None,
                poster_path: None,
                backdrop_path: None,
                logo_path: None,
                seasons: target
                    .seasons
                    .iter()
                    .map(|season| ReplaceTmdbRevalidationSeason {
                        season_id: season.season_id,
                        observed_updated_at: season.observed_updated_at,
                        title: Some("Season 01".to_string()),
                        overview: None,
                        poster_path: None,
                        backdrop_path: None,
                    })
                    .collect(),
                episodes: target
                    .episodes
                    .iter()
                    .map(|episode| ReplaceTmdbRevalidationEpisode {
                        media_item_id: episode.media_item_id,
                        observed_updated_at: episode.observed_updated_at,
                        title: if episode.media_item_id == nfo_episode_id {
                            episode.title.clone()
                        } else {
                            episode.source_title.clone()
                        },
                        overview: (episode.media_item_id == nfo_episode_id)
                            .then(|| episode.overview.clone())
                            .flatten(),
                        poster_path: (episode.media_item_id == nfo_episode_id)
                            .then(|| episode.poster_path.clone())
                            .flatten(),
                        backdrop_path: (episode.media_item_id == nfo_episode_id)
                            .then(|| episode.backdrop_path.clone())
                            .flatten(),
                    })
                    .collect(),
                artwork_cleanup_paths: vec![
                    "/cache/libraries/1/artwork/tmdb/poster/series.jpg".to_string(),
                    "/cache/libraries/1/artwork/tmdb/poster/season.jpg".to_string(),
                    "/cache/libraries/1/artwork/tmdb/poster/episode.jpg".to_string(),
                ],
            },
        )
        .await
        .unwrap());

        let season = sqlx::query("select title, overview, poster_path from seasons where id = $1")
            .bind(season_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            season.get::<Option<String>, _>("title").as_deref(),
            Some("Season 01")
        );
        assert_eq!(season.get::<Option<String>, _>("overview"), None);
        assert_eq!(season.get::<Option<String>, _>("poster_path"), None);
        let episode = sqlx::query(
            r#"
            select title, metadata_provider, metadata_provider_item_id,
                   metadata_status, overview, poster_path
            from media_items
            where id = $1
            "#,
        )
        .bind(episode_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(episode.get::<String, _>("title"), "Episode 01");
        assert_eq!(episode.get::<Option<String>, _>("metadata_provider"), None);
        assert_eq!(
            episode.get::<Option<String>, _>("metadata_provider_item_id"),
            None
        );
        assert_eq!(episode.get::<String, _>("metadata_status"), "pending");
        assert_eq!(episode.get::<Option<String>, _>("overview"), None);
        assert_eq!(episode.get::<Option<String>, _>("poster_path"), None);
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                r#"
                select
                    (select count(*) from media_item_external_ids where media_item_id = $1)
                    + (select count(*) from media_item_ratings where media_item_id = $1)
                "#,
            )
            .bind(episode_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
            0
        );
        let nfo_episode = sqlx::query(
            r#"
            select title, metadata_provider, metadata_provider_item_id,
                   metadata_status, overview, poster_path
            from media_items
            where id = $1
            "#,
        )
        .bind(nfo_episode_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(nfo_episode.get::<String, _>("title"), "NFO Episode");
        assert_eq!(
            nfo_episode
                .get::<Option<String>, _>("metadata_provider")
                .as_deref(),
            Some("nfo")
        );
        assert_eq!(
            nfo_episode
                .get::<Option<String>, _>("metadata_provider_item_id")
                .as_deref(),
            Some("local-episode-2")
        );
        assert_eq!(nfo_episode.get::<String, _>("metadata_status"), "matched");
        assert_eq!(
            nfo_episode.get::<Option<String>, _>("overview").as_deref(),
            Some("NFO overview")
        );
        assert_eq!(
            nfo_episode
                .get::<Option<String>, _>("poster_path")
                .as_deref(),
            Some("/media/series/episode-02.jpg")
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                r#"
                select
                    (select count(*) from media_item_external_ids where media_item_id = $1)
                    + (select count(*) from media_item_ratings where media_item_id = $1)
                "#,
            )
            .bind(nfo_episode_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
            2
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "select count(*) from series_episode_outline_cache where series_media_item_id = $1",
            )
            .bind(series_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
            0
        );
    }
}
