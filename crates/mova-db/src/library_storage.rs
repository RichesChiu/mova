use anyhow::{Context, Result};
use mova_domain::{LibraryStorageIssue, LIBRARY_STORAGE_AVAILABLE, LIBRARY_STORAGE_UNAVAILABLE};
use serde_json::json;
use sqlx::{postgres::PgPool, Postgres, Row, Transaction};

/// A mount a library's media lived on at its last successful scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryStorageMountRecord {
    pub mount_point: String,
    pub fs_type: String,
    pub source: String,
}

pub async fn list_library_storage_mounts(
    pool: &PgPool,
    library_id: i64,
) -> Result<Vec<LibraryStorageMountRecord>> {
    let rows = sqlx::query(
        r#"
        select mount_point, fs_type, source
        from library_storage_mounts
        where library_id = $1
        order by mount_point
        "#,
    )
    .bind(library_id)
    .fetch_all(pool)
    .await
    .context("failed to list library storage mounts")?;

    Ok(rows
        .into_iter()
        .map(|row| LibraryStorageMountRecord {
            mount_point: row.get("mount_point"),
            fs_type: row.get("fs_type"),
            source: row.get("source"),
        })
        .collect())
}

/// Replaces the storage record inside the scan's reconciliation transaction,
/// so the record always describes the storage the committed catalog came from.
pub(crate) async fn replace_library_storage_mounts_tx(
    tx: &mut Transaction<'_, Postgres>,
    library_id: i64,
    mounts: &[LibraryStorageMountRecord],
) -> Result<()> {
    sqlx::query("delete from library_storage_mounts where library_id = $1")
        .bind(library_id)
        .execute(&mut **tx)
        .await
        .context("failed to clear library storage mounts")?;

    let mount_points = mounts
        .iter()
        .map(|mount| mount.mount_point.clone())
        .collect::<Vec<_>>();
    let fs_types = mounts
        .iter()
        .map(|mount| mount.fs_type.clone())
        .collect::<Vec<_>>();
    let sources = mounts
        .iter()
        .map(|mount| mount.source.clone())
        .collect::<Vec<_>>();
    sqlx::query(
        r#"
        insert into library_storage_mounts (library_id, mount_point, fs_type, source)
        select $1, mount.mount_point, mount.fs_type, mount.source
        from unnest($2::text[], $3::text[], $4::text[])
            as mount(mount_point, fs_type, source)
        on conflict (library_id, mount_point) do update
        set fs_type = excluded.fs_type,
            source = excluded.source,
            recorded_at = now()
        "#,
    )
    .bind(library_id)
    .bind(mount_points)
    .bind(fs_types)
    .bind(sources)
    .execute(&mut **tx)
    .await
    .context("failed to record library storage mounts")?;

    Ok(())
}

/// Catalog paths of a library under `mount_point`, skipping paths that belong
/// to deeper mounts. Used to tell whether changed storage still holds the
/// library's known files.
pub async fn sample_library_media_file_paths(
    pool: &PgPool,
    library_id: i64,
    mount_point: &str,
    deeper_mount_points: &[String],
    limit: i64,
) -> Result<Vec<String>> {
    sqlx::query_scalar::<_, String>(
        r#"
        select file_path
        from media_files
        where library_id = $1
          and starts_with(file_path, $2)
          and not exists (
              select 1
              from unnest($3::text[]) as deeper(prefix)
              where starts_with(media_files.file_path, deeper.prefix)
          )
        order by id desc
        limit $4
        "#,
    )
    .bind(library_id)
    .bind(directory_prefix(mount_point))
    .bind(
        deeper_mount_points
            .iter()
            .map(|mount_point| directory_prefix(mount_point))
            .collect::<Vec<_>>(),
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("failed to sample library media file paths")
}

pub async fn library_has_media_files(pool: &PgPool, library_id: i64) -> Result<bool> {
    sqlx::query_scalar::<_, bool>("select exists(select 1 from media_files where library_id = $1)")
        .bind(library_id)
        .fetch_one(pool)
        .await
        .context("failed to check whether the library has media files")
}

fn directory_prefix(mount_point: &str) -> String {
    if mount_point.ends_with('/') {
        mount_point.to_string()
    } else {
        format!("{mount_point}/")
    }
}

/// Marks the library unavailable. The first transition notifies owners and
/// admins; later checks during the same outage only refresh the details.
/// Returns whether the library was available before.
pub async fn mark_library_storage_unavailable(
    pool: &PgPool,
    library_id: i64,
    issue: &LibraryStorageIssue,
) -> Result<bool> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to start library storage status transaction")?;
    let current = sqlx::query(
        r#"
        select name, storage_status, storage_issue
        from libraries
        where id = $1
        for update
        "#,
    )
    .bind(library_id)
    .fetch_optional(&mut *tx)
    .await
    .context("failed to lock library storage status")?;
    let Some(current) = current else {
        tx.commit()
            .await
            .context("failed to finish storage status update for a missing library")?;
        return Ok(false);
    };

    let issue_value =
        serde_json::to_value(issue).context("failed to encode library storage issue")?;
    let was_available = current.get::<String, _>("storage_status") == LIBRARY_STORAGE_AVAILABLE;
    if !was_available {
        let unchanged = current
            .get::<Option<serde_json::Value>, _>("storage_issue")
            .as_ref()
            == Some(&issue_value);
        if !unchanged {
            sqlx::query("update libraries set storage_issue = $2 where id = $1")
                .bind(library_id)
                .bind(&issue_value)
                .execute(&mut *tx)
                .await
                .context("failed to refresh library storage issue")?;
        }
        tx.commit()
            .await
            .context("failed to commit library storage issue refresh")?;
        return Ok(false);
    }

    let unavailable_since_micros = sqlx::query_scalar::<_, i64>(
        r#"
        update libraries
        set storage_status = $2,
            storage_issue = $3,
            storage_unavailable_since = now()
        where id = $1
        returning (extract(epoch from storage_unavailable_since) * 1000000)::bigint
        "#,
    )
    .bind(library_id)
    .bind(LIBRARY_STORAGE_UNAVAILABLE)
    .bind(&issue_value)
    .fetch_one(&mut *tx)
    .await
    .context("failed to mark library storage unavailable")?;

    let library_name = current.get::<String, _>("name");
    insert_admin_notification(
        &mut tx,
        "library.storage.unavailable",
        "error",
        &format!("library:{library_id}:storage-unavailable:{unavailable_since_micros}"),
        json!({
            "library_id": library_id,
            "library_name": library_name,
            "reason_code": issue.reason_code,
            "reason_params": {
                "library_name": library_name,
                "mount_point": issue.mount_point,
                "expected_fs_type": issue.expected_fs_type,
                "expected_source": issue.expected_source,
                "actual_fs_type": issue.actual_fs_type,
                "actual_source": issue.actual_source,
            },
            "diagnostic_message": issue.diagnostic_message,
        }),
    )
    .await?;
    tx.commit()
        .await
        .context("failed to commit library storage unavailability")?;

    Ok(true)
}

