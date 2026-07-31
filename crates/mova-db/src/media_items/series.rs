use super::{
    ratings::replace_media_item_remote_data,
    sync::{
        cleanup_media_item_if_no_files, display_title_for_entry,
        existing_media_item_has_accepted_remote_binding, existing_media_item_is_matched,
        insert_media_file, local_artwork_path, patch_media_item_remote_fields,
        promote_cached_artwork_paths, reassign_media_file_parent_only,
        reassign_media_file_to_media_item, record_transient_metadata_refresh_failure,
        update_media_file_from_entry, ExistingLibraryMediaFileRecord,
    },
    CreateMediaEntryParams,
};
use anyhow::{Context, Result};
use mova_domain::METADATA_STATUS_MATCHED;
use sqlx::{Postgres, Row, Transaction};

use crate::{
    playback_progress::merge_media_item_user_state,
    tmdb_revalidation::record_authoritative_tmdb_snapshot_tx,
};

pub(super) async fn upsert_episode_media_entry(
    tx: &mut Transaction<'_, Postgres>,
    entry: &CreateMediaEntryParams,
    existing: Option<ExistingLibraryMediaFileRecord>,
    preserve_existing_parent: bool,
) -> Result<()> {
    let season_number = entry
        .season_number
        .context("episode entry missing season number")?;
    let episode_number = entry
        .episode_number
        .context("episode entry missing episode number")?;
    let (series_id, preserve_series_parent) =
        upsert_series_item_from_entry(tx, entry, preserve_existing_parent).await?;
    let season_id =
        upsert_season(tx, series_id, season_number, entry, preserve_series_parent).await?;
    let existing_episode_media_item_id =
        find_existing_episode_media_item(tx, season_id, episode_number).await?;

    if let Some(existing) = existing {
        if !existing.media_type.eq_ignore_ascii_case("episode") {
            let target_media_item_id =
                if let Some(existing_episode_media_item_id) = existing_episode_media_item_id {
                    update_existing_episode_media_item(
                        tx,
                        existing_episode_media_item_id,
                        entry,
                        preserve_series_parent,
                    )
                    .await?;
                    existing_episode_media_item_id
                } else {
                    insert_episode_structure(tx, entry, season_id, episode_number).await?
                };
            reassign_media_file_to_media_item(
                tx,
                existing.media_file_id,
                target_media_item_id,
                entry,
            )
            .await?;
            merge_media_item_user_state(tx, existing.media_item_id, target_media_item_id).await?;
            cleanup_media_item_if_no_files(tx, existing.media_item_id).await?;
            if existing.media_type.eq_ignore_ascii_case("series") {
                cleanup_orphan_series_structure(tx, entry.library_id).await?;
            }
            return Ok(());
        }

        let previous_series_id = find_series_id_for_episode(tx, existing.media_item_id).await?;

        if let Some(existing_episode_media_item_id) = existing_episode_media_item_id {
            if existing_episode_media_item_id != existing.media_item_id {
                update_existing_episode_media_item(
                    tx,
                    existing_episode_media_item_id,
                    entry,
                    preserve_series_parent,
                )
                .await?;
                reassign_media_file_to_media_item(
                    tx,
                    existing.media_file_id,
                    existing_episode_media_item_id,
                    entry,
                )
                .await?;
                merge_media_item_user_state(
                    tx,
                    existing.media_item_id,
                    existing_episode_media_item_id,
                )
                .await?;
                if let Some(previous_series_id) = previous_series_id {
                    merge_media_item_user_state(tx, previous_series_id, series_id).await?;
                }
                cleanup_media_item_if_no_files(tx, existing.media_item_id).await?;
                return Ok(());
            }
        }

        update_existing_episode_media_item(
            tx,
            existing.media_item_id,
            entry,
            preserve_series_parent,
        )
        .await?;
        update_episode_record(
            tx,
            existing.media_item_id,
            entry.library_id,
            season_id,
            episode_number,
        )
        .await?;
        if let Some(previous_series_id) = previous_series_id {
            merge_media_item_user_state(tx, previous_series_id, series_id).await?;
        }
        update_media_file_from_entry(tx, existing.media_file_id, entry).await?;
        return Ok(());
    }

    if let Some(existing_episode_media_item_id) = existing_episode_media_item_id {
        update_existing_episode_media_item(
            tx,
            existing_episode_media_item_id,
            entry,
            preserve_series_parent,
        )
        .await?;
        insert_media_file(tx, existing_episode_media_item_id, entry).await?;
        return Ok(());
    }

    insert_episode_media_tree(tx, entry, season_id, episode_number).await?;
    Ok(())
}

