use super::{
    sync::{
        delete_media_item, insert_media_file, update_media_file_from_entry,
        ExistingLibraryMediaFileRecord,
    },
    CreateMediaEntryParams,
};
use anyhow::{Context, Result};
use sqlx::{Postgres, Row, Transaction};

pub(super) async fn upsert_episode_media_entry(
    tx: &mut Transaction<'_, Postgres>,
    entry: &CreateMediaEntryParams,
    existing: Option<ExistingLibraryMediaFileRecord>,
) -> Result<()> {
    let season_number = entry
        .season_number
        .context("episode entry missing season number")?;
    let episode_number = entry
        .episode_number
        .context("episode entry missing episode number")?;
    let series_id = upsert_series_item_from_entry(tx, entry).await?;
    let season_id = upsert_season(tx, series_id, season_number, entry).await?;

    if let Some(existing) = existing {
        if !existing.media_type.eq_ignore_ascii_case("episode") {
            delete_media_item(tx, existing.media_item_id).await?;
            insert_episode_media_tree(tx, entry, series_id, season_id, episode_number).await?;
            return Ok(());
        }

        update_episode_media_item_from_entry(tx, existing.media_item_id, entry).await?;
        update_episode_record(
            tx,
            existing.media_item_id,
            series_id,
            season_id,
            episode_number,
            episode_title_for_entry(entry, episode_number),
        )
        .await?;
        update_media_file_from_entry(tx, existing.media_file_id, entry).await?;
        return Ok(());
    }

    insert_episode_media_tree(tx, entry, series_id, season_id, episode_number).await?;
    Ok(())
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
    series_id: i64,
    season_id: i64,
    episode_number: i32,
) -> Result<()> {
    let media_item_id = insert_episode_media_item(tx, entry, episode_number).await?;
    insert_episode_record(
        tx,
        media_item_id,
        series_id,
        season_id,
        episode_number,
        episode_title_for_entry(entry, episode_number),
    )
    .await?;
    insert_media_file(tx, media_item_id, entry).await?;
    Ok(())
}

async fn upsert_series_item_from_entry(
    tx: &mut Transaction<'_, Postgres>,
    entry: &CreateMediaEntryParams,
) -> Result<i64> {
    if let Some(series_id) =
        find_existing_series_item(tx, entry.library_id, &entry.title, entry.year).await?
    {
        update_series_item_from_entry(tx, series_id, entry).await?;
        Ok(series_id)
    } else {
        insert_series_item_from_entry(tx, entry).await
    }
}

async fn find_existing_series_item(
    tx: &mut Transaction<'_, Postgres>,
    library_id: i64,
    title: &str,
    year: Option<i32>,
) -> Result<Option<i64>> {
    let row = sqlx::query(
        r#"
        select id
        from media_items
        where library_id = $1
          and media_type = 'series'
          and title = $2
          and (
                ($3::int is null and year is null)
                or year = $3
              )
        limit 1
        "#,
    )
    .bind(library_id)
    .bind(title)
    .bind(year)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to find existing series item")?;

    Ok(row.map(|row| row.get("id")))
}

async fn insert_series_item_from_entry(
    tx: &mut Transaction<'_, Postgres>,
    entry: &CreateMediaEntryParams,
) -> Result<i64> {
    let poster_path = entry
        .series_poster_path
        .as_ref()
        .or(entry.poster_path.as_ref());
    let backdrop_path = entry
        .series_backdrop_path
        .as_ref()
        .or(entry.backdrop_path.as_ref());
    let row = sqlx::query(
        r#"
        insert into media_items (
            library_id,
            media_type,
            title,
            original_title,
            sort_title,
            year,
            overview,
            poster_path,
            backdrop_path
        )
        values ($1, 'series', $2, $3, $4, $5, $6, $7, $8)
        returning id
        "#,
    )
    .bind(entry.library_id)
    .bind(&entry.title)
    .bind(&entry.original_title)
    .bind(&entry.sort_title)
    .bind(entry.year)
    .bind(&entry.overview)
    .bind(poster_path)
    .bind(backdrop_path)
    .fetch_one(&mut **tx)
    .await
    .context("failed to insert series item")?;

    Ok(row.get("id"))
}

