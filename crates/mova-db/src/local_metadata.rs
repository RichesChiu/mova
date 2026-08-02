use anyhow::{bail, Context, Result};
use mova_domain::{
    MediaExternalId, MediaExternalIdRecord, MediaItemCredit, MediaLocalMetadataSource,
    MediaLocalMetadataSourceSummary, MediaRating,
};
use serde_json::Value;
use sqlx::{postgres::PgPool, Postgres, QueryBuilder, Row, Transaction};
use std::{collections::HashSet, path::Path};

pub const LOCAL_METADATA_RETRIEVED_VIA: &str = "nfo";

const POSTGRES_MAX_BIND_PARAMETERS: usize = 65_535;
const NFO_CREDIT_BIND_PARAMETERS_PER_ROW: usize = 9;
const NFO_CREDIT_INSERT_BATCH_SIZE: usize = 1_000;
const _: () = assert!(
    NFO_CREDIT_INSERT_BATCH_SIZE * NFO_CREDIT_BIND_PARAMETERS_PER_ROW
        <= POSTGRES_MAX_BIND_PARAMETERS
);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaLocalMetadataTarget {
    MediaItem(i64),
}

#[derive(Debug, Clone)]
pub struct ReplaceMediaItemCreditParams {
    pub credit_type: String,
    pub sort_order: i32,
    /// Provider namespace for `person_id`. NFO ingestion currently persists
    /// only TMDB person identifiers, but keeping the namespace in storage
    /// prevents future providers from sharing one ambiguous identifier space.
    pub person_provider: Option<String>,
    pub person_id: Option<String>,
    pub name: String,
    pub role: Option<String>,
    pub profile_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ReplaceMediaLocalMetadataSourceParams {
    pub library_id: i64,
    pub target: MediaLocalMetadataTarget,
    pub source_path: String,
    pub document_type: String,
    pub is_locked: bool,
    /// The application selects one stable sidecar within a local scan group.
    /// Persistence resolves selected candidates from groups that later merge
    /// into one media item, so group completion order cannot choose the winner.
    pub is_selected: bool,
    pub payload: Value,
    pub external_ids: Vec<MediaExternalId>,
    pub ratings: Vec<MediaRating>,
    pub credits: Vec<ReplaceMediaItemCreditParams>,
}

pub async fn replace_media_local_metadata_source(
    pool: &PgPool,
    params: ReplaceMediaLocalMetadataSourceParams,
) -> Result<MediaLocalMetadataSource> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to start local metadata replacement transaction")?;
    let source = replace_media_local_metadata_source_tx(&mut tx, params).await?;
    tx.commit()
        .await
        .context("failed to commit local metadata replacement transaction")?;
    Ok(source)
}