pub(super) async fn patch_episode_remote_entry(
    tx: &mut Transaction<'_, Postgres>,
    entry: &CreateMediaEntryParams,
    existing: ExistingLibraryMediaFileRecord,
    preserve_existing_parent: bool,
) -> Result<()> {
    if !existing.media_type.eq_ignore_ascii_case("episode") {
        anyhow::bail!(
            "remote metadata cannot change locally committed media type {} for {}",
            existing.media_type,
            entry.file_path
        );
    }
    let season_number = entry
        .season_number
        .context("remote episode patch missing season number")?;
    let episode_number = entry
        .episode_number
        .context("remote episode patch missing episode number")?;
    let (stored_series_id, stored_season_number, stored_episode_number) =
        existing_episode_coordinates(tx, existing.media_item_id)
            .await?
            .context("remote episode patch requires a locally committed episode")?;
    if stored_season_number != season_number || stored_episode_number != episode_number {
        anyhow::bail!(
            "remote metadata cannot change local episode coordinates S{stored_season_number:02}E{stored_episode_number:02} to S{season_number:02}E{episode_number:02}"
        );
    }

    // A remote match may add or replace the series year and title, so its
    // local identity lookup can legitimately miss. Keep provider-ID matches
    // authoritative for merging, then fall back to the persisted parent.
    let series_id = find_existing_series_item(tx, entry)
        .await?
        .unwrap_or(stored_series_id);
    let preserve_series_parent = preserve_existing_parent
        && existing_media_item_has_accepted_remote_binding(tx, series_id).await?;
    // `replace_remote_data` is deliberately true for only one entry in a
    // series group so shared series external IDs and ratings are replaced
    // once. A successful authoritative remote group must still patch every
    // episode's own title, overview, and artwork.
    let replace_episode_remote_fields = !preserve_existing_parent;
    patch_media_item_remote_fields(
        tx,
        series_id,
        entry,
        preserve_existing_parent,
        entry.series_poster_path.as_deref(),
        entry.series_backdrop_path.as_deref(),
        entry.series_logo_path.as_deref(),
    )
    .await?;
    let season_id = upsert_season(
        tx,
        series_id,
        season_number,
        entry,
        preserve_existing_parent,
    )
    .await?;
    let target_episode_id = find_existing_episode_media_item(tx, season_id, episode_number).await?;
    let previous_series_id = Some(stored_series_id);

    match target_episode_id {
        Some(target_episode_id) if target_episode_id != existing.media_item_id => {
            patch_episode_remote_fields(
                tx,
                target_episode_id,
                entry,
                preserve_series_parent,
                replace_episode_remote_fields,
            )
            .await?;
            reassign_media_file_parent_only(tx, existing.media_file_id, target_episode_id).await?;
            merge_media_item_user_state(tx, existing.media_item_id, target_episode_id).await?;
            cleanup_media_item_if_no_files(tx, existing.media_item_id).await?;
        }
        _ => {
            patch_episode_remote_fields(
                tx,
                existing.media_item_id,
                entry,
                preserve_series_parent,
                replace_episode_remote_fields,
            )
            .await?;
            update_episode_record(
                tx,
                existing.media_item_id,
                entry.library_id,
                season_id,
                episode_number,
            )
            .await?;
        }
    }

    if let Some(previous_series_id) = previous_series_id {
        if previous_series_id != series_id {
            merge_media_item_user_state(tx, previous_series_id, series_id).await?;
        }
    }

    Ok(())
}

async fn existing_episode_coordinates(
    tx: &mut Transaction<'_, Postgres>,
    media_item_id: i64,
) -> Result<Option<(i64, i32, i32)>> {
    sqlx::query_as(
        r#"
        select s.series_id, s.season_number, e.episode_number
        from episodes e
        join seasons s on s.id = e.season_id
        where e.media_item_id = $1
        "#,
    )
    .bind(media_item_id)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to read locally committed episode coordinates")
}

