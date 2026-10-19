//! Movie identity: a titled file takes a matching directory's year, and a
//! renamed or moved file carries its playback state to the item its new
//! path resolves to when that is the same work.

use super::tests::{build_discovered_file, library_file_paths, run_scan};
use super::ExecuteScanJobOutcome;
use crate::media_classification::apply_movie_container_identity_when_title_is_missing;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime},
};

fn movie_file(path: &str, title: &str, year: Option<i32>) -> mova_scan::DiscoveredMediaFile {
    let mut file = build_discovered_file();
    file.file_path = PathBuf::from(path);
    file.title = title.to_string();
    file.source_title = title.to_string();
    file.year = year;
    file.season_number = None;
    file.episode_number = None;
    file.metadata_provider_item_id = None;
    file
}

#[test]
fn a_titled_file_takes_the_year_of_a_directory_naming_the_same_film() {
    let root = Path::new("/media/movies");

    let mut same = movie_file("/media/movies/Dune (1984)/Dune.mkv", "Dune", None);
    apply_movie_container_identity_when_title_is_missing(&mut same, root);
    assert_eq!(same.year, Some(1984));
    assert_eq!(same.source_title, "Dune");

    let mut punctuation = movie_file(
        "/media/movies/Spider-Man (2002)/Spider Man.mkv",
        "Spider Man",
        None,
    );
    apply_movie_container_identity_when_title_is_missing(&mut punctuation, root);
    assert_eq!(punctuation.year, Some(2002));

    // The filename's own year wins.
    let mut own_year = movie_file(
        "/media/movies/Dune (1984)/Dune (2021).mkv",
        "Dune",
        Some(2021),
    );
    apply_movie_container_identity_when_title_is_missing(&mut own_year, root);
    assert_eq!(own_year.year, Some(2021));

    // A directory naming a different film contributes nothing.
    let mut other = movie_file(
        "/media/movies/Dune (2021)/Dune Part Two.mkv",
        "Dune Part Two",
        None,
    );
    apply_movie_container_identity_when_title_is_missing(&mut other, root);
    assert_eq!(other.year, None);

    // The library root is never a container.
    let mut at_root = movie_file("/media/movies/Dune.mkv", "Dune", None);
    apply_movie_container_identity_when_title_is_missing(&mut at_root, Path::new("/media/movies"));
    assert_eq!(at_root.year, None);
}

struct Fixture {
    base: PathBuf,
    media: PathBuf,
    cache: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let base =
            std::env::temp_dir().join(format!("mova-identity-{name}-{}", uuid::Uuid::new_v4()));
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

    async fn library(&self, pool: &sqlx::PgPool) -> mova_domain::Library {
        mova_db::create_library(
            pool,
            mova_db::CreateLibraryParams {
                name: "Movies".to_string(),
                description: None,
                metadata_language: "en-US".to_string(),
                root_path: self.media.to_string_lossy().into_owned(),
            },
        )
        .await
        .unwrap()
    }