pub async fn replace_media_local_metadata_source_tx(
    tx: &mut Transaction<'_, Postgres>,
    params: ReplaceMediaLocalMetadataSourceParams,
) -> Result<MediaLocalMetadataSource> {
    validate_replace_params(&params)?;

    let MediaLocalMetadataTarget::MediaItem(media_item_id) = params.target;
    let target_binding = sqlx::query_as::<_, (Option<String>, Option<String>)>(
        r#"
        select metadata_provider, metadata_provider_item_id
        from media_items
        where id = $1 and library_id = $2
        for update
        "#,
    )
    .bind(media_item_id)
    .bind(params.library_id)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to lock local metadata target")?
    .context("local metadata target does not exist")?;

    let selected_source = sqlx::query_as::<_, (i64, String, String)>(
        r#"
        select id, source_path, document_type
        from media_local_metadata_sources
        where library_id = $1
          and media_item_id = $2
          and is_selected
        for update
        "#,
    )
    .bind(params.library_id)
    .bind(media_item_id)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to lock selected local metadata source")?;
    let selected_external_id = if let (Some((selected_source_id, _, _)), Some(provider), Some(_)) = (
        selected_source.as_ref(),
        target_binding.0.as_deref(),
        target_binding.1.as_deref(),
    ) {
        sqlx::query_scalar::<_, String>(
            r#"
            select external_id
            from media_item_external_ids
            where media_item_id = $1
              and provider = $2
              and retrieved_via = 'nfo'
              and local_metadata_source_id = $3
            "#,
        )
        .bind(media_item_id)
        .bind(provider)
        .bind(selected_source_id)
        .fetch_optional(&mut **tx)
        .await
        .context("failed to load selected NFO identity")?
    } else {
        None
    };

    let existing = sqlx::query(
        r#"
        select
            id,
            library_id,
            media_item_id,
            source_path,
            document_type,
            schema_version,
            is_locked,
            is_selected,
            payload,
            created_at,
            updated_at
        from media_local_metadata_sources
        where library_id = $1 and source_path = $2
        for update
        "#,
    )
    .bind(params.library_id)
    .bind(&params.source_path)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to lock existing local metadata source")?;

    let replaces_selected_source = existing.as_ref().is_some_and(|row| {
        row.get::<Option<i64>, _>("media_item_id") == Some(media_item_id)
            && row.get::<bool, _>("is_selected")
    });

    // Each scan group contributes at most one selected candidate. Keeping the
    // best candidate seen so far makes the final choice the global minimum of
    // all group winners, regardless of the order in which groups commit.
    let select_incoming = params.is_selected
        && (replaces_selected_source
            || selected_source
                .as_ref()
                .is_none_or(|(_, source_path, document_type)| {
                    incoming_source_precedes_selected(
                        &params,
                        source_path,
                        document_type,
                        selected_external_id.as_deref(),
                        target_binding.0.as_deref(),
                        target_binding.1.as_deref(),
                    )
                }));

    // A shared `tvshow.nfo` is carried by every episode in a series group.
    // Reusing an unchanged source row keeps the expensive structured
    // projection (especially a large cast) at sources x credits rather than
    // episodes x credits. The normalized payload also contains the runtime
    // identity-projection decision, so a binding change still forces the
    // required refresh.
    if existing.as_ref().is_some_and(|row| {
        row.get::<Option<i64>, _>("media_item_id") == Some(media_item_id)
            && row.get::<String, _>("document_type") == params.document_type
            && row.get::<i32, _>("schema_version") == 1
            && row.get::<bool, _>("is_locked") == params.is_locked
            && row.get::<bool, _>("is_selected") == select_incoming
            && row.get::<Value, _>("payload") == params.payload
    }) {
        return Ok(map_local_metadata_source(
            existing.context("unchanged local metadata source disappeared")?,
        ));
    }

    sqlx::query(
        r#"
        delete from media_local_metadata_sources
        where library_id = $1 and source_path = $2
        "#,
    )
    .bind(params.library_id)
    .bind(&params.source_path)
    .execute(&mut **tx)
    .await
    .context("failed to replace existing local metadata source")?;

    if select_incoming {
        sqlx::query(
            r#"
            update media_local_metadata_sources
            set is_selected = false,
                updated_at = now()
            where library_id = $1
              and media_item_id = $2
              and is_selected
            "#,
        )
        .bind(params.library_id)
        .bind(media_item_id)
        .execute(&mut **tx)
        .await
        .context("failed to replace selected local metadata source")?;
    }

    let row = sqlx::query(
        r#"
        insert into media_local_metadata_sources (
            library_id,
            media_item_id,
            source_path,
            document_type,
            schema_version,
            is_locked,
            is_selected,
            payload
        )
        values ($1, $2, $3, $4, 1, $5, $6, $7)
        returning
            id,
            library_id,
            media_item_id,
            source_path,
            document_type,
            schema_version,
            is_locked,
            is_selected,
            payload,
            created_at,
            updated_at
        "#,
    )
    .bind(params.library_id)
    .bind(media_item_id)
    .bind(&params.source_path)
    .bind(&params.document_type)
    .bind(params.is_locked)
    .bind(select_incoming)
    .bind(&params.payload)
    .fetch_one(&mut **tx)
    .await
    .context("failed to insert local metadata source")?;

    if select_incoming {
        let local_metadata_source_id = row.get::<i64, _>("id");
        replace_nfo_projection_tx(
            tx,
            local_metadata_source_id,
            media_item_id,
            &params.external_ids,
            &params.ratings,
            &params.credits,
        )
        .await?;
    }

    Ok(map_local_metadata_source(row))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum BindingPreference {
    Matches,
    Unspecified,
    Conflicts,
}

fn incoming_source_precedes_selected(
    incoming: &ReplaceMediaLocalMetadataSourceParams,
    selected_source_path: &str,
    selected_document_type: &str,
    selected_external_id: Option<&str>,
    metadata_provider: Option<&str>,
    metadata_provider_item_id: Option<&str>,
) -> bool {
    let incoming_external_id = metadata_provider.and_then(|provider| {
        incoming
            .external_ids
            .iter()
            .find(|external_id| external_id.provider.eq_ignore_ascii_case(provider))
            .map(|external_id| external_id.external_id.as_str())
    });
    source_selection_key(
        &incoming.source_path,
        &incoming.document_type,
        binding_preference(incoming_external_id, metadata_provider_item_id),
    ) < source_selection_key(
        selected_source_path,
        selected_document_type,
        binding_preference(selected_external_id, metadata_provider_item_id),
    )
}

fn binding_preference(
    source_external_id: Option<&str>,
    metadata_provider_item_id: Option<&str>,
) -> BindingPreference {
    match (source_external_id, metadata_provider_item_id) {
        (Some(source), Some(binding)) if source == binding => BindingPreference::Matches,
        (None, _) | (_, None) => BindingPreference::Unspecified,
        (Some(_), Some(_)) => BindingPreference::Conflicts,
    }
}

fn source_selection_key<'a>(
    source_path: &'a str,
    document_type: &str,
    binding: BindingPreference,
) -> (BindingPreference, bool, &'a str) {
    let is_generic_movie_nfo = document_type.eq_ignore_ascii_case("movie")
        && Path::new(source_path)
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("movie.nfo"));
    (binding, is_generic_movie_nfo, source_path)
}

pub async fn remove_media_local_metadata_source(
    pool: &PgPool,
    library_id: i64,
    source_path: &str,
) -> Result<bool> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to start local metadata removal transaction")?;
    let removed = remove_media_local_metadata_source_tx(&mut tx, library_id, source_path).await?;
    tx.commit()
        .await
        .context("failed to commit local metadata removal transaction")?;
    Ok(removed)
}

pub async fn remove_media_local_metadata_source_tx(
    tx: &mut Transaction<'_, Postgres>,
    library_id: i64,
    source_path: &str,
) -> Result<bool> {
    let source_id = sqlx::query_scalar::<_, i64>(
        r#"
        select id
        from media_local_metadata_sources
        where library_id = $1 and source_path = $2
        for update
        "#,
    )
    .bind(library_id)
    .bind(source_path)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to lock local metadata source for removal")?;
    let Some(source_id) = source_id else {
        return Ok(false);
    };

    sqlx::query("delete from media_local_metadata_sources where id = $1")
        .bind(source_id)
        .execute(&mut **tx)
        .await
        .context("failed to remove local metadata source")?;

    Ok(true)
}