async fn patch_episode_remote_fields(
    tx: &mut Transaction<'_, Postgres>,
    media_item_id: i64,
    entry: &CreateMediaEntryParams,
    preserve_existing_parent: bool,
    replace_remote_fields: bool,
) -> Result<()> {
    if preserve_existing_parent && existing_media_item_is_matched(tx, media_item_id).await? {
        record_transient_metadata_refresh_failure(tx, media_item_id, entry).await?;
        return promote_cached_artwork_paths(
            tx,
            media_item_id,
            entry,
            entry.poster_path.as_deref(),
            entry.backdrop_path.as_deref(),
            entry.logo_path.as_deref(),
        )
        .await;
    }

    if !replace_remote_fields {
        sqlx::query(
            r#"
            update media_items
            set metadata_status = $2,
                metadata_failure_reason = $3,
                remote_media_type = coalesce($4, remote_media_type),
                updated_at = now()
            where id = $1
            "#,
        )
        .bind(media_item_id)
        .bind(&entry.metadata_status)
        .bind(&entry.metadata_failure_reason)
        .bind(&entry.remote_media_type)
        .execute(&mut **tx)
        .await
        .context("failed to patch remote episode review state")?;
        return Ok(());
    }

    let episode_number = entry
        .episode_number
        .context("remote episode patch missing episode number")?;
    sqlx::query(
        r#"
        update media_items
        set title = $2,
            metadata_status = $3,
            metadata_failure_reason = $4,
            remote_media_type = $5,
            overview = $6,
            poster_path = case when $10 then $7 else coalesce($7, poster_path) end,
            backdrop_path = case when $10 then $8 else coalesce($8, backdrop_path) end,
            logo_path = case when $10 then $9 else coalesce($9, logo_path) end,
            updated_at = now()
        where id = $1
        "#,
    )
    .bind(media_item_id)
    .bind(episode_title_for_entry(entry, episode_number))
    .bind(&entry.metadata_status)
    .bind(&entry.metadata_failure_reason)
    .bind(&entry.remote_media_type)
    .bind(&entry.overview)
    .bind(&entry.poster_path)
    .bind(&entry.backdrop_path)
    .bind(&entry.logo_path)
    .bind(entry.allow_artwork_clear)
    .execute(&mut **tx)
    .await
    .context("failed to patch remote-owned episode fields")?;

    Ok(())
}

async fn find_series_id_for_episode(
    tx: &mut Transaction<'_, Postgres>,
    media_item_id: i64,
) -> Result<Option<i64>> {
    sqlx::query_scalar(
        r#"
        select s.series_id
        from episodes e
        join seasons s on s.id = e.season_id
        where e.media_item_id = $1
        "#,
    )
    .bind(media_item_id)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to resolve the current series for an episode")
}

fn episode_title_for_entry(entry: &CreateMediaEntryParams, episode_number: i32) -> String {
    entry
        .episode_title
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("Episode {:02}", episode_number))
}

async fn insert_episode_media_tree(
    tx: &mut Transaction<'_, Postgres>,
    entry: &CreateMediaEntryParams,
    season_id: i64,
    episode_number: i32,
) -> Result<()> {
    let media_item_id = insert_episode_structure(tx, entry, season_id, episode_number).await?;
    insert_media_file(tx, media_item_id, entry).await?;
    Ok(())
}

async fn insert_episode_structure(
    tx: &mut Transaction<'_, Postgres>,
    entry: &CreateMediaEntryParams,
    season_id: i64,
    episode_number: i32,
) -> Result<i64> {
    let media_item_id = insert_episode_media_item(tx, entry, episode_number).await?;
    insert_episode_record(
        tx,
        media_item_id,
        entry.library_id,
        season_id,
        episode_number,
    )
    .await?;
    Ok(media_item_id)
}

async fn upsert_series_item_from_entry(
    tx: &mut Transaction<'_, Postgres>,
    entry: &CreateMediaEntryParams,
    preserve_existing_parent: bool,
) -> Result<(i64, bool)> {
    if let Some(series_id) = find_existing_series_item(tx, entry).await? {
        let preserve_series_parent = preserve_existing_parent
            && existing_media_item_has_accepted_remote_binding(tx, series_id).await?;
        if preserve_series_parent {
            record_transient_metadata_refresh_failure(tx, series_id, entry).await?;
            promote_cached_artwork_paths(
                tx,
                series_id,
                entry,
                entry.series_poster_path.as_deref(),
                entry.series_backdrop_path.as_deref(),
                entry.series_logo_path.as_deref(),
            )
            .await?;
        } else {
            update_series_item_from_entry(tx, series_id, entry).await?;
        }
        Ok((series_id, preserve_series_parent))
    } else {
        Ok((insert_series_item_from_entry(tx, entry).await?, false))
    }
}