    async fn scan(&self, pool: &sqlx::PgPool, library_id: i64) {
        let outcome = run_scan(
            pool,
            library_id,
            &self.cache,
            Arc::new(crate::HostLibraryStorageEnvironment),
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

fn set_modified(path: &Path, time: SystemTime) {
    fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(time)
        .unwrap();
}

async fn movies(pool: &sqlx::PgPool, library_id: i64) -> Vec<(i64, String, Option<i32>)> {
    sqlx::query_as(
        "select id, title, year from media_items where library_id = $1 and media_type = 'movie' order by year, id",
    )
    .bind(library_id)
    .fetch_all(pool)
    .await
    .unwrap()
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

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
async fn same_title_in_directories_with_different_years_stays_two_films(pool: sqlx::PgPool) {
    let fixture = Fixture::new("years");
    fixture.write("Dune (1984)/Dune.mkv", b"the 1984 film");
    fixture.write("Dune (2021)/Dune.mkv", b"the 2021 film, longer");
    let library = fixture.library(&pool).await;

    fixture.scan(&pool, library.id).await;

    let items = movies(&pool, library.id).await;
    assert_eq!(
        items
            .iter()
            .map(|(_, title, year)| (title.as_str(), *year))
            .collect::<Vec<_>>(),
        vec![("Dune", Some(1984)), ("Dune", Some(2021))]
    );
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
async fn a_renamed_file_carries_its_playback_progress_to_the_corrected_item(pool: sqlx::PgPool) {
    let fixture = Fixture::new("rename");
    let old_path = fixture.write("Dnue (2021)/Dnue (2021).mkv", b"the film");
    let library = fixture.library(&pool).await;
    fixture.scan(&pool, library.id).await;
    let old_item_id = add_progress(&pool, library.id, &old_path.to_string_lossy()).await;
    assert_eq!(
        movies(&pool, library.id)
            .await
            .into_iter()
            .map(|(_, title, _)| title)
            .collect::<Vec<_>>(),
        vec!["Dnue".to_string()]
    );

    // The user fixes the typo in the folder and the file: a move plus rename.
    let new_dir = fixture.media.join("Dune (2021)");
    fs::rename(old_path.parent().unwrap(), &new_dir).unwrap();
    let new_path = new_dir.join("Dune (2021).mkv");
    fs::rename(new_dir.join("Dnue (2021).mkv"), &new_path).unwrap();
    fixture.scan(&pool, library.id).await;

    assert_eq!(
        library_file_paths(&pool, library.id).await,
        vec![new_path.to_string_lossy().into_owned()]
    );
    let items = movies(&pool, library.id).await;
    assert_eq!(items.len(), 1);
    let (new_item_id, title, year) = &items[0];
    assert_eq!((title.as_str(), *year), ("Dune", Some(2021)));
    // The old entry is gone, as any missing path is; its watched state moved.
    assert_ne!(*new_item_id, old_item_id);
    assert_eq!(progress_count(&pool, old_item_id).await, 0);
    assert_eq!(progress_count(&pool, *new_item_id).await, 1);
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
async fn identical_files_renamed_together_are_not_followed(pool: sqlx::PgPool) {
    let fixture = Fixture::new("duplicates");
    let first = fixture.write("Alpha (2020)/Alpha (2020).mkv", b"same bytes");
    let second = fixture.write("Beta (2021)/Beta (2021).mkv", b"same bytes");
    let stamp = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    set_modified(&first, stamp);
    set_modified(&second, stamp);
    let library = fixture.library(&pool).await;
    fixture.scan(&pool, library.id).await;
    let old_ids: Vec<i64> = sqlx::query_scalar("select id from media_files where library_id = $1")
        .bind(library.id)
        .fetch_all(&pool)
        .await
        .unwrap();

    fs::rename(&first, first.with_file_name("Alpha Renamed (2020).mkv")).unwrap();
    fs::rename(&second, second.with_file_name("Beta Renamed (2021).mkv")).unwrap();
    fixture.scan(&pool, library.id).await;

    // Ambiguous signatures: both rows were replaced instead of followed.
    let remaining: i64 = sqlx::query_scalar(
        "select count(*) from media_files where library_id = $1 and id = any($2)",
    )
    .bind(library.id)
    .bind(&old_ids)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(remaining, 0);
    assert_eq!(library_file_paths(&pool, library.id).await.len(), 2);
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
async fn modification_times_are_backfilled_for_rows_without_one(pool: sqlx::PgPool) {
    let fixture = Fixture::new("backfill");
    let path = fixture.write("Gamma (2022)/Gamma (2022).mkv", b"the film");
    let library = fixture.library(&pool).await;
    fixture.scan(&pool, library.id).await;
    sqlx::query("update media_files set file_modified_at_ms = null where library_id = $1")
        .bind(library.id)
        .execute(&pool)
        .await
        .unwrap();

    // The unchanged file is skipped by the incremental plan but still backfilled.
    fixture.scan(&pool, library.id).await;
    let stored: Option<i64> =
        sqlx::query_scalar("select file_modified_at_ms from media_files where file_path = $1")
            .bind(path.to_string_lossy().into_owned())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(stored.is_some());

    // With the time known, a rename of the never re-analyzed file is followed.
    add_progress(&pool, library.id, &path.to_string_lossy()).await;
    fs::rename(
        &path,
        path.with_file_name("Gamma Director's Cut (2022).mkv"),
    )
    .unwrap();
    fixture.scan(&pool, library.id).await;
    let items = movies(&pool, library.id).await;
    assert_eq!(items.len(), 1);
    assert_eq!(progress_count(&pool, items[0].0).await, 1);
}
