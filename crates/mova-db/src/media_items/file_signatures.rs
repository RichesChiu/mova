//! What identifies a local file independently of its path, so a renamed or
//! moved file can be recognized and its users' playback state carried to the
//! item it resolves to.

use anyhow::{Context, Result};
use sqlx::{postgres::PgPool, Row};

/// What identifies a local file independently of its path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalMediaFileSignature {
    pub media_file_id: i64,
    pub media_item_id: i64,
    pub file_path: String,
    pub file_size: i64,
    /// Unknown for rows written before the column existed; backfilled from
    /// the next scan's inventory.
    pub file_modified_at_ms: Option<i64>,
}

pub async fn list_local_media_file_signatures(
    pool: &PgPool,
    library_id: i64,
) -> Result<Vec<LocalMediaFileSignature>> {
    let rows = sqlx::query(
        r#"
        select id, media_item_id, file_path, file_size, file_modified_at_ms
        from media_files
        where library_id = $1
          and source_kind = 'local_file'
        order by file_path
        "#,
    )
    .bind(library_id)
    .fetch_all(pool)
    .await
    .context("failed to list local media file signatures")?;

    Ok(rows
        .into_iter()
        .map(|row| LocalMediaFileSignature {
            media_file_id: row.get("id"),
            media_item_id: row.get("media_item_id"),
            file_path: row.get("file_path"),
            file_size: row.get("file_size"),
            file_modified_at_ms: row.get("file_modified_at_ms"),
        })
        .collect())
}

/// Records the modification time of rows that have none yet, so files
/// catalogued before the column existed can be followed when renamed.
pub async fn backfill_media_file_modified_at(
    pool: &PgPool,
    library_id: i64,
    entries: &[(String, i64)],
) -> Result<u64> {
    if entries.is_empty() {
        return Ok(0);
    }
    let paths = entries
        .iter()
        .map(|(path, _)| path.clone())
        .collect::<Vec<_>>();
    let times = entries.iter().map(|(_, time)| *time).collect::<Vec<_>>();
    let result = sqlx::query(
        r#"
        update media_files mf
        set file_modified_at_ms = observed.modified_at_ms
        from unnest($2::text[], $3::bigint[]) as observed(file_path, modified_at_ms)
        where mf.library_id = $1
          and mf.file_path = observed.file_path
          and mf.file_modified_at_ms is null
        "#,
    )
    .bind(library_id)
    .bind(paths)
    .bind(times)
    .execute(pool)
    .await
    .context("failed to backfill media file modification times")?;
    Ok(result.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::{backfill_media_file_modified_at, list_local_media_file_signatures};
    use sqlx::postgres::PgPool;

    async fn seed(pool: &PgPool) -> (i64, i64) {
        let library_id: i64 = sqlx::query_scalar(
            "insert into libraries (name, root_path) values ('Movies', '/media/movies') returning id",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        let media_item_id: i64 = sqlx::query_scalar(
            "insert into media_items (library_id, media_type, title, source_title) values ($1, 'movie', 'Dnue', 'Dnue') returning id",
        )
        .bind(library_id)
        .fetch_one(pool)
        .await
        .unwrap();
        let media_file_id: i64 = sqlx::query_scalar(
            "insert into media_files (library_id, media_item_id, file_path, file_size, scan_hash) values ($1, $2, '/media/movies/Dnue (2021)/Dnue (2021).mkv', 42, 'hash') returning id",
        )
        .bind(library_id)
        .bind(media_item_id)
        .fetch_one(pool)
        .await
        .unwrap();
        (library_id, media_file_id)
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn backfill_fills_only_unknown_modification_times(pool: PgPool) {
        let (library_id, media_file_id) = seed(&pool).await;
        let path = "/media/movies/Dnue (2021)/Dnue (2021).mkv".to_string();

        let updated = backfill_media_file_modified_at(
            &pool,
            library_id,
            &[
                (path.clone(), 1_700_000_000_000),
                ("/media/movies/none.mkv".to_string(), 1),
            ],
        )
        .await
        .unwrap();
        assert_eq!(updated, 1);
        let updated = backfill_media_file_modified_at(&pool, library_id, &[(path, 5)])
            .await
            .unwrap();
        assert_eq!(updated, 0);
        let stored: Option<i64> =
            sqlx::query_scalar("select file_modified_at_ms from media_files where id = $1")
                .bind(media_file_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(stored, Some(1_700_000_000_000));
        let signatures = list_local_media_file_signatures(&pool, library_id)
            .await
            .unwrap();
        assert_eq!(signatures.len(), 1);
        assert_eq!(signatures[0].file_modified_at_ms, Some(1_700_000_000_000));
    }
}