async fn find_existing_series_item(
    tx: &mut Transaction<'_, Postgres>,
    entry: &CreateMediaEntryParams,
) -> Result<Option<i64>> {
    if let (Some(provider), Some(provider_item_id)) = (
        entry.metadata_provider.as_deref(),
        entry.metadata_provider_item_id.clone(),
    ) {
        let row = sqlx::query(
            r#"
            select id
            from media_items
            where library_id = $1
              and media_type = 'series'
              and metadata_provider = $2
              and metadata_provider_item_id = $3
            order by id asc
            limit 1
            "#,
        )
        .bind(entry.library_id)
        .bind(provider)
        .bind(provider_item_id)
        .fetch_optional(&mut **tx)
        .await
        .context("failed to find existing series item by remote metadata id")?;

        if let Some(row) = row {
            return Ok(Some(row.get("id")));
        }
    }

    let row = sqlx::query(
        r#"
        select id
        from media_items
        where library_id = $1
          and media_type = 'series'
          and source_title = $2
          and (
                ($3::int is null and year is null)
                or year = $3
              )
        limit 1
        "#,
    )
    .bind(entry.library_id)
    .bind(&entry.source_title)
    .bind(entry.year)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to find existing series item")?;

    Ok(row.map(|row| row.get("id")))
}

async fn insert_series_item_from_entry(
    tx: &mut Transaction<'_, Postgres>,
    entry: &CreateMediaEntryParams,
) -> Result<i64> {
    let title = display_title_for_entry(entry);
    let poster_path = entry.series_poster_path.as_ref();
    let backdrop_path = entry.series_backdrop_path.as_ref();
    let logo_path = entry.series_logo_path.as_ref();
    let row = sqlx::query(
        r#"
        insert into media_items (
            library_id,
            media_type,
            title,
            source_title,
            original_title,
            sort_title,
            metadata_provider,
            metadata_provider_item_id,
            metadata_status,
            metadata_failure_reason,
            remote_media_type,
            year,
            country,
            genres,
            studio,
            overview,
            poster_path,
            backdrop_path,
            logo_path
        )
        values (
            $1, 'series', $2, $3, $4, $5, $6, $7, $8, $9, $10, $11,
            $12, $13, $14, $15, $16, $17, $18
        )
        returning id
        "#,
    )
    .bind(entry.library_id)
    .bind(title)
    .bind(&entry.source_title)
    .bind(&entry.original_title)
    .bind(&entry.sort_title)
    .bind(&entry.metadata_provider)
    .bind(entry.metadata_provider_item_id.as_deref())
    .bind(&entry.metadata_status)
    .bind(&entry.metadata_failure_reason)
    .bind(&entry.remote_media_type)
    .bind(entry.year)
    .bind(&entry.country)
    .bind(&entry.genres)
    .bind(&entry.studio)
    .bind(&entry.overview)
    .bind(poster_path)
    .bind(backdrop_path)
    .bind(logo_path)
    .fetch_one(&mut **tx)
    .await
    .context("failed to insert series item")?;

    let series_id = row.get("id");
    if entry.replace_remote_data {
        replace_media_item_remote_data(
            tx,
            series_id,
            entry.metadata_provider.as_deref(),
            &entry.external_ids,
            &entry.ratings,
        )
        .await?;
        record_authoritative_tmdb_snapshot_tx(
            tx,
            series_id,
            entry.metadata_provider.as_deref(),
            entry.tmdb_remote_snapshot_json.as_deref(),
            entry.tmdb_remote_snapshot_renews_retention,
        )
        .await?;
    }

    Ok(series_id)
}