async fn update_series_item_from_entry(
    tx: &mut Transaction<'_, Postgres>,
    series_id: i64,
    entry: &CreateMediaEntryParams,
) -> Result<()> {
    let poster_path = entry
        .series_poster_path
        .as_ref()
        .or(entry.poster_path.as_ref());
    let backdrop_path = entry
        .series_backdrop_path
        .as_ref()
        .or(entry.backdrop_path.as_ref());

    sqlx::query(
        r#"
        update media_items
        set
            title = $2,
            original_title = $3,
            sort_title = $4,
            year = $5,
            overview = $6,
            poster_path = $7,
            backdrop_path = $8,
            updated_at = now()
        where id = $1
        "#,
    )
    .bind(series_id)
    .bind(&entry.title)
    .bind(&entry.original_title)
    .bind(&entry.sort_title)
    .bind(entry.year)
    .bind(&entry.overview)
    .bind(poster_path)
    .bind(backdrop_path)
    .execute(&mut **tx)
    .await
    .context("failed to update series item during library sync")?;

    Ok(())
}

async fn upsert_season(
    tx: &mut Transaction<'_, Postgres>,
    series_id: i64,
    season_number: i32,
    entry: &CreateMediaEntryParams,
) -> Result<i64> {
    let title = entry
        .season_title
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| format!("Season {:02}", season_number));
    let poster_path = entry
        .season_poster_path
        .as_ref()
        .or(entry.poster_path.as_ref());
    let backdrop_path = entry
        .season_backdrop_path
        .as_ref()
        .or(entry.backdrop_path.as_ref());
    let row = sqlx::query(
        r#"
        insert into seasons (
            series_id,
            season_number,
            title,
            overview,
            poster_path,
            backdrop_path
        )
        values ($1, $2, $3, $4, $5, $6)
        on conflict (series_id, season_number)
        do update set
            title = excluded.title,
            overview = coalesce(excluded.overview, seasons.overview),
            poster_path = coalesce(excluded.poster_path, seasons.poster_path),
            backdrop_path = coalesce(excluded.backdrop_path, seasons.backdrop_path),
            updated_at = now()
        returning id
        "#,
    )
    .bind(series_id)
    .bind(season_number)
    .bind(title)
    .bind(&entry.season_overview)
    .bind(poster_path)
    .bind(backdrop_path)
    .fetch_one(&mut **tx)
    .await
    .context("failed to upsert season")?;

    Ok(row.get("id"))
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
            original_title,
            sort_title,
            year,
            overview,
            poster_path,
            backdrop_path
        )
        values ($1, 'episode', $2, null, null, null, $3, $4, $5)
        returning id
        "#,
    )
    .bind(entry.library_id)
    .bind(episode_title_for_entry(entry, episode_number))
    .bind(&entry.overview)
    .bind(&entry.poster_path)
    .bind(&entry.backdrop_path)
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

    sqlx::query(
        r#"
        update media_items
        set
            title = $2,
            original_title = null,
            sort_title = null,
            year = null,
            overview = $3,
            poster_path = $4,
            backdrop_path = $5,
            updated_at = now()
        where id = $1
        "#,
    )
    .bind(media_item_id)
    .bind(episode_title_for_entry(entry, episode_number))
    .bind(&entry.overview)
    .bind(&entry.poster_path)
    .bind(&entry.backdrop_path)
    .execute(&mut **tx)
    .await
    .context("failed to update episode media item during library sync")?;

    Ok(())
}

async fn insert_episode_record(
    tx: &mut Transaction<'_, Postgres>,
    media_item_id: i64,
    series_id: i64,
    season_id: i64,
    episode_number: i32,
    title: String,
) -> Result<()> {
    sqlx::query(
        r#"
        insert into episodes (media_item_id, series_id, season_id, episode_number, title)
        values ($1, $2, $3, $4, $5)
        "#,
    )
    .bind(media_item_id)
    .bind(series_id)
    .bind(season_id)
    .bind(episode_number)
    .bind(title)
    .execute(&mut **tx)
    .await
    .context("failed to insert episode record")?;

    Ok(())
}

async fn update_episode_record(
    tx: &mut Transaction<'_, Postgres>,
    media_item_id: i64,
    series_id: i64,
    season_id: i64,
    episode_number: i32,
    title: String,
) -> Result<()> {
    let updated = sqlx::query(
        r#"
        update episodes
        set
            series_id = $2,
            season_id = $3,
            episode_number = $4,
            title = $5,
            updated_at = now()
        where media_item_id = $1
        "#,
    )
    .bind(media_item_id)
    .bind(series_id)
    .bind(season_id)
    .bind(episode_number)
    .bind(title.clone())
    .execute(&mut **tx)
    .await
    .context("failed to update episode record")?;

    if updated.rows_affected() == 0 {
        insert_episode_record(
            tx,
            media_item_id,
            series_id,
            season_id,
            episode_number,
            title,
        )
        .await?;
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