/// Reconcile the library-wide set of sidecars after a complete, authoritative
/// media discovery. A source is retained only when at least one live video can
/// still select that exact candidate path. Invalid or temporarily unreadable
/// candidates remain in the retained set at the application boundary so their
/// last-known-good payload is not discarded.
pub(crate) async fn reconcile_library_local_metadata_source_paths_tx(
    tx: &mut Transaction<'_, Postgres>,
    library_id: i64,
    retained_source_paths: &[String],
) -> Result<usize> {
    let retained = retained_source_paths.iter().collect::<HashSet<_>>();
    let existing_sources = sqlx::query_as::<_, (String, bool)>(
        r#"
        select source_path, is_selected
        from media_local_metadata_sources
        where library_id = $1
        order by source_path
        for update
        "#,
    )
    .bind(library_id)
    .fetch_all(&mut **tx)
    .await
    .context("failed to lock library local metadata sources for reconciliation")?;

    let stale_sources = existing_sources
        .into_iter()
        .filter(|(source_path, _)| !retained.contains(source_path))
        .collect::<Vec<_>>();
    let stale_paths = stale_sources
        .into_iter()
        .filter_map(|(source_path, is_selected)| {
            if is_selected {
                // The complete-inventory planner normally forces this owner
                // through field re-projection before final reconciliation. If
                // a concurrent filesystem change or a best-effort group error
                // still leaves a selected source here, retain its last-known-
                // good projection for this pass instead of making every later
                // scan fail at the same assertion. The next authoritative plan
                // will mark the owner touched and remove it safely.
                tracing::warn!(
                    library_id,
                    source_path,
                    "selected local metadata source was no longer eligible at final reconciliation"
                );
                None
            } else {
                Some(source_path)
            }
        })
        .collect::<Vec<_>>();
    for source_path in &stale_paths {
        remove_media_local_metadata_source_tx(tx, library_id, source_path).await?;
    }

    Ok(stale_paths.len())
}

pub async fn list_media_local_metadata_sources(
    pool: &PgPool,
    library_id: i64,
) -> Result<Vec<MediaLocalMetadataSource>> {
    let rows = sqlx::query(
        r#"
        select
            id,
            library_id,
            media_item_id,
            source_path,
            document_type,
            schema_version,
            is_locked,
            is_selected,
            payload,
            created_at,
            updated_at
        from media_local_metadata_sources
        where library_id = $1
        order by source_path asc, id asc
        "#,
    )
    .bind(library_id)
    .fetch_all(pool)
    .await
    .context("failed to list local metadata sources")?;

    Ok(rows.into_iter().map(map_local_metadata_source).collect())
}

pub async fn list_media_local_metadata_sources_for_item(
    pool: &PgPool,
    media_item_id: i64,
) -> Result<Vec<MediaLocalMetadataSource>> {
    let rows = sqlx::query(
        r#"
        select
            id,
            library_id,
            media_item_id,
            source_path,
            document_type,
            schema_version,
            is_locked,
            is_selected,
            payload,
            created_at,
            updated_at
        from media_local_metadata_sources
        where media_item_id = $1
        order by is_selected desc, source_path asc, id asc
        "#,
    )
    .bind(media_item_id)
    .fetch_all(pool)
    .await
    .context("failed to list media item local metadata sources")?;

    Ok(rows.into_iter().map(map_local_metadata_source).collect())
}

/// List persisted source headers without selecting their normalized JSON
/// payload. This query backs the source index API and must remain cheap enough
/// to call before an administrator chooses one source to inspect.
pub async fn list_media_local_metadata_source_summaries_for_item(
    pool: &PgPool,
    media_item_id: i64,
) -> Result<Vec<MediaLocalMetadataSourceSummary>> {
    let rows = sqlx::query(
        r#"
        select
            id,
            library_id,
            media_item_id,
            source_path,
            document_type,
            schema_version,
            is_locked,
            is_selected,
            created_at,
            updated_at
        from media_local_metadata_sources
        where media_item_id = $1
        order by is_selected desc, source_path asc, id asc
        "#,
    )
    .bind(media_item_id)
    .fetch_all(pool)
    .await
    .context("failed to list media item local metadata source summaries")?;

    Ok(rows
        .into_iter()
        .map(map_local_metadata_source_summary)
        .collect())
}

/// Fetch one source only when its full normalized payload is requested.
/// Constraining by both item and source id prevents a caller from using a
/// source id discovered for another media item.
pub async fn get_media_local_metadata_source_for_item(
    pool: &PgPool,
    media_item_id: i64,
    source_id: i64,
) -> Result<Option<MediaLocalMetadataSource>> {
    let row = sqlx::query(
        r#"
        select
            id,
            library_id,
            media_item_id,
            source_path,
            document_type,
            schema_version,
            is_locked,
            is_selected,
            payload,
            created_at,
            updated_at
        from media_local_metadata_sources
        where media_item_id = $1 and id = $2
        "#,
    )
    .bind(media_item_id)
    .bind(source_id)
    .fetch_optional(pool)
    .await
    .context("failed to get media item local metadata source")?;

    Ok(row.map(map_local_metadata_source))
}

pub async fn list_media_item_external_ids(
    pool: &PgPool,
    media_item_id: i64,
) -> Result<Vec<MediaExternalIdRecord>> {
    let rows = sqlx::query(
        r#"
        select media_item_id, provider, external_id, retrieved_via
        from media_item_external_ids
        where media_item_id = $1
        order by
            provider asc,
            case retrieved_via
                when 'manual' then 0
                when 'nfo' then 1
                else 100
            end,
            retrieved_via asc
        "#,
    )
    .bind(media_item_id)
    .fetch_all(pool)
    .await
    .context("failed to list media item external ids")?;

    Ok(rows
        .into_iter()
        .map(|row| MediaExternalIdRecord {
            media_item_id: row.get("media_item_id"),
            provider: row.get("provider"),
            external_id: row.get("external_id"),
            retrieved_via: row.get("retrieved_via"),
        })
        .collect())
}