async fn update_series_item_from_entry(
    tx: &mut Transaction<'_, Postgres>,
    series_id: i64,
    entry: &CreateMediaEntryParams,
) -> Result<()> {
    let title = display_title_for_entry(entry);
    let poster_path = entry.series_poster_path.as_ref();
    let backdrop_path = entry.series_backdrop_path.as_ref();
    let logo_path = entry.series_logo_path.as_ref();
    let allow_artwork_clear = allows_artwork_clear(entry);

    sqlx::query(
        r#"
        update media_items
        set
            title = $2,
            source_title = $3,
            original_title = $4,
            sort_title = $5,
            metadata_provider = $6,
            metadata_provider_item_id = $7,
            metadata_status = $8,
            metadata_failure_reason = $9,
            remote_media_type = $10,
            year = $11,
            country = $12,
            genres = $13,
            studio = $14,
            overview = $15,
            poster_path = case
                when $19 then $16
                else coalesce($16, poster_path)
            end,
            backdrop_path = case
                when $19 then $17
                else coalesce($17, backdrop_path)
            end,
            logo_path = case
                when $19 then $18
                else coalesce($18, logo_path)
            end,
            updated_at = now()
        where id = $1
        "#,
    )
    .bind(series_id)
    .bind(title)
    .bind(&entry.source_title)
    .bind(&entry.original_title)
    .bind(&entry.sort_title)
    .bind(&entry.metadata_provider)
    .bind(entry.metadata_provider_item_id.as_deref())
    .bind(&entry.metadata_status)
    .bind(&entry.metadata_failure_reason)
    .bind(&entry.remote_media_type)
    .bind(entry.year)
    .bind(&entry.country)
    .bind(&entry.genres)
    .bind(&entry.studio)
    .bind(&entry.overview)
    .bind(poster_path)
    .bind(backdrop_path)
    .bind(logo_path)
    .bind(allow_artwork_clear)
    .execute(&mut **tx)
    .await
    .context("failed to update series item during library sync")?;

    if entry.replace_remote_data {
        replace_media_item_remote_data(
            tx,
            series_id,
            entry.metadata_provider.as_deref(),
            &entry.external_ids,
            &entry.ratings,
        )
        .await?;
        record_authoritative_tmdb_snapshot_tx(
            tx,
            series_id,
            entry.metadata_provider.as_deref(),
            entry.tmdb_remote_snapshot_json.as_deref(),
            entry.tmdb_remote_snapshot_renews_retention,
        )
        .await?;
    }

    Ok(())
}

async fn upsert_season(
    tx: &mut Transaction<'_, Postgres>,
    series_id: i64,
    season_number: i32,
    entry: &CreateMediaEntryParams,
    preserve_existing_parent: bool,
) -> Result<i64> {
    let title = entry
        .season_title
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| format!("Season {:02}", season_number));
    let poster_path = entry.season_poster_path.as_ref();
    let backdrop_path = entry.season_backdrop_path.as_ref();
    let allow_artwork_clear = allows_artwork_clear(entry);
    let row = sqlx::query(
        r#"
        insert into seasons (
            library_id,
            series_id,
            season_number,
            title,
            overview,
            poster_path,
            backdrop_path
        )
        values ($1, $2, $3, $4, $5, $6, $7)
        on conflict (series_id, season_number)
        do update set
            title = case
                when $9 then seasons.title
                else excluded.title
            end,
            overview = case
                when $9 then seasons.overview
                else coalesce(excluded.overview, seasons.overview)
            end,
            poster_path = case
                when $9 then seasons.poster_path
                when $8 then excluded.poster_path
                else coalesce(excluded.poster_path, seasons.poster_path)
            end,
            backdrop_path = case
                when $9 then seasons.backdrop_path
                when $8 then excluded.backdrop_path
                else coalesce(excluded.backdrop_path, seasons.backdrop_path)
            end,
            updated_at = now()
        returning id
        "#,
    )
    .bind(entry.library_id)
    .bind(series_id)
    .bind(season_number)
    .bind(title)
    .bind(&entry.season_overview)
    .bind(poster_path)
    .bind(backdrop_path)
    .bind(allow_artwork_clear)
    .bind(preserve_existing_parent)
    .fetch_one(&mut **tx)
    .await
    .context("failed to upsert season")?;

    let season_id = row.get("id");
    if preserve_existing_parent {
        promote_season_cached_artwork_paths(tx, season_id, entry).await?;
    }

    Ok(season_id)
}

async fn update_existing_episode_media_item(
    tx: &mut Transaction<'_, Postgres>,
    media_item_id: i64,
    entry: &CreateMediaEntryParams,
    preserve_series_parent: bool,
) -> Result<()> {
    if preserve_series_parent && existing_media_item_is_matched(tx, media_item_id).await? {
        record_transient_metadata_refresh_failure(tx, media_item_id, entry).await?;
        promote_cached_artwork_paths(
            tx,
            media_item_id,
            entry,
            entry.poster_path.as_deref(),
            entry.backdrop_path.as_deref(),
            entry.logo_path.as_deref(),
        )
        .await
    } else {
        update_episode_media_item_from_entry(tx, media_item_id, entry).await
    }
}