/// Returns whether the library was unavailable before.
pub async fn mark_library_storage_available(pool: &PgPool, library_id: i64) -> Result<bool> {
    let result = sqlx::query(
        r#"
        update libraries
        set storage_status = $2,
            storage_issue = null,
            storage_unavailable_since = null
        where id = $1
          and storage_status <> $2
        "#,
    )
    .bind(library_id)
    .bind(LIBRARY_STORAGE_AVAILABLE)
    .execute(pool)
    .await
    .context("failed to mark library storage available")?;

    Ok(result.rows_affected() > 0)
}

/// Tells owners and admins that a library was deleted because every folder it
/// was scanned from is gone from the deployment.
pub async fn record_library_removed_from_deployment(
    pool: &PgPool,
    library_id: i64,
    library_name: &str,
    mount_points: &[String],
) -> Result<()> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to start library removal notification transaction")?;
    insert_admin_notification(
        &mut tx,
        "library.removed_from_deployment",
        "warning",
        &format!("library:{library_id}:removed-from-deployment"),
        json!({
            "library_id": library_id,
            "library_name": library_name,
            "reason_code": "library_removed_from_deployment",
            "reason_params": {
                "library_name": library_name,
                "mount_points": mount_points,
            },
            "diagnostic_message": null,
        }),
    )
    .await?;
    tx.commit()
        .await
        .context("failed to commit library removal notification")
}

