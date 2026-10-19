//! Scoped scans, the click-time inspection and episode retention: a scan
//! limited to an item's directories touches nothing else, an episode without
//! files stays in its series with its records while the series has files, and
//! opening an item reports which versions are gone without deleting anything.

use super::tests::{library_file_paths, run_scan_with_scope};
use super::{ExecuteScanJobOutcome, ScanScope};
use crate::{inspect_media_item_sources, MediaSourceStatus};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

#[test]
fn a_scan_scope_keeps_only_directories_inside_the_root() {
    let whole = ScanScope::from_requested("/media/movies", None);
    assert!(!whole.is_scoped());
    assert_eq!(whole.prefixes(), None);

    let scoped = ScanScope::from_requested(
        "/media/movies/",
        Some(vec![
            "/media/movies/Dune (2021)/".to_string(),
            "/media/movies/Dune (2021)/extras".to_string(),
            "/media/movies/Alien (1979)".to_string(),
            "/media/movies/Alien (1979)".to_string(),
            "/media/moviesX/Other".to_string(),
            "/media".to_string(),
            "/media/movies".to_string(),
        ]),
    );
    assert!(scoped.is_scoped());
    assert_eq!(
        scoped.prefixes(),
        Some(
            &[
                "/media/movies/Alien (1979)".to_string(),
                "/media/movies/Dune (2021)".to_string()
            ][..]
        )
    );

    // Every requested directory lies outside the root: scoped and empty, not
    // a whole-library scan.
    let rejected = ScanScope::from_requested("/media/movies", Some(vec!["/other".to_string()]));
    assert!(rejected.is_scoped());
    assert_eq!(rejected.prefixes(), Some(&[][..]));
}

