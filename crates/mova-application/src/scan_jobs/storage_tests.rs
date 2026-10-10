//! End-to-end scans over storage that disconnects, changes or disappears.
//! Mounts are described by a test environment over real temporary folders:
//! a "disconnected" mount is an emptied folder whose mount table entry now
//! names a different filesystem, as Docker shows an unmounted share.

use super::tests::{library_file_paths, run_scan};
use super::ExecuteScanJobOutcome;
use crate::library_storage::TestStorageEnvironment;
use crate::{ApplicationError, LibraryStorageCheck};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

struct Fixture {
    base: PathBuf,
    share: PathBuf,
    root: PathBuf,
    cache: PathBuf,
}

impl Fixture {
    /// `<base>/share` stands for a mounted share; the library root is
    /// `<base>/share/TV`.
    fn new(name: &str) -> Self {
        let base = std::env::temp_dir().join(format!(
            "mova-storage-{name}-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let share = base.join("share");
        let root = share.join("TV");
        let cache = base.join("cache");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&cache).unwrap();
        Self {
            base,
            share,
            root,
            cache,
        }
    }

    fn write_movie(&self, relative: &str) -> String {
        let path = self.root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"video").unwrap();
        path.to_string_lossy().into_owned()
    }

    async fn library(&self, pool: &sqlx::PgPool) -> mova_domain::Library {
        mova_db::create_library(
            pool,
            mova_db::CreateLibraryParams {
                name: "TV".to_string(),
                description: None,
                metadata_language: "en-US".to_string(),
                root_path: self.root.to_string_lossy().into_owned(),
            },
        )
        .await
        .unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

async fn scan(
    pool: &sqlx::PgPool,
    library_id: i64,
    fixture: &Fixture,
    storage: &Arc<TestStorageEnvironment>,
) -> crate::ApplicationResult<ExecuteScanJobOutcome> {
    run_scan(pool, library_id, &fixture.cache, storage.clone()).await
}

async fn scan_succeeds(
    pool: &sqlx::PgPool,
    library_id: i64,
    fixture: &Fixture,
    storage: &Arc<TestStorageEnvironment>,
) {
    match scan(pool, library_id, fixture, storage).await {
        Ok(ExecuteScanJobOutcome::Completed(job)) => assert_eq!(job.status, "success"),
        other => panic!("the scan must succeed: {other:?}"),
    }
}

async fn library_status(pool: &sqlx::PgPool, library_id: i64) -> (String, Option<String>) {
    sqlx::query_as(
        "select storage_status, storage_issue->>'reason_code' from libraries where id = $1",
    )
    .bind(library_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn recorded_mounts(pool: &sqlx::PgPool, library_id: i64) -> Vec<(String, String)> {
    mova_db::list_library_storage_mounts(pool, library_id)
        .await
        .unwrap()
        .into_iter()
        .map(|mount| (mount.mount_point, mount.fs_type))
        .collect()
}

async fn add_progress(pool: &sqlx::PgPool, library_id: i64, file_path: &str) -> i64 {
    let (file_id, item_id) = sqlx::query_as::<_, (i64, i64)>(
        "select id, media_item_id from media_files where file_path = $1",
    )
    .bind(file_path)
    .fetch_one(pool)
    .await
    .unwrap();
    let viewer = mova_db::create_user(
        pool,
        mova_db::CreateUserParams {
            username: format!("viewer{item_id}"),
            username_normalized: format!("viewer{item_id}"),
            nickname: "Viewer".to_string(),
            password_hash: "unused".to_string(),
            role: mova_domain::UserRole::Viewer,
            is_enabled: true,
            library_ids: vec![library_id],
        },
    )
    .await
    .unwrap();
    mova_db::upsert_playback_progress(
        pool,
        mova_db::UpsertPlaybackProgressParams {
            user_id: viewer.user.id,
            media_item_id: item_id,
            media_file_id: file_id,
            position_seconds: 600,
            duration_seconds: Some(5400),
            is_finished: false,
        },
    )
    .await
    .unwrap();
    item_id
}

async fn progress_count(pool: &sqlx::PgPool, item_id: i64) -> i64 {
    sqlx::query_scalar("select count(*) from playback_progress where media_item_id = $1")
        .bind(item_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

fn empty_dir(path: &Path) {
    fs::remove_dir_all(path).unwrap();
    fs::create_dir_all(path).unwrap();
}

/// The user's own layout: one CIFS share holds the library. The share is not
/// connected at the next start, so the folder is empty local storage.
#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
async fn a_disconnected_network_root_keeps_everything_and_refuses_scans(pool: sqlx::PgPool) {
    let fixture = Fixture::new("network-root");
    let alpha = fixture.write_movie("Alpha (2020)/Alpha (2020).mkv");
    let beta = fixture.write_movie("Beta (2021)/Beta (2021).mkv");
    let library = fixture.library(&pool).await;
    let storage = Arc::new(TestStorageEnvironment::new(
        &[(&fixture.share, "cifs", "//nas/media")],
        true,
    ));
    scan_succeeds(&pool, library.id, &fixture, &storage).await;
    let alpha_item = add_progress(&pool, library.id, &alpha).await;
    assert_eq!(
        recorded_mounts(&pool, library.id).await,
        vec![(
            fixture.share.to_string_lossy().into_owned(),
            "cifs".to_string()
        )]
    );

    // The share is not mounted: Docker binds the empty host folder.
    let backup = fixture.base.join("backup");
    fs::rename(&fixture.share, &backup).unwrap();
    fs::create_dir_all(&fixture.share).unwrap();
    storage.set(&[(&fixture.share, "ext4", "/dev/sda2")]);

    let error = super::enqueue_library_scan(&pool, library.id, storage.as_ref())
        .await
        .unwrap_err();
    let ApplicationError::Business(error) = error else {
        panic!("a scan of unavailable storage is a business error: {error:?}");
    };
    assert_eq!(error.code(), "library_storage_unavailable");
    assert_eq!(
        error
            .params()
            .get("reason_code")
            .and_then(|value| value.as_str()),
        Some("storage_not_connected")
    );
    assert_eq!(
        library_status(&pool, library.id).await,
        (
            "unavailable".to_string(),
            Some("storage_not_connected".to_string())
        )
    );
    assert!(scan(&pool, library.id, &fixture, &storage).await.is_err());
    assert_eq!(
        library_file_paths(&pool, library.id).await,
        vec![alpha.clone(), beta.clone()]
    );
    assert_eq!(progress_count(&pool, alpha_item).await, 1);
    let notifications: i64 = sqlx::query_scalar(
        "select count(*) from notifications where notification_type = 'library.storage.unavailable'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(notifications, 1);

    // The share comes back: the library is available and nothing was lost.
    fs::remove_dir_all(&fixture.share).unwrap();
    fs::rename(&backup, &fixture.share).unwrap();
    storage.set(&[(&fixture.share, "cifs", "//nas/media")]);
    scan_succeeds(&pool, library.id, &fixture, &storage).await;
    assert_eq!(
        library_status(&pool, library.id).await,
        ("available".to_string(), None)
    );
    assert_eq!(
        library_file_paths(&pool, library.id).await,
        vec![alpha, beta]
    );
    assert_eq!(progress_count(&pool, alpha_item).await, 1);
}

/// A second disk mounted below the library root is not connected, so Docker
/// shows an empty folder there. It is not compared with its record: it has no
/// files, so its media is removed and the rest is untouched.
#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
async fn a_disconnected_sub_mount_removes_only_its_media(pool: sqlx::PgPool) {
    let fixture = Fixture::new("sub-mount");
    let kept = fixture.write_movie("Alpha (2020)/Alpha (2020).mkv");
    let on_disk2 = fixture.write_movie("disk2/Beta (2021)/Beta (2021).mkv");
    let disk2 = fixture.root.join("disk2");
    let library = fixture.library(&pool).await;
    let storage = Arc::new(TestStorageEnvironment::new(
        &[
            (&fixture.share, "cifs", "//nas/media"),
            (&disk2, "cifs", "//nas2/disk2"),
        ],
        true,
    ));
    scan_succeeds(&pool, library.id, &fixture, &storage).await;
    let kept_item = add_progress(&pool, library.id, &kept).await;
    assert_eq!(recorded_mounts(&pool, library.id).await.len(), 2);

    empty_dir(&disk2);
    storage.set(&[
        (&fixture.share, "cifs", "//nas/media"),
        (&disk2, "ext4", "/dev/sda2"),
    ]);
    scan_succeeds(&pool, library.id, &fixture, &storage).await;

    assert_eq!(
        library_file_paths(&pool, library.id).await,
        vec![kept.clone()]
    );
    assert_eq!(progress_count(&pool, kept_item).await, 1);
    assert_eq!(
        library_status(&pool, library.id).await,
        ("available".to_string(), None)
    );
    assert_eq!(
        recorded_mounts(&pool, library.id).await,
        vec![
            (
                fixture.share.to_string_lossy().into_owned(),
                "cifs".to_string()
            ),
            (disk2.to_string_lossy().into_owned(), "ext4".to_string()),
        ]
    );

    // A sub-mount that cannot be read at all is skipped as nonexistent.
    let disk3 = fixture.root.join("disk3");
    storage.set(&[
        (&fixture.share, "cifs", "//nas/media"),
        (&disk2, "ext4", "/dev/sda2"),
        (&disk3, "cifs", "//nas3/disk3"),
    ]);
    scan_succeeds(&pool, library.id, &fixture, &storage).await;
    assert_eq!(library_file_paths(&pool, library.id).await, vec![kept]);
    assert_eq!(recorded_mounts(&pool, library.id).await.len(), 2);
    let _ = on_disk2;
}

/// Local storage holding the root is not the disk that was recorded: the
/// library is cleared and kept. When the disk returns, the media is scanned in
/// again.
#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
async fn a_missing_local_root_disk_clears_the_library_and_keeps_it(pool: sqlx::PgPool) {
    let fixture = Fixture::new("local-root");
    let alpha = fixture.write_movie("Alpha (2020)/Alpha (2020).mkv");
    let library = fixture.library(&pool).await;
    let storage = Arc::new(TestStorageEnvironment::new(
        &[(&fixture.share, "ext4", "/dev/sdb1")],
        true,
    ));
    scan_succeeds(&pool, library.id, &fixture, &storage).await;

    let backup = fixture.base.join("backup");
    fs::rename(&fixture.share, &backup).unwrap();
    fs::create_dir_all(&fixture.share).unwrap();
    storage.set(&[(&fixture.share, "ext4", "/dev/sda2")]);
    scan_succeeds(&pool, library.id, &fixture, &storage).await;
    assert!(library_file_paths(&pool, library.id).await.is_empty());
    assert_eq!(
        library_status(&pool, library.id).await,
        ("available".to_string(), None)
    );
    // The previous record stays, so the disk is recognized when it returns.
    assert_eq!(
        mova_db::list_library_storage_mounts(&pool, library.id)
            .await
            .unwrap()[0]
            .source,
        "/dev/sdb1"
    );

    fs::remove_dir_all(&fixture.share).unwrap();
    fs::rename(&backup, &fixture.share).unwrap();
    storage.set(&[(&fixture.share, "ext4", "/dev/sdb1")]);
    scan_succeeds(&pool, library.id, &fixture, &storage).await;
    assert_eq!(library_file_paths(&pool, library.id).await, vec![alpha]);
}

/// Connected, readable storage whose files are really gone is emptied.
#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
async fn connected_storage_without_files_empties_the_library(pool: sqlx::PgPool) {
    let fixture = Fixture::new("emptied");
    fixture.write_movie("Alpha (2020)/Alpha (2020).mkv");
    fixture.write_movie("Beta (2021)/Beta (2021).mkv");
    let library = fixture.library(&pool).await;
    let storage = Arc::new(TestStorageEnvironment::new(
        &[(&fixture.share, "cifs", "//nas/media")],
        true,
    ));
    scan_succeeds(&pool, library.id, &fixture, &storage).await;

    empty_dir(&fixture.root);
    scan_succeeds(&pool, library.id, &fixture, &storage).await;
    assert!(library_file_paths(&pool, library.id).await.is_empty());

    // The library folder itself is removed from connected storage.
    fixture.write_movie("Gamma (2022)/Gamma (2022).mkv");
    scan_succeeds(&pool, library.id, &fixture, &storage).await;
    assert_eq!(library_file_paths(&pool, library.id).await.len(), 1);
    fs::remove_dir_all(&fixture.root).unwrap();
    scan_succeeds(&pool, library.id, &fixture, &storage).await;
    assert!(library_file_paths(&pool, library.id).await.is_empty());
    assert_eq!(
        library_status(&pool, library.id).await,
        ("available".to_string(), None)
    );
}

/// The same storage under a new name (`sdb` became `sdc`) still holds the
/// library's files: it is accepted and the record follows it.
#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
async fn renamed_storage_that_still_holds_the_files_is_accepted(pool: sqlx::PgPool) {
    let fixture = Fixture::new("renamed");
    let alpha = fixture.write_movie("Alpha (2020)/Alpha (2020).mkv");
    let library = fixture.library(&pool).await;
    let storage = Arc::new(TestStorageEnvironment::new(
        &[(&fixture.share, "ext4", "/dev/sdb1")],
        true,
    ));
    scan_succeeds(&pool, library.id, &fixture, &storage).await;

    storage.set(&[(&fixture.share, "ext4", "/dev/sdc1")]);
    scan_succeeds(&pool, library.id, &fixture, &storage).await;
    assert_eq!(library_file_paths(&pool, library.id).await, vec![alpha]);
    assert_eq!(
        mova_db::list_library_storage_mounts(&pool, library.id)
            .await
            .unwrap()[0]
            .source,
        "/dev/sdc1"
    );
}

/// A folder removed from the deployment: one of two mounts removes its media;
/// every mount removed means the library is gone from the deployment.
#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
async fn folders_removed_from_the_deployment_remove_their_media_or_the_library(pool: sqlx::PgPool) {
    let fixture = Fixture::new("removed");
    let kept = fixture.write_movie("disk1/Alpha (2020)/Alpha (2020).mkv");
    fixture.write_movie("disk2/Beta (2021)/Beta (2021).mkv");
    let disk1 = fixture.root.join("disk1");
    let disk2 = fixture.root.join("disk2");
    let library = fixture.library(&pool).await;
    // The library root itself sits on the container's own filesystem.
    let storage = Arc::new(TestStorageEnvironment::new(
        &[(&disk1, "ext4", "/dev/sdb1"), (&disk2, "ext4", "/dev/sdc1")],
        true,
    ));
    scan_succeeds(&pool, library.id, &fixture, &storage).await;
    assert_eq!(library_file_paths(&pool, library.id).await.len(), 2);

    // disk2 is removed from the compose file: Docker leaves an empty folder.
    empty_dir(&disk2);
    storage.set(&[(&disk1, "ext4", "/dev/sdb1")]);
    scan_succeeds(&pool, library.id, &fixture, &storage).await;
    assert_eq!(library_file_paths(&pool, library.id).await, vec![kept]);

    storage.set(&[]);
    let library = mova_db::get_library(&pool, library.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        crate::check_library_storage(&pool, &library, storage.as_ref())
            .await
            .unwrap(),
        LibraryStorageCheck::RemovedFromDeployment {
            mount_points: vec![disk1.to_string_lossy().into_owned()],
        }
    );

    // Outside a container a missing mount may not be connected yet: the
    // media on it is treated as nonexistent, the library is not deleted.
    let host = Arc::new(TestStorageEnvironment::new(&[], false));
    assert_eq!(
        crate::check_library_storage(&pool, &library, host.as_ref())
            .await
            .unwrap(),
        LibraryStorageCheck::Available
    );
}

/// After an upgrade a library has media but no storage record. If none of
/// its known files can be found, nothing proves the storage is connected.
#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
async fn an_upgraded_library_without_a_record_is_only_trusted_with_its_files(pool: sqlx::PgPool) {
    let fixture = Fixture::new("upgrade");
    let alpha = fixture.write_movie("Alpha (2020)/Alpha (2020).mkv");
    let library = fixture.library(&pool).await;
    let storage = Arc::new(TestStorageEnvironment::new(
        &[(&fixture.share, "ext4", "/dev/sdb1")],
        true,
    ));
    scan_succeeds(&pool, library.id, &fixture, &storage).await;
    sqlx::query("delete from library_storage_mounts where library_id = $1")
        .bind(library.id)
        .execute(&pool)
        .await
        .unwrap();

    let backup = fixture.base.join("backup");
    fs::rename(&fixture.share, &backup).unwrap();
    fs::create_dir_all(&fixture.share).unwrap();
    storage.set(&[(&fixture.share, "ext4", "/dev/sda2")]);
    assert!(scan(&pool, library.id, &fixture, &storage).await.is_err());
    assert_eq!(
        library_status(&pool, library.id).await,
        (
            "unavailable".to_string(),
            Some("storage_unverified".to_string())
        )
    );
    assert_eq!(
        library_file_paths(&pool, library.id).await,
        vec![alpha.clone()]
    );

    fs::remove_dir_all(&fixture.share).unwrap();
    fs::rename(&backup, &fixture.share).unwrap();
    storage.set(&[(&fixture.share, "ext4", "/dev/sdb1")]);
    scan_succeeds(&pool, library.id, &fixture, &storage).await;
    assert_eq!(library_file_paths(&pool, library.id).await, vec![alpha]);
    assert_eq!(recorded_mounts(&pool, library.id).await.len(), 1);
}

/// A link whose target is gone is treated as nonexistent instead of failing
/// every scan of the library.
#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
async fn a_dangling_link_no_longer_fails_the_scan(pool: sqlx::PgPool) {
    let fixture = Fixture::new("dangling");
    let alpha = fixture.write_movie("Alpha (2020)/Alpha (2020).mkv");
    let target = fixture.write_movie("Beta (2021)/Beta (2021).mkv");
    let link = fixture.root.join("Linked (2022).mkv");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    std::os::unix::fs::symlink(fixture.base.join("gone"), fixture.root.join("Gone")).unwrap();
    let library = fixture.library(&pool).await;
    let storage = Arc::new(TestStorageEnvironment::new(
        &[(&fixture.share, "cifs", "//nas/media")],
        true,
    ));
    scan_succeeds(&pool, library.id, &fixture, &storage).await;
    let link_path = link.to_string_lossy().into_owned();
    assert_eq!(
        library_file_paths(&pool, library.id).await,
        vec![alpha.clone(), target.clone(), link_path]
    );

    fs::remove_file(&target).unwrap();
    scan_succeeds(&pool, library.id, &fixture, &storage).await;
    assert_eq!(library_file_paths(&pool, library.id).await, vec![alpha]);
}