async fn promote_season_cached_artwork_paths(
    tx: &mut Transaction<'_, Postgres>,
    season_id: i64,
    entry: &CreateMediaEntryParams,
) -> Result<()> {
    if !entry
        .metadata_status
        .eq_ignore_ascii_case(METADATA_STATUS_MATCHED)
    {
        return Ok(());
    }

    let poster_path = local_artwork_path(entry.season_poster_path.as_deref());
    let backdrop_path = local_artwork_path(entry.season_backdrop_path.as_deref());
    if poster_path.is_none() && backdrop_path.is_none() {
        return Ok(());
    }

    sqlx::query(
        r#"
        update seasons
        set poster_path = case
                when $2::text is not null
                     and (
                         lower(coalesce(poster_path, '')) like 'http://%'
                         or lower(coalesce(poster_path, '')) like 'https://%'
                     )
                    then $2
                else poster_path
            end,
            backdrop_path = case
                when $3::text is not null
                     and (
                         lower(coalesce(backdrop_path, '')) like 'http://%'
                         or lower(coalesce(backdrop_path, '')) like 'https://%'
                     )
                    then $3
                else backdrop_path
            end,
            updated_at = now()
        where id = $1
          and (
                (
                    $2::text is not null
                    and (
                        lower(coalesce(poster_path, '')) like 'http://%'
                        or lower(coalesce(poster_path, '')) like 'https://%'
                    )
                )
             or (
                    $3::text is not null
                    and (
                        lower(coalesce(backdrop_path, '')) like 'http://%'
                        or lower(coalesce(backdrop_path, '')) like 'https://%'
                    )
                )
          )
        "#,
    )
    .bind(season_id)
    .bind(poster_path)
    .bind(backdrop_path)
    .execute(&mut **tx)
    .await
    .context("failed to promote cached season artwork")?;

    Ok(())
}

async fn insert_episode_media_item(
    tx: &mut Transaction<'_, Postgres>,
    entry: &CreateMediaEntryParams,
    episode_number: i32,
) -> Result<i64> {
    let row = sqlx::query(
        r#"
        insert into media_items (
            library_id,
            media_type,
            title,
            source_title,
            original_title,
            sort_title,
            metadata_status,
            metadata_failure_reason,
            remote_media_type,
            year,
            country,
            genres,
            studio,
            overview,
            poster_path,
            backdrop_path,
            logo_path
        )
        values (
            $1, 'episode', $2, $3, null, null, $4, $5, $6,
            null, null, null, null, $7, $8, $9, $10
        )
        returning id
        "#,
    )
    .bind(entry.library_id)
    .bind(episode_title_for_entry(entry, episode_number))
    .bind(
        entry
            .episode_title
            .as_ref()
            .cloned()
            .unwrap_or_else(|| entry.source_title.clone()),
    )
    .bind(&entry.metadata_status)
    .bind(&entry.metadata_failure_reason)
    .bind(&entry.remote_media_type)
    .bind(&entry.overview)
    .bind(&entry.poster_path)
    .bind(&entry.backdrop_path)
    .bind(&entry.logo_path)
    .fetch_one(&mut **tx)
    .await
    .context("failed to insert episode media item")?;

    Ok(row.get("id"))
}

async fn update_episode_media_item_from_entry(
    tx: &mut Transaction<'_, Postgres>,
    media_item_id: i64,
    entry: &CreateMediaEntryParams,
) -> Result<()> {
    let episode_number = entry
        .episode_number
        .context("episode entry missing episode number")?;
    let allow_artwork_clear = allows_artwork_clear(entry);

    sqlx::query(
        r#"
        update media_items
        set
            title = $2,
            source_title = $3,
            original_title = null,
            sort_title = null,
            metadata_status = $4,
            metadata_failure_reason = $5,
            remote_media_type = $6,
            year = null,
            country = null,
            genres = null,
            studio = null,
            overview = $7,
            poster_path = case
                when $11 then $8
                else coalesce($8, poster_path)
            end,
            backdrop_path = case
                when $11 then $9
                else coalesce($9, backdrop_path)
            end,
            logo_path = case
                when $11 then $10
                else coalesce($10, logo_path)
            end,
            updated_at = now()
        where id = $1
        "#,
    )
    .bind(media_item_id)
    .bind(episode_title_for_entry(entry, episode_number))
    .bind(
        entry
            .episode_title
            .as_ref()
            .cloned()
            .unwrap_or_else(|| entry.source_title.clone()),
    )
    .bind(&entry.metadata_status)
    .bind(&entry.metadata_failure_reason)
    .bind(&entry.remote_media_type)
    .bind(&entry.overview)
    .bind(&entry.poster_path)
    .bind(&entry.backdrop_path)
    .bind(&entry.logo_path)
    .bind(allow_artwork_clear)
    .execute(&mut **tx)
    .await
    .context("failed to update episode media item during library sync")?;

    Ok(())
}