struct Fixture {
    base: PathBuf,
    media: PathBuf,
    cache: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let base =
            std::env::temp_dir().join(format!("mova-inspection-{name}-{}", uuid::Uuid::new_v4()));
        let media = base.join("media");
        let cache = base.join("cache");
        fs::create_dir_all(&media).unwrap();
        fs::create_dir_all(&cache).unwrap();
        Self { base, media, cache }
    }

    fn write(&self, relative: &str, content: &[u8]) -> PathBuf {
        let path = self.media.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, content).unwrap();
        path
    }

    fn dir(&self, relative: &str) -> String {
        self.media.join(relative).to_string_lossy().into_owned()
    }

    async fn library(&self, pool: &sqlx::PgPool) -> mova_domain::Library {
        mova_db::create_library(
            pool,
            mova_db::CreateLibraryParams {
                name: "Library".to_string(),
                description: None,
                metadata_language: "en-US".to_string(),
                root_path: self.media.to_string_lossy().into_owned(),
            },
        )
        .await
        .unwrap()
    }

    async fn scan(&self, pool: &sqlx::PgPool, library_id: i64) {
        self.scan_with_scope(pool, library_id, Vec::new()).await;
    }

    async fn scan_with_scope(&self, pool: &sqlx::PgPool, library_id: i64, scope: Vec<String>) {
        let outcome = run_scan_with_scope(
            pool,
            library_id,
            &self.cache,
            Arc::new(crate::HostLibraryStorageEnvironment),
            scope,
        )
        .await
        .unwrap();
        match outcome {
            ExecuteScanJobOutcome::Completed(job) => assert_eq!(job.status, "success"),
            other => panic!("the scan must succeed: {other:?}"),
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

fn paths(paths: &[&Path]) -> Vec<String> {
    let mut paths = paths
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    paths.sort();
    paths
}

async fn item_of_path(pool: &sqlx::PgPool, path: &Path) -> i64 {
    sqlx::query_scalar("select media_item_id from media_files where file_path = $1")
        .bind(path.to_string_lossy().into_owned())
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn item_exists(pool: &sqlx::PgPool, item_id: i64) -> bool {
    sqlx::query_scalar("select exists(select 1 from media_items where id = $1)")
        .bind(item_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn add_progress(pool: &sqlx::PgPool, library_id: i64, path: &Path) -> i64 {
    let (file_id, item_id) = sqlx::query_as::<_, (i64, i64)>(
        "select id, media_item_id from media_files where file_path = $1",
    )
    .bind(path.to_string_lossy().into_owned())
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
            duration_seconds: Some(2400),
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

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
async fn a_scoped_scan_touches_nothing_outside_its_directories(pool: sqlx::PgPool) {
    let fixture = Fixture::new("scope");
    let alpha = fixture.write("Alpha (2020)/Alpha (2020).mkv", b"alpha");
    let beta = fixture.write("Beta (2021)/Beta (2021).mkv", b"beta");
    let library = fixture.library(&pool).await;
    fixture.scan(&pool, library.id).await;
    assert_eq!(
        library_file_paths(&pool, library.id).await,
        paths(&[&alpha, &beta])
    );

    // Both files vanish, but only Alpha's directory is inspected.
    fs::remove_file(&alpha).unwrap();
    fs::remove_file(&beta).unwrap();
    let gamma = fixture.write("Gamma (2022)/Gamma (2022).mkv", b"gamma");
    fixture
        .scan_with_scope(&pool, library.id, vec![fixture.dir("Alpha (2020)")])
        .await;
    assert_eq!(library_file_paths(&pool, library.id).await, paths(&[&beta]));
    let scan_job = mova_db::get_latest_scan_job_for_library(&pool, library.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        scan_job.scope_paths,
        Some(vec![fixture.dir("Alpha (2020)")])
    );

    // The next whole-library scan settles the rest.
    fixture.scan(&pool, library.id).await;
    assert_eq!(
        library_file_paths(&pool, library.id).await,
        paths(&[&gamma])
    );
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
async fn a_scoped_scan_follows_a_rename_inside_its_directories(pool: sqlx::PgPool) {
    let fixture = Fixture::new("scope-rename");
    let old_path = fixture.write("Dune (2021)/Dnue (2021).mkv", b"the film");
    let library = fixture.library(&pool).await;
    fixture.scan(&pool, library.id).await;
    let old_item_id = add_progress(&pool, library.id, &old_path).await;

    let new_path = old_path.with_file_name("Dune (2021).mkv");
    fs::rename(&old_path, &new_path).unwrap();
    fixture
        .scan_with_scope(&pool, library.id, vec![fixture.dir("Dune (2021)")])
        .await;

    assert_eq!(
        library_file_paths(&pool, library.id).await,
        paths(&[&new_path])
    );
    let new_item_id = item_of_path(&pool, &new_path).await;
    assert!(!item_exists(&pool, old_item_id).await);
    assert_eq!(progress_count(&pool, new_item_id).await, 1);
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
async fn an_episode_without_files_stays_with_its_records_while_the_series_has_files(
    pool: sqlx::PgPool,
) {
    let fixture = Fixture::new("episodes");
    let first = fixture.write("Show/Season 01/Show S01E01.mkv", b"episode one");
    let second = fixture.write("Show/Season 01/Show S01E02.mkv", b"episode two");
    let library = fixture.library(&pool).await;
    fixture.scan(&pool, library.id).await;
    let first_item_id = add_progress(&pool, library.id, &first).await;
    let second_item_id = item_of_path(&pool, &second).await;

    // The watched episode's file is deleted; its entry and records stay.
    fs::remove_file(&first).unwrap();
    fixture
        .scan_with_scope(&pool, library.id, vec![fixture.dir("Show/Season 01")])
        .await;
    assert_eq!(
        library_file_paths(&pool, library.id).await,
        paths(&[&second])
    );
    assert!(item_exists(&pool, first_item_id).await);
    assert_eq!(progress_count(&pool, first_item_id).await, 1);
    let episode_rows: Vec<(i64, i32, bool)> = sqlx::query_as(
        r#"
        select e.media_item_id, e.episode_number,
               exists(select 1 from media_files mf where mf.media_item_id = e.media_item_id)
        from episodes e
        join media_items mi on mi.id = e.media_item_id
        where mi.library_id = $1
        order by e.episode_number
        "#,
    )
    .bind(library.id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        episode_rows,
        vec![(first_item_id, 1, false), (second_item_id, 2, true)]
    );
    // Opening the kept episode reports it missing without another rescan.
    let library_row = mova_db::get_library(&pool, library.id)
        .await
        .unwrap()
        .unwrap();
    let ghost = mova_db::get_media_item(&pool, first_item_id)
        .await
        .unwrap()
        .unwrap();
    let inspection = inspect_media_item_sources(&pool, &library_row, &ghost)
        .await
        .unwrap();
    assert_eq!(inspection.status, MediaSourceStatus::Missing);
    assert!(inspection.rescan_directories.is_empty());

    // The file comes back under its name: it attaches to the same episode.
    fs::write(&first, b"episode one again").unwrap();
    fixture.scan(&pool, library.id).await;
    assert_eq!(item_of_path(&pool, &first).await, first_item_id);
    assert_eq!(progress_count(&pool, first_item_id).await, 1);

    // With no file left anywhere, the series goes with its records.
    fs::remove_file(&first).unwrap();
    fs::remove_file(&second).unwrap();
    fixture.scan(&pool, library.id).await;
    assert!(library_file_paths(&pool, library.id).await.is_empty());
    assert!(!item_exists(&pool, first_item_id).await);
    assert!(!item_exists(&pool, second_item_id).await);
    assert_eq!(progress_count(&pool, first_item_id).await, 0);
    let remaining: i64 =
        sqlx::query_scalar("select count(*) from media_items where library_id = $1")
            .bind(library.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(remaining, 0);
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
async fn opening_an_item_reports_its_missing_versions_without_deleting_anything(
    pool: sqlx::PgPool,
) {
    let fixture = Fixture::new("inspect");
    let small = fixture.write("Film (2020)/Film (2020) - 1080p.mkv", b"small");
    let large = fixture.write("Film (2020)/Film (2020) - 2160p.mkv", b"large version");
    let library = fixture.library(&pool).await;
    fixture.scan(&pool, library.id).await;
    let item_id = item_of_path(&pool, &small).await;
    assert_eq!(item_of_path(&pool, &large).await, item_id);
    let library = mova_db::get_library(&pool, library.id)
        .await
        .unwrap()
        .unwrap();
    let item = mova_db::get_media_item(&pool, item_id)
        .await
        .unwrap()
        .unwrap();
    let file_id = |path: &Path| {
        let path = path.to_string_lossy().into_owned();
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, i64>("select id from media_files where file_path = $1")
                .bind(path)
                .fetch_one(&pool)
                .await
                .unwrap()
        }
    };

    let all_present = inspect_media_item_sources(&pool, &library, &item)
        .await
        .unwrap();
    assert_eq!(all_present.status, MediaSourceStatus::Available);
    assert!(all_present.missing_media_file_ids.is_empty());
    assert!(all_present.rescan_directories.is_empty());

    fs::remove_file(&large).unwrap();
    let one_missing = inspect_media_item_sources(&pool, &library, &item)
        .await
        .unwrap();
    assert_eq!(one_missing.status, MediaSourceStatus::Available);
    assert_eq!(
        one_missing.missing_media_file_ids,
        vec![file_id(&large).await]
    );
    assert_eq!(
        one_missing.rescan_directories,
        vec![fixture.dir("Film (2020)")]
    );

    fs::remove_file(&small).unwrap();
    let all_missing = inspect_media_item_sources(&pool, &library, &item)
        .await
        .unwrap();
    assert_eq!(all_missing.status, MediaSourceStatus::Missing);
    assert_eq!(all_missing.missing_media_file_ids.len(), 2);
    assert_eq!(
        all_missing.rescan_directories,
        vec![fixture.dir("Film (2020)")]
    );
    // Looking never deletes.
    assert_eq!(library_file_paths(&pool, library.id).await.len(), 2);

    // Unavailable storage: nothing is looked up.
    let mut offline = library.clone();
    offline.storage_status = mova_domain::LIBRARY_STORAGE_UNAVAILABLE.to_string();
    let unchecked = inspect_media_item_sources(&pool, &offline, &item)
        .await
        .unwrap();
    assert_eq!(unchecked.status, MediaSourceStatus::StorageUnavailable);
    assert!(unchecked.missing_media_file_ids.is_empty());
}