async fn insert_admin_notification(
    tx: &mut Transaction<'_, Postgres>,
    notification_type: &str,
    severity: &str,
    source_key: &str,
    payload: serde_json::Value,
) -> Result<()> {
    sqlx::query(
        r#"
        insert into notifications (
            category,
            notification_type,
            severity,
            audience,
            source_key,
            payload
        )
        values ('system', $1, $2, 'admin', $3, $4)
        on conflict (source_key) do update
        set notification_type = excluded.notification_type,
            severity = excluded.severity,
            payload = excluded.payload,
            updated_at = now()
        "#,
    )
    .bind(notification_type)
    .bind(severity)
    .bind(source_key)
    .bind(payload)
    .execute(&mut **tx)
    .await
    .context("failed to persist library storage notification")?;
    sqlx::query("select mova_bump_realtime_revision('admin:notifications')")
        .fetch_one(&mut **tx)
        .await
        .context("failed to bump admin notification revision")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        list_library_storage_mounts, mark_library_storage_available,
        mark_library_storage_unavailable, replace_library_storage_mounts_tx,
        sample_library_media_file_paths, LibraryStorageMountRecord,
    };
    use mova_domain::LibraryStorageIssue;
    use sqlx::postgres::PgPool;

    async fn seed_library(pool: &PgPool) -> i64 {
        sqlx::query_scalar(
            "insert into libraries (name, root_path) values ('TV', '/media/tv') returning id",
        )
        .fetch_one(pool)
        .await
        .unwrap()
    }

    async fn seed_file(pool: &PgPool, library_id: i64, file_path: &str) {
        let media_item_id: i64 = sqlx::query_scalar(
            r#"
            insert into media_items (library_id, media_type, title, source_title)
            values ($1, 'movie', $2, $2)
            returning id
            "#,
        )
        .bind(library_id)
        .bind(file_path)
        .fetch_one(pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            insert into media_files (library_id, media_item_id, file_path, file_size)
            values ($1, $2, $3, 1)
            "#,
        )
        .bind(library_id)
        .bind(media_item_id)
        .bind(file_path)
        .execute(pool)
        .await
        .unwrap();
    }

    fn issue(reason_code: &str) -> LibraryStorageIssue {
        LibraryStorageIssue {
            reason_code: reason_code.to_string(),
            mount_point: "/media".to_string(),
            expected_fs_type: Some("cifs".to_string()),
            expected_source: Some("//nas/media".to_string()),
            actual_fs_type: Some("ext4".to_string()),
            actual_source: Some("/dev/sda2".to_string()),
            diagnostic_message: None,
        }
    }

    async fn storage_notification_count(pool: &PgPool) -> i64 {
        sqlx::query_scalar(
            "select count(*) from notifications where notification_type = 'library.storage.unavailable'",
        )
        .fetch_one(pool)
        .await
        .unwrap()
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn storage_record_is_replaced_as_a_whole(pool: PgPool) {
        let library_id = seed_library(&pool).await;
        let record = |mount_point: &str, fs_type: &str| LibraryStorageMountRecord {
            mount_point: mount_point.to_string(),
            fs_type: fs_type.to_string(),
            source: format!("{fs_type}-source"),
        };

        let mut tx = pool.begin().await.unwrap();
        replace_library_storage_mounts_tx(
            &mut tx,
            library_id,
            &[record("/media", "cifs"), record("/media/tv/disk2", "ext4")],
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let mut tx = pool.begin().await.unwrap();
        replace_library_storage_mounts_tx(&mut tx, library_id, &[record("/media", "nfs4")])
            .await
            .unwrap();
        tx.commit().await.unwrap();

        assert_eq!(
            list_library_storage_mounts(&pool, library_id)
                .await
                .unwrap(),
            vec![record("/media", "nfs4")]
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn samples_skip_paths_owned_by_deeper_mounts(pool: PgPool) {
        let library_id = seed_library(&pool).await;
        seed_file(&pool, library_id, "/media/tv/a.mkv").await;
        seed_file(&pool, library_id, "/media/tv/disk2/b.mkv").await;
        seed_file(&pool, library_id, "/mediaother/c.mkv").await;

        let mut root_paths = sample_library_media_file_paths(
            &pool,
            library_id,
            "/media",
            &["/media/tv/disk2".to_string()],
            10,
        )
        .await
        .unwrap();
        root_paths.sort();
        let disk_paths =
            sample_library_media_file_paths(&pool, library_id, "/media/tv/disk2", &[], 10)
                .await
                .unwrap();
        let everything = sample_library_media_file_paths(&pool, library_id, "/", &[], 10)
            .await
            .unwrap();

        assert_eq!(root_paths, vec!["/media/tv/a.mkv".to_string()]);
        assert_eq!(disk_paths, vec!["/media/tv/disk2/b.mkv".to_string()]);
        assert_eq!(everything.len(), 3);
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn each_outage_notifies_once_and_recovery_clears_the_issue(pool: PgPool) {
        let library_id = seed_library(&pool).await;

        assert!(mark_library_storage_unavailable(
            &pool,
            library_id,
            &issue("storage_not_connected")
        )
        .await
        .unwrap());
        assert!(
            !mark_library_storage_unavailable(&pool, library_id, &issue("storage_unreadable"))
                .await
                .unwrap()
        );
        assert_eq!(storage_notification_count(&pool).await, 1);
        let (status, reason): (String, String) = sqlx::query_as(
            "select storage_status, storage_issue->>'reason_code' from libraries where id = $1",
        )
        .bind(library_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            (status.as_str(), reason.as_str()),
            ("unavailable", "storage_unreadable")
        );

        assert!(mark_library_storage_available(&pool, library_id)
            .await
            .unwrap());
        assert!(!mark_library_storage_available(&pool, library_id)
            .await
            .unwrap());
        let cleared: (String, bool, bool) = sqlx::query_as(
            r#"
            select storage_status, storage_issue is null, storage_unavailable_since is null
            from libraries
            where id = $1
            "#,
        )
        .bind(library_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(cleared, ("available".to_string(), true, true));

        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        assert!(
            mark_library_storage_unavailable(&pool, library_id, &issue("storage_timeout"))
                .await
                .unwrap()
        );
        assert_eq!(storage_notification_count(&pool).await, 2);
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn storage_status_columns_reject_inconsistent_rows(pool: PgPool) {
        let library_id = seed_library(&pool).await;

        let error =
            sqlx::query("update libraries set storage_status = 'unavailable' where id = $1")
                .bind(library_id)
                .execute(&pool)
                .await
                .unwrap_err();
        assert_eq!(
            error
                .as_database_error()
                .and_then(|error| error.code())
                .as_deref(),
            Some("23514")
        );
    }
}