fn allows_artwork_clear(entry: &CreateMediaEntryParams) -> bool {
    entry.allow_artwork_clear && metadata_status_allows_artwork_clear(&entry.metadata_status)
}

fn metadata_status_allows_artwork_clear(metadata_status: &str) -> bool {
    metadata_status.eq_ignore_ascii_case(METADATA_STATUS_MATCHED)
}

async fn insert_episode_record(
    tx: &mut Transaction<'_, Postgres>,
    media_item_id: i64,
    library_id: i64,
    season_id: i64,
    episode_number: i32,
) -> Result<()> {
    sqlx::query(
        r#"
        insert into episodes (media_item_id, library_id, season_id, episode_number)
        values ($1, $2, $3, $4)
        "#,
    )
    .bind(media_item_id)
    .bind(library_id)
    .bind(season_id)
    .bind(episode_number)
    .execute(&mut **tx)
    .await
    .context("failed to insert episode record")?;

    Ok(())
}

async fn find_existing_episode_media_item(
    tx: &mut Transaction<'_, Postgres>,
    season_id: i64,
    episode_number: i32,
) -> Result<Option<i64>> {
    let row = sqlx::query(
        r#"
        select media_item_id
        from episodes
        where season_id = $1
          and episode_number = $2
        limit 1
        "#,
    )
    .bind(season_id)
    .bind(episode_number)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to find existing episode record")?;

    Ok(row.map(|row| row.get("media_item_id")))
}

async fn update_episode_record(
    tx: &mut Transaction<'_, Postgres>,
    media_item_id: i64,
    library_id: i64,
    season_id: i64,
    episode_number: i32,
) -> Result<()> {
    let updated = sqlx::query(
        r#"
        update episodes
        set
            library_id = $2,
            season_id = $3,
            episode_number = $4
        where media_item_id = $1
        "#,
    )
    .bind(media_item_id)
    .bind(library_id)
    .bind(season_id)
    .bind(episode_number)
    .execute(&mut **tx)
    .await
    .context("failed to update episode record")?;

    if updated.rows_affected() == 0 {
        insert_episode_record(tx, media_item_id, library_id, season_id, episode_number).await?;
    }

    Ok(())
}

pub(super) async fn cleanup_orphan_series_structure(
    tx: &mut Transaction<'_, Postgres>,
    library_id: i64,
) -> Result<()> {
    sqlx::query(
        r#"
        delete from seasons s
        where s.series_id in (
            select id
            from media_items
            where library_id = $1
              and media_type = 'series'
        )
          and not exists (
            select 1
            from episodes e
            where e.season_id = s.id
          )
        "#,
    )
    .bind(library_id)
    .execute(&mut **tx)
    .await
    .context("failed to delete orphan seasons")?;

    sqlx::query(
        r#"
        delete from media_items mi
        where mi.library_id = $1
          and mi.media_type = 'series'
          and not exists (
            select 1
            from seasons s
            where s.series_id = mi.id
          )
        "#,
    )
    .bind(library_id)
    .execute(&mut **tx)
    .await
    .context("failed to delete orphan series items")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::metadata_status_allows_artwork_clear;

    #[test]
    fn only_matched_metadata_status_clears_artwork() {
        assert!(metadata_status_allows_artwork_clear("matched"));
        assert!(metadata_status_allows_artwork_clear("MATCHED"));
        assert!(!metadata_status_allows_artwork_clear("unmatched"));
        assert!(!metadata_status_allows_artwork_clear("skipped"));
        assert!(!metadata_status_allows_artwork_clear("failed"));
    }
}