pub async fn list_media_item_credits(
    pool: &PgPool,
    media_item_id: i64,
) -> Result<Vec<MediaItemCredit>> {
    let rows = sqlx::query(
        r#"
        select
            id,
            media_item_id,
            credit_type,
            retrieved_via,
            sort_order,
            provider_person_id,
            name,
            role,
            profile_path
        from media_item_credits
        where media_item_id = $1
        order by
            case credit_type
                when 'actor' then 0
                when 'director' then 1
                when 'writer' then 2
                else 100
            end,
            case retrieved_via
                when 'manual' then 0
                when 'nfo' then 1
                else 100
            end,
            sort_order asc,
            name asc,
            id asc
        "#,
    )
    .bind(media_item_id)
    .fetch_all(pool)
    .await
    .context("failed to list media item credits")?;

    Ok(rows.into_iter().map(map_media_item_credit).collect())
}

pub(crate) async fn list_preferred_local_cast_members(
    pool: &PgPool,
    media_item_id: i64,
) -> Result<Vec<MediaItemCredit>> {
    let rows = sqlx::query(
        r#"
        with selected_source as (
            select credit.retrieved_via, credit.local_metadata_source_id
            from media_item_credits credit
            left join media_local_metadata_sources local_source
              on local_source.id = credit.local_metadata_source_id
             and local_source.media_item_id = credit.media_item_id
             and local_source.is_selected
            where credit.media_item_id = $1
              and credit.credit_type = 'actor'
              and (
                  credit.retrieved_via = 'manual'
                  or (credit.retrieved_via = 'nfo' and local_source.id is not null)
              )
            order by case credit.retrieved_via when 'manual' then 0 else 1 end
            limit 1
        )
        select
            credit.id,
            credit.media_item_id,
            credit.credit_type,
            credit.retrieved_via,
            credit.sort_order,
            credit.provider_person_id,
            credit.name,
            credit.role,
            credit.profile_path
        from media_item_credits credit
        join selected_source source
          on source.retrieved_via = credit.retrieved_via
         and source.local_metadata_source_id
             is not distinct from credit.local_metadata_source_id
        where credit.media_item_id = $1
          and credit.credit_type = 'actor'
        order by credit.sort_order asc, credit.name asc, credit.id asc
        "#,
    )
    .bind(media_item_id)
    .fetch_all(pool)
    .await
    .context("failed to list preferred local cast members")?;

    Ok(rows.into_iter().map(map_media_item_credit).collect())
}

fn validate_replace_params(params: &ReplaceMediaLocalMetadataSourceParams) -> Result<()> {
    if params.source_path.trim().is_empty() {
        bail!("local metadata source path must not be blank");
    }
    if !params.payload.is_object() {
        bail!("local metadata payload must be a JSON object");
    }
    if !matches!(
        params.document_type.as_str(),
        "movie" | "tvshow" | "episodedetails"
    ) {
        bail!("unsupported local metadata document type");
    }
    for credit in &params.credits {
        if !matches!(credit.credit_type.as_str(), "actor" | "director" | "writer") {
            bail!("unsupported media credit type");
        }
        if credit.sort_order < 0 || credit.name.trim().is_empty() {
            bail!("invalid local media credit");
        }
    }
    Ok(())
}

async fn replace_nfo_projection_tx(
    tx: &mut Transaction<'_, Postgres>,
    local_metadata_source_id: i64,
    media_item_id: i64,
    external_ids: &[MediaExternalId],
    ratings: &[MediaRating],
    credits: &[ReplaceMediaItemCreditParams],
) -> Result<()> {
    clear_nfo_projection_tx(tx, media_item_id).await?;

    for external_id in external_ids {
        sqlx::query(
            r#"
            insert into media_item_external_ids (
                media_item_id,
                provider,
                external_id,
                retrieved_via,
                local_metadata_source_id
            )
            values ($1, $2, $3, 'nfo', $4)
            on conflict (media_item_id, provider, retrieved_via) do update
            set external_id = excluded.external_id,
                local_metadata_source_id = excluded.local_metadata_source_id,
                updated_at = now()
            "#,
        )
        .bind(media_item_id)
        .bind(&external_id.provider)
        .bind(&external_id.external_id)
        .bind(local_metadata_source_id)
        .execute(&mut **tx)
        .await
        .context("failed to upsert NFO external id")?;
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
                local_metadata_source_id,
                attributes,
                fetched_at
            )
            values ($1, $2, $3, $4::double precision::numeric,
                    $5::double precision::numeric, $6, 'nfo', $7, $8, $9)
            on conflict (media_item_id, source, kind, retrieved_via) do update
            set score = excluded.score,
                scale = excluded.scale,
                rating_count = excluded.rating_count,
                local_metadata_source_id = excluded.local_metadata_source_id,
                attributes = excluded.attributes,
                fetched_at = excluded.fetched_at,
                updated_at = now()
            "#,
        )
        .bind(media_item_id)
        .bind(&rating.source)
        .bind(&rating.kind)
        .bind(rating.score)
        .bind(rating.scale)
        .bind(rating.rating_count)
        .bind(local_metadata_source_id)
        .bind(&rating.attributes)
        .bind(rating.fetched_at)
        .execute(&mut **tx)
        .await
        .context("failed to upsert NFO rating")?;
    }

    for credit_batch in credits.chunks(NFO_CREDIT_INSERT_BATCH_SIZE) {
        let mut query = QueryBuilder::<Postgres>::new(
            r#"
            insert into media_item_credits (
                media_item_id,
                local_metadata_source_id,
                credit_type,
                retrieved_via,
                sort_order,
                person_provider,
                provider_person_id,
                name,
                role,
                profile_path
            )
            "#,
        );
        query.push_values(credit_batch, |mut row, credit| {
            row.push_bind(media_item_id)
                .push_bind(local_metadata_source_id)
                .push_bind(&credit.credit_type)
                .push("'nfo'")
                .push_bind(credit.sort_order)
                .push_bind(&credit.person_provider)
                .push_bind(&credit.person_id)
                .push_bind(&credit.name)
                .push_bind(&credit.role)
                .push_bind(&credit.profile_path);
        });
        query
            .build()
            .execute(&mut **tx)
            .await
            .context("failed to insert NFO media credit batch")?;
    }

    Ok(())
}

