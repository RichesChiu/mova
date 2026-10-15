//! Paths discovery skips by name leave the catalog on the next scan, even
//! while the files still exist.

use super::tests::{library_file_paths, run_scan};
use super::ExecuteScanJobOutcome;
use std::{fs, sync::Arc};

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
async fn catalogued_hidden_and_trailer_files_are_removed_on_the_next_scan(pool: sqlx::PgPool) {
    let root = std::env::temp_dir().join(format!("mova-ignore-{}", uuid::Uuid::new_v4()));
    let cache = root.join("cache");
    let media = root.join("media");
    let keep = media.join("Dune (2021)").join("Dune (2021).mkv");
    let apple_double = media.join("Dune (2021)").join("._Dune (2021).mkv");
    let trailer = media.join("Dune (2021)").join("Dune (2021)-trailer.mkv");
    let recycled = media
        .join("#recycle")
        .join("Old (2000)")
        .join("Old (2000).mkv");
    for path in [&keep, &apple_double, &trailer, &recycled] {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"video").unwrap();
    }
    fs::create_dir_all(&cache).unwrap();
    let library = mova_db::create_library(
        &pool,
        mova_db::CreateLibraryParams {
            name: "Movies".to_string(),
            description: None,
            metadata_language: "en-US".to_string(),
            root_path: media.to_string_lossy().into_owned(),
        },
    )
    .await
    .unwrap();

    // Rows an earlier version catalogued before the name rules existed.
    for (title, path) in [
        ("Dune", &apple_double),
        ("Dune trailer", &trailer),
        ("Old", &recycled),
    ] {
        let media_item_id: i64 = sqlx::query_scalar(
            "insert into media_items (library_id, media_type, title, source_title) values ($1, 'movie', $2, $2) returning id",
        )
        .bind(library.id)
        .bind(title)
        .fetch_one(&pool)
        .await
        .unwrap();
        sqlx::query(
            "insert into media_files (library_id, media_item_id, file_path, file_size) values ($1, $2, $3, 5)",
        )
        .bind(library.id)
        .bind(media_item_id)
        .bind(path.to_string_lossy().into_owned())
        .execute(&pool)
        .await
        .unwrap();
    }
    assert_eq!(library_file_paths(&pool, library.id).await.len(), 3);

    let outcome = run_scan(
        &pool,
        library.id,
        &cache,
        Arc::new(crate::HostLibraryStorageEnvironment),
    )
    .await
    .unwrap();
    let ExecuteScanJobOutcome::Completed(job) = outcome else {
        panic!("the scan must complete: {outcome:?}");
    };
    assert_eq!(job.status, "success");
    assert_eq!(
        library_file_paths(&pool, library.id).await,
        vec![keep.to_string_lossy().into_owned()]
    );
    assert!(apple_double.exists() && trailer.exists() && recycled.exists());

    let _ = fs::remove_dir_all(&root);
}