async fn clear_nfo_projection_tx(
    tx: &mut Transaction<'_, Postgres>,
    media_item_id: i64,
) -> Result<()> {
    sqlx::query(
        "delete from media_item_external_ids where media_item_id = $1 and retrieved_via = 'nfo'",
    )
    .bind(media_item_id)
    .execute(&mut **tx)
    .await
    .context("failed to clear NFO external ids")?;
    sqlx::query(
        "delete from media_item_ratings where media_item_id = $1 and retrieved_via = 'nfo'",
    )
    .bind(media_item_id)
    .execute(&mut **tx)
    .await
    .context("failed to clear NFO ratings")?;
    sqlx::query(
        "delete from media_item_credits where media_item_id = $1 and retrieved_via = 'nfo'",
    )
    .bind(media_item_id)
    .execute(&mut **tx)
    .await
    .context("failed to clear NFO media credits")?;
    Ok(())
}

fn map_local_metadata_source(row: sqlx::postgres::PgRow) -> MediaLocalMetadataSource {
    MediaLocalMetadataSource {
        id: row.get("id"),
        library_id: row.get("library_id"),
        media_item_id: row.get("media_item_id"),
        source_path: row.get("source_path"),
        document_type: row.get("document_type"),
        schema_version: row.get("schema_version"),
        is_locked: row.get("is_locked"),
        is_selected: row.get("is_selected"),
        payload: row.get("payload"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn map_local_metadata_source_summary(
    row: sqlx::postgres::PgRow,
) -> MediaLocalMetadataSourceSummary {
    MediaLocalMetadataSourceSummary {
        id: row.get("id"),
        library_id: row.get("library_id"),
        media_item_id: row.get("media_item_id"),
        source_path: row.get("source_path"),
        document_type: row.get("document_type"),
        schema_version: row.get("schema_version"),
        is_locked: row.get("is_locked"),
        is_selected: row.get("is_selected"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn map_media_item_credit(row: sqlx::postgres::PgRow) -> MediaItemCredit {
    MediaItemCredit {
        id: row.get("id"),
        media_item_id: row.get("media_item_id"),
        credit_type: row.get("credit_type"),
        retrieved_via: row.get("retrieved_via"),
        sort_order: row.get("sort_order"),
        person_id: row.get("provider_person_id"),
        name: row.get("name"),
        role: row.get("role"),
        profile_path: row.get("profile_path"),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        binding_preference, get_media_local_metadata_source_for_item, list_media_item_external_ids,
        list_media_local_metadata_source_summaries_for_item,
        list_media_local_metadata_sources_for_item, remove_media_local_metadata_source,
        replace_media_local_metadata_source, replace_media_local_metadata_source_tx,
        source_selection_key, BindingPreference, MediaLocalMetadataTarget,
        ReplaceMediaItemCreditParams, ReplaceMediaLocalMetadataSourceParams,
        NFO_CREDIT_INSERT_BATCH_SIZE,
    };
    use crate::media_items::{
        capture_local_metadata_projection_checkpoint_tx,
        restore_authoritative_local_metadata_projection_tx, CreateLocalMetadataSnapshotParams,
        LocalMetadataProjectionScope,
    };
    use mova_domain::{MediaExternalId, MediaRating};
    use serde_json::json;
    use time::OffsetDateTime;

    #[test]
    fn source_selection_is_stable_and_prefers_matching_identity_then_file_specific_nfo() {
        assert_eq!(
            binding_preference(Some("42"), Some("42")),
            BindingPreference::Matches
        );
        assert_eq!(
            binding_preference(None, Some("42")),
            BindingPreference::Unspecified
        );
        assert_eq!(
            binding_preference(Some("84"), Some("42")),
            BindingPreference::Conflicts
        );
        assert!(
            source_selection_key(
                "/media/Movie/Movie.2160p.nfo",
                "movie",
                BindingPreference::Unspecified,
            ) < source_selection_key(
                "/media/Movie/movie.nfo",
                "movie",
                BindingPreference::Unspecified,
            )
        );
        assert!(
            source_selection_key("/z/movie.nfo", "movie", BindingPreference::Matches,)
                < source_selection_key("/a/Movie.nfo", "movie", BindingPreference::Unspecified,)
        );
    }

    fn source_params(
        library_id: i64,
        media_item_id: i64,
        path: &str,
        selected: bool,
        tmdb_id: &str,
        actor: &str,
    ) -> ReplaceMediaLocalMetadataSourceParams {
        ReplaceMediaLocalMetadataSourceParams {
            library_id,
            target: MediaLocalMetadataTarget::MediaItem(media_item_id),
            source_path: path.to_string(),
            document_type: "movie".to_string(),
            is_locked: true,
            is_selected: selected,
            payload: json!({"title": "Local Movie", "unique_ids": {"tmdb": tmdb_id}}),
            external_ids: vec![MediaExternalId {
                provider: "tmdb".to_string(),
                external_id: tmdb_id.to_string(),
            }],
            ratings: vec![MediaRating {
                source: "tmdb".to_string(),
                kind: "audience".to_string(),
                score: 8.5,
                scale: 10.0,
                rating_count: Some(10),
                retrieved_via: "nfo".to_string(),
                attributes: json!({}),
                fetched_at: OffsetDateTime::now_utc(),
            }],
            credits: vec![ReplaceMediaItemCreditParams {
                credit_type: "actor".to_string(),
                sort_order: 0,
                person_provider: None,
                person_id: None,
                name: actor.to_string(),
                role: Some("Lead".to_string()),
                profile_path: None,
            }],
        }
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn large_nfo_credit_sets_are_batched_without_losing_duplicate_coordinates(
        pool: sqlx::PgPool,
    ) {
        let library_id = sqlx::query_scalar::<_, i64>(
            "insert into libraries (name, root_path) values ('Large NFO', '/media/large-nfo') returning id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let media_item_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into media_items (library_id, media_type, title, source_title)
            values ($1, 'movie', 'Large Credits', 'Large Credits')
            returning id
            "#,
        )
        .bind(library_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        let expected = NFO_CREDIT_INSERT_BATCH_SIZE + 1;
        let credits = (0..expected)
            .map(|index| ReplaceMediaItemCreditParams {
                credit_type: "actor".to_string(),
                sort_order: 0,
                person_provider: None,
                person_id: None,
                name: "Shared Name".to_string(),
                role: Some(format!("Role {index}")),
                profile_path: None,
            })
            .collect();
        let params = ReplaceMediaLocalMetadataSourceParams {
            library_id,
            target: MediaLocalMetadataTarget::MediaItem(media_item_id),
            source_path: "/media/large-nfo/movie.nfo".to_string(),
            document_type: "movie".to_string(),
            is_locked: false,
            is_selected: true,
            payload: json!({"title": "Large Credits"}),
            external_ids: Vec::new(),
            ratings: Vec::new(),
            credits,
        };

        let source = replace_media_local_metadata_source(&pool, params)
            .await
            .unwrap();
        let persisted = sqlx::query_as::<_, (i64, i64)>(
            r#"
            select count(*), count(distinct local_metadata_source_id)
            from media_item_credits
            where media_item_id = $1
              and local_metadata_source_id = $2
            "#,
        )
        .bind(media_item_id)
        .bind(source.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(persisted, (expected as i64, 1));
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn unchanged_shared_source_reuses_row_and_structured_projection(pool: sqlx::PgPool) {
        let library_id = sqlx::query_scalar::<_, i64>(
            "insert into libraries (name, root_path) values ('Shared NFO', '/media/shared-nfo') returning id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let media_item_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into media_items (library_id, media_type, title, source_title)
            values ($1, 'series', 'Shared Series', 'Shared Series')
            returning id
            "#,
        )
        .bind(library_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        let mut params = source_params(
            library_id,
            media_item_id,
            "/media/shared-nfo/tvshow.nfo",
            true,
            "42",
            "Shared actor",
        );
        params.document_type = "tvshow".to_string();

        let first = replace_media_local_metadata_source(&pool, params.clone())
            .await
            .unwrap();
        let first_credit_id = sqlx::query_scalar::<_, i64>(
            "select id from media_item_credits where local_metadata_source_id = $1",
        )
        .bind(first.id)
        .fetch_one(&pool)
        .await
        .unwrap();

        let second = replace_media_local_metadata_source(&pool, params)
            .await
            .unwrap();
        let projection = sqlx::query_as::<_, (i64, i64)>(
            r#"
            select count(*), min(id)
            from media_item_credits
            where local_metadata_source_id = $1
            "#,
        )
        .bind(first.id)
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(second.id, first.id);
        assert_eq!(projection, (1, first_credit_id));
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn versioned_sources_keep_one_selected_projection_and_remote_identity(
        pool: sqlx::PgPool,
    ) {
        let library_id = sqlx::query_scalar::<_, i64>(
            "insert into libraries (name, root_path) values ('NFO', '/media/nfo') returning id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let media_item_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into media_items (library_id, media_type, title, source_title)
            values ($1, 'movie', 'Movie', 'Movie')
            returning id
            "#,
        )
        .bind(library_id)
        .fetch_one(&pool)
        .await
        .unwrap();

        replace_media_local_metadata_source(
            &pool,
            source_params(
                library_id,
                media_item_id,
                "/media/nfo/version-a.nfo",
                true,
                "42",
                "Actor A",
            ),
        )
        .await
        .unwrap();
        replace_media_local_metadata_source(
            &pool,
            source_params(
                library_id,
                media_item_id,
                "/media/nfo/version-b.nfo",
                false,
                "84",
                "Actor B",
            ),
        )
        .await
        .unwrap();

        let mut tx = pool.begin().await.unwrap();
        crate::media_items::replace_media_item_remote_data(
            &mut tx,
            media_item_id,
            Some("tmdb"),
            &[MediaExternalId {
                provider: "tmdb".to_string(),
                external_id: "remote-42".to_string(),
            }],
            &[],
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();

        let identities = list_media_item_external_ids(&pool, media_item_id)
            .await
            .unwrap();
        assert!(identities
            .iter()
            .any(|id| id.retrieved_via == "nfo" && id.external_id == "42"));
        assert!(identities
            .iter()
            .any(|id| id.retrieved_via == "tmdb" && id.external_id == "remote-42"));

        sqlx::query(
            r#"
            update media_items
            set metadata_provider = 'tmdb',
                metadata_provider_item_id = 'remote-42',
                metadata_status = 'matched'
            where id = $1
            "#,
        )
        .bind(media_item_id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            update media_items
            set metadata_provider_item_id = '84'
            where id = $1
            "#,
        )
        .bind(media_item_id)
        .execute(&pool)
        .await
        .unwrap();

        let identities = list_media_item_external_ids(&pool, media_item_id)
            .await
            .unwrap();
        assert!(identities
            .iter()
            .any(|id| id.retrieved_via == "nfo" && id.external_id == "42"));
        assert!(!identities
            .iter()
            .any(|id| id.retrieved_via == "tmdb" && id.external_id == "remote-42"));
        let nfo_rating_count = sqlx::query_scalar::<_, i64>(
            "select count(*) from media_item_ratings where media_item_id = $1 and retrieved_via = 'nfo'",
        )
        .bind(media_item_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(nfo_rating_count, 1);

        replace_media_local_metadata_source(
            &pool,
            source_params(
                library_id,
                media_item_id,
                "/media/nfo/version-b.nfo",
                true,
                "84",
                "Actor B",
            ),
        )
        .await
        .unwrap();

        let sources = list_media_local_metadata_sources_for_item(&pool, media_item_id)
            .await
            .unwrap();
        assert_eq!(sources.len(), 2);
        assert_eq!(
            sources.iter().filter(|source| source.is_selected).count(),
            1
        );
        assert_eq!(
            sources
                .iter()
                .find(|source| source.is_selected)
                .map(|source| source.source_path.as_str()),
            Some("/media/nfo/version-b.nfo")
        );
        let summaries = list_media_local_metadata_source_summaries_for_item(&pool, media_item_id)
            .await
            .unwrap();
        assert_eq!(summaries.len(), 2);
        assert!(summaries[0].is_selected);
        let selected_source_id = summaries[0].id;
        let selected_source =
            get_media_local_metadata_source_for_item(&pool, media_item_id, selected_source_id)
                .await
                .unwrap()
                .unwrap();
        assert_eq!(selected_source.payload["title"], "Local Movie");
        assert!(get_media_local_metadata_source_for_item(
            &pool,
            media_item_id + 1,
            selected_source_id,
        )
        .await
        .unwrap()
        .is_none());
        let identities = list_media_item_external_ids(&pool, media_item_id)
            .await
            .unwrap();
        assert!(identities
            .iter()
            .any(|id| id.retrieved_via == "nfo" && id.external_id == "84"));
        assert!(!identities
            .iter()
            .any(|id| id.retrieved_via == "nfo" && id.external_id == "42"));

        let owned_projection_count = sqlx::query_scalar::<_, i64>(
            r#"
            select count(*)
            from (
                select local_metadata_source_id
                from media_item_external_ids
                where media_item_id = $1 and retrieved_via = 'nfo'
                union all
                select local_metadata_source_id
                from media_item_ratings
                where media_item_id = $1 and retrieved_via = 'nfo'
                union all
                select local_metadata_source_id
                from media_item_credits
                where media_item_id = $1 and retrieved_via = 'nfo'
            ) projection
            where projection.local_metadata_source_id = $2
            "#,
        )
        .bind(media_item_id)
        .bind(selected_source_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(owned_projection_count, 3);

        assert!(
            remove_media_local_metadata_source(&pool, library_id, "/media/nfo/version-b.nfo",)
                .await
                .unwrap()
        );
        let remaining_nfo_projection_count = sqlx::query_scalar::<_, i64>(
            r#"
            select
                (select count(*) from media_item_external_ids
                 where media_item_id = $1 and retrieved_via = 'nfo')
              + (select count(*) from media_item_ratings
                 where media_item_id = $1 and retrieved_via = 'nfo')
              + (select count(*) from media_item_credits
                 where media_item_id = $1 and retrieved_via = 'nfo')
            "#,
        )
        .bind(media_item_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(remaining_nfo_projection_count, 0);
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn merged_groups_choose_the_same_global_source_in_either_order(pool: sqlx::PgPool) {
        async fn create_movie(pool: &sqlx::PgPool, library_id: i64, title: &str) -> i64 {
            sqlx::query_scalar::<_, i64>(
                r#"
                insert into media_items (library_id, media_type, title, source_title)
                values ($1, 'movie', $2, $2)
                returning id
                "#,
            )
            .bind(library_id)
            .bind(title)
            .fetch_one(pool)
            .await
            .unwrap()
        }

        async fn selected_path(pool: &sqlx::PgPool, media_item_id: i64) -> String {
            sqlx::query_scalar::<_, String>(
                "select source_path from media_local_metadata_sources where media_item_id = $1 and is_selected",
            )
            .bind(media_item_id)
            .fetch_one(pool)
            .await
            .unwrap()
        }

        for reverse in [false, true] {
            let suffix = if reverse { "reverse" } else { "forward" };
            let library_id = sqlx::query_scalar::<_, i64>(
                "insert into libraries (name, root_path) values ($1, $2) returning id",
            )
            .bind(format!("NFO {suffix}"))
            .bind(format!("/media/{suffix}"))
            .fetch_one(&pool)
            .await
            .unwrap();
            let target_id = create_movie(&pool, library_id, "Target").await;
            let source_id = create_movie(&pool, library_id, "Source").await;
            let generic_path = format!("/media/{suffix}/Movie/movie.nfo");
            let specific_path = format!("/media/{suffix}/Movie/Movie.2160p.nfo");

            let (target_path, target_actor, source_path, source_actor) = if reverse {
                (
                    specific_path.as_str(),
                    "Specific actor",
                    generic_path.as_str(),
                    "Generic actor",
                )
            } else {
                (
                    generic_path.as_str(),
                    "Generic actor",
                    specific_path.as_str(),
                    "Specific actor",
                )
            };
            replace_media_local_metadata_source(
                &pool,
                source_params(library_id, target_id, target_path, true, "42", target_actor),
            )
            .await
            .unwrap();
            replace_media_local_metadata_source(
                &pool,
                source_params(library_id, source_id, source_path, true, "42", source_actor),
            )
            .await
            .unwrap();

            replace_media_local_metadata_source(
                &pool,
                source_params(library_id, target_id, source_path, true, "42", source_actor),
            )
            .await
            .unwrap();

            assert_eq!(selected_path(&pool, target_id).await, specific_path);
            let selected_actor = sqlx::query_scalar::<_, String>(
                r#"
                select name
                from media_item_credits
                where media_item_id = $1
                  and retrieved_via = 'nfo'
                  and credit_type = 'actor'
                "#,
            )
            .bind(target_id)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(selected_actor, "Specific actor");
        }
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn merged_groups_keep_public_projection_aligned_with_global_source_in_either_order(
        pool: sqlx::PgPool,
    ) {
        for reverse in [false, true] {
            let suffix = if reverse { "reverse" } else { "forward" };
            let library_id = sqlx::query_scalar::<_, i64>(
                "insert into libraries (name, root_path) values ($1, $2) returning id",
            )
            .bind(format!("Projection {suffix}"))
            .bind(format!("/media/projection-{suffix}"))
            .fetch_one(&pool)
            .await
            .unwrap();
            let media_item_id = sqlx::query_scalar::<_, i64>(
                r#"
                insert into media_items (
                    library_id, media_type, title, source_title, overview, poster_path,
                    metadata_provider, metadata_provider_item_id, metadata_status
                )
                values ($1, 'movie', $2, $2, $3, $4, 'tmdb', '42', 'matched')
                returning id
                "#,
            )
            .bind(library_id)
            .bind(if reverse {
                "Generic title"
            } else {
                "Specific title"
            })
            .bind(if reverse {
                "Generic overview"
            } else {
                "Specific overview"
            })
            .bind(if reverse {
                "/cache/generic.jpg"
            } else {
                "/cache/specific.jpg"
            })
            .fetch_one(&pool)
            .await
            .unwrap();
            let generic_path = format!("/media/projection-{suffix}/Movie/movie.nfo");
            let specific_path = format!("/media/projection-{suffix}/Movie/Movie.2160p.nfo");
            let mut generic = source_params(
                library_id,
                media_item_id,
                &generic_path,
                true,
                "42",
                "Generic actor",
            );
            generic.payload = json!({
                "schema_version": 1,
                "metadata": {
                    "title": "Generic title",
                    "overview": "Generic overview",
                    "artwork": { "posters": ["/cache/generic.jpg"] }
                },
                "public_projection": {
                    "title": "Generic title",
                    "source_title": "Generic title",
                    "overview": "Generic overview",
                    "poster_path": "/cache/generic.jpg"
                }
            });
            let mut specific = source_params(
                library_id,
                media_item_id,
                &specific_path,
                true,
                "42",
                "Specific actor",
            );
            specific.payload = json!({
                "schema_version": 1,
                "metadata": {
                    "title": "Specific title",
                    "overview": "Specific overview",
                    "artwork": { "posters": ["/cache/specific.jpg"] }
                },
                "public_projection": {
                    "title": "Specific title",
                    "source_title": "Specific title",
                    "overview": "Specific overview",
                    "poster_path": "/cache/specific.jpg"
                }
            });
            let (initial, incoming) = if reverse {
                (generic, specific)
            } else {
                (specific, generic)
            };
            replace_media_local_metadata_source(&pool, initial)
                .await
                .unwrap();

            let snapshot = CreateLocalMetadataSnapshotParams {
                source_path: incoming.source_path.clone(),
                document_type: incoming.document_type.clone(),
                is_locked: incoming.is_locked,
                is_selected: incoming.is_selected,
                payload: incoming.payload.clone(),
                external_ids: incoming.external_ids.clone(),
                ratings: incoming.ratings.clone(),
                credits: incoming.credits.clone(),
            };
            let incoming_title = if reverse {
                "Specific title"
            } else {
                "Generic title"
            };
            let incoming_overview = if reverse {
                "Specific overview"
            } else {
                "Generic overview"
            };
            let incoming_poster = if reverse {
                "/cache/specific.jpg"
            } else {
                "/cache/generic.jpg"
            };
            let mut tx = pool.begin().await.unwrap();
            let checkpoint =
                capture_local_metadata_projection_checkpoint_tx(&mut tx, media_item_id)
                    .await
                    .unwrap();
            sqlx::query(
                "update media_items set title = $2, source_title = $2, overview = $3, poster_path = $4 where id = $1",
            )
            .bind(media_item_id)
            .bind(incoming_title)
            .bind(incoming_overview)
            .bind(incoming_poster)
            .execute(&mut *tx)
            .await
            .unwrap();
            replace_media_local_metadata_source_tx(&mut tx, incoming)
                .await
                .unwrap();
            restore_authoritative_local_metadata_projection_tx(
                &mut tx,
                media_item_id,
                &checkpoint,
                Some(&snapshot),
                LocalMetadataProjectionScope::Movie,
            )
            .await
            .unwrap();
            tx.commit().await.unwrap();

            let selected_path = sqlx::query_scalar::<_, String>(
                "select source_path from media_local_metadata_sources where media_item_id = $1 and is_selected",
            )
            .bind(media_item_id)
            .fetch_one(&pool)
            .await
            .unwrap();
            let public = sqlx::query_as::<_, (String, Option<String>, Option<String>)>(
                "select title, overview, poster_path from media_items where id = $1",
            )
            .bind(media_item_id)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(selected_path, specific_path);
            assert_eq!(public.0, "Specific title");
            assert_eq!(public.1.as_deref(), Some("Specific overview"));
            assert_eq!(public.2.as_deref(), Some("/cache/specific.jpg"));
        }
    }
}
