//! Probe outcomes: a file ffprobe cannot analyze is still catalogued and
//! scraped, marked with its category, and never stops the scan.

use super::tests::{library_file_paths, run_scan};
use super::{build_media_entries, ExecuteScanJobOutcome};
use crate::ApplicationError;
use mova_domain::{MediaProbeIssue, METADATA_STATUS_MATCHED};
use std::{fs, path::PathBuf, process::Command, sync::Arc};

fn storage() -> Arc<crate::HostLibraryStorageEnvironment> {
    Arc::new(crate::HostLibraryStorageEnvironment)
}

#[test]
fn transient_probe_failures_leave_no_scan_hash() {
    let library = super::tests::build_library();
    let entries_for = |issue: Option<MediaProbeIssue>| {
        let mut file = super::tests::build_discovered_file();
        file.metadata_status = Some(METADATA_STATUS_MATCHED.to_string());
        file.probe_issue = issue;
        build_media_entries(&library, vec![file], false, false, None, false).unwrap()
    };

    assert!(entries_for(None)[0].scan_hash.is_some());
    assert!(entries_for(None)[0].probe_issue.is_none());
    for issue in [MediaProbeIssue::Unreadable, MediaProbeIssue::Timeout] {
        let entry = &entries_for(Some(issue))[0];
        assert!(entry.scan_hash.is_none(), "{issue} must be probed again");
        assert_eq!(entry.probe_issue.as_deref(), Some(issue.as_str()));
    }
    for issue in [
        MediaProbeIssue::Unrecognized,
        MediaProbeIssue::NoVideoStream,
    ] {
        let entry = &entries_for(Some(issue))[0];
        assert!(
            entry.scan_hash.is_some(),
            "{issue} waits for the file to change"
        );
        assert_eq!(entry.probe_issue.as_deref(), Some(issue.as_str()));
    }
}

#[test]
fn notification_names_the_probe_category_and_counts_partial_files() {
    let mut broken = super::tests::build_discovered_file();
    broken.metadata_status = Some(METADATA_STATUS_MATCHED.to_string());
    broken.probe_error = Some("EBML header parsing failed".to_string());
    broken.probe_issue = Some(MediaProbeIssue::Unrecognized);
    let mut partial = super::tests::build_discovered_file();
    partial.file_path = PathBuf::from("/media/series/Arcane/Arcane.S01E02.mkv");
    partial.metadata_status = Some(METADATA_STATUS_MATCHED.to_string());
    partial.width = None;
    partial.height = None;
    let mut complete = super::tests::build_discovered_file();
    complete.file_path = PathBuf::from("/media/series/Arcane/Arcane.S01E03.mkv");
    complete.metadata_status = Some(METADATA_STATUS_MATCHED.to_string());
    complete.width = Some(1920);
    complete.height = Some(1080);
    complete.duration_seconds = Some(100);
    complete.video_codec = Some("h264".to_string());
    let group = super::ScanDiscoveredGroup {
        presentation: super::ScanPresentationGroup {
            item_key: "series:arcane:2021".to_string(),
            media_type: "series".to_string(),
            title: "Arcane".to_string(),
            lookup_title: "Arcane".to_string(),
            year: Some(2021),
            season_air_year: None,
        },
        files: vec![broken, partial, complete],
        metadata_lookup_hint: None,
        metadata_binding_conflict: false,
    };

    let mut summary = mova_domain::ScanNotificationSummary::default();
    super::record_scan_notification_group(&mut summary, &group, None);

    assert_eq!(summary.probe_warning_count, 1);
    assert_eq!(summary.partial_probe_files, 1);
    assert_eq!(summary.matched_files, 3);
    assert_eq!(summary.issue_count, 1);
    let issue = &summary.issues[0];
    assert_eq!(
        issue.probe_warning_code.as_deref(),
        Some("probe_unrecognized")
    );
    assert_eq!(
        issue.probe_warning_file_path.as_deref(),
        Some("/media/series/Arcane/Arcane.S01E01.mkv")
    );
}

#[test]
fn a_rejected_group_is_reported_as_a_commit_failure() {
    let mut file = super::tests::build_discovered_file();
    file.metadata_status = Some(METADATA_STATUS_MATCHED.to_string());
    let group = super::ScanDiscoveredGroup {
        presentation: super::ScanPresentationGroup {
            item_key: "series:arcane:2021".to_string(),
            media_type: "series".to_string(),
            title: "Arcane".to_string(),
            lookup_title: "Arcane".to_string(),
            year: Some(2021),
            season_air_year: None,
        },
        files: vec![file],
        metadata_lookup_hint: None,
        metadata_binding_conflict: false,
    };

    let mut summary = mova_domain::ScanNotificationSummary::default();
    super::record_scan_group_commit_failure(&mut summary, &group, "value too long\n for type");

    assert_eq!(summary.failed_files, 1);
    assert_eq!(summary.issue_count, 1);
    assert_eq!(
        summary.issues[0].reason_code,
        super::SCAN_GROUP_COMMIT_FAILED
    );
    assert_eq!(
        summary.issues[0].diagnostic_message.as_deref(),
        Some("value too long for type")
    );
}

/// Only a value the database rejects is a group's own problem; everything
/// else must still fail the scan so it is retried.
#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
async fn only_data_errors_are_classified_as_group_problems(pool: sqlx::PgPool) {
    let library_id: i64 = sqlx::query_scalar(
        "insert into libraries (name, root_path) values ('TV', '/media/tv') returning id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let media_item_id: i64 = sqlx::query_scalar(
        "insert into media_items (library_id, media_type, title, source_title) values ($1, 'movie', 'A', 'A') returning id",
    )
    .bind(library_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let wrap = |error: sqlx::Error| {
        ApplicationError::Unexpected(anyhow::Error::from(error).context("failed to insert"))
    };

    let constraint = sqlx::query(
        "insert into media_files (library_id, media_item_id, file_path, file_size, width) values ($1, $2, '/media/tv/a.mkv', 1, 0)",
    )
    .bind(library_id)
    .bind(media_item_id)
    .execute(&pool)
    .await
    .unwrap_err();
    assert!(super::is_group_data_error(&wrap(constraint)));

    let too_long = sqlx::query(
        "insert into media_files (library_id, media_item_id, file_path, file_size, container) values ($1, $2, '/media/tv/b.mkv', 1, repeat('x', 65))",
    )
    .bind(library_id)
    .bind(media_item_id)
    .execute(&pool)
    .await
    .unwrap_err();
    assert!(super::is_group_data_error(&wrap(too_long)));

    let missing_table = sqlx::query("select 1 from no_such_table")
        .execute(&pool)
        .await
        .unwrap_err();
    assert!(!super::is_group_data_error(&wrap(missing_table)));
    assert!(!super::is_group_data_error(&ApplicationError::Unexpected(
        anyhow::anyhow!("connection reset")
    )));
    assert!(!super::is_group_data_error(&ApplicationError::Conflict(
        "x".to_string()
    )));
}

/// Real media written with ffmpeg. CI installs the shipped static tools and
/// sets `MOVA_TEST_REQUIRE_FFPROBE`, so a missing binary fails there; a local
/// checkout without the tools skips these end-to-end tests, and the parsing
/// and policy tests above still cover the logic.
fn media_tools_available() -> bool {
    let available = ["ffmpeg", "ffprobe"].iter().all(|tool| {
        Command::new(tool)
            .arg("-version")
            .output()
            .is_ok_and(|output| output.status.success())
    });
    if !available && std::env::var_os("MOVA_TEST_REQUIRE_FFPROBE").is_some() {
        panic!("MOVA_TEST_REQUIRE_FFPROBE is set but ffmpeg/ffprobe are not on PATH");
    }
    available
}

fn write_video(path: &std::path::Path, extra_args: &[&str]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let status = Command::new("ffmpeg")
        .args([
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=64x48:rate=5",
        ])
        .args(["-f", "lavfi", "-i", "sine=frequency=440:duration=1"])
        .args(["-t", "1", "-pix_fmt", "yuv420p", "-shortest"])
        .args(extra_args)
        .arg(path)
        .status()
        .unwrap();
    assert!(status.success(), "ffmpeg must write {}", path.display());
}

fn write_audio_only(path: &std::path::Path) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let status = Command::new("ffmpeg")
        .args([
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=1",
        ])
        .args(["-t", "1"])
        .arg(path)
        .status()
        .unwrap();
    assert!(status.success());
}

async fn probe_issue_of(pool: &sqlx::PgPool, file_path: &str) -> Option<String> {
    sqlx::query_scalar::<_, Option<String>>(
        "select probe_issue from media_files where file_path = $1",
    )
    .bind(file_path)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
async fn files_ffprobe_cannot_analyze_are_catalogued_and_marked(pool: sqlx::PgPool) {
    if !media_tools_available() {
        eprintln!("skipping: ffmpeg/ffprobe are not available");
        return;
    }
    let root = std::env::temp_dir().join(format!("mova-probe-{}", uuid::Uuid::new_v4()));
    let cache = root.join("cache");
    fs::create_dir_all(&cache).unwrap();
    let media = root.join("media");
    let good = media.join("Good (2020)").join("Good (2020).mkv");
    let broken = media.join("Broken (2021)").join("Broken (2021).mkv");
    let audio = media.join("Audio (2022)").join("Audio (2022).mkv");
    write_video(&good, &[]);
    fs::create_dir_all(broken.parent().unwrap()).unwrap();
    fs::write(&broken, b"garbage that is not a video").unwrap();
    write_audio_only(&audio);
    let library = mova_db::create_library(
        &pool,
        mova_db::CreateLibraryParams {
            name: "Probe".to_string(),
            description: None,
            metadata_language: "en-US".to_string(),
            root_path: media.to_string_lossy().into_owned(),
        },
    )
    .await
    .unwrap();

    let outcome = run_scan(&pool, library.id, &cache, storage())
        .await
        .unwrap();
    let ExecuteScanJobOutcome::Completed(job) = outcome else {
        panic!("the scan must complete: {outcome:?}");
    };
    assert_eq!(job.status, "success");
    let paths = library_file_paths(&pool, library.id).await;
    assert_eq!(paths.len(), 3, "every file is catalogued: {paths:?}");
    let good_path = good.to_string_lossy().into_owned();
    let broken_path = broken.to_string_lossy().into_owned();
    let audio_path = audio.to_string_lossy().into_owned();
    assert_eq!(probe_issue_of(&pool, &good_path).await, None);
    assert_eq!(
        probe_issue_of(&pool, &broken_path).await.as_deref(),
        Some("unrecognized")
    );
    assert_eq!(
        probe_issue_of(&pool, &audio_path).await.as_deref(),
        Some("no_video_stream")
    );
    let (width, hash): (Option<i32>, Option<String>) =
        sqlx::query_as("select width, scan_hash from media_files where file_path = $1")
            .bind(&good_path)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(width, Some(64));
    assert!(hash.is_some());
    let notification: (String, Option<String>) = sqlx::query_as(
        r#"
        select notification_type, payload->'issues'->0->>'probe_warning_code'
        from notifications where library_id = $1 order by id desc limit 1
        "#,
    )
    .bind(library.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(notification.0, "scan.completed_with_issues");
    assert!(notification
        .1
        .as_deref()
        .is_some_and(|code| code.starts_with("probe_")));

    // The broken file is replaced by a real video: the marker clears.
    write_video(&broken, &[]);
    let outcome = run_scan(&pool, library.id, &cache, storage())
        .await
        .unwrap();
    assert!(matches!(outcome, ExecuteScanJobOutcome::Completed(_)));
    assert_eq!(probe_issue_of(&pool, &broken_path).await, None);
    assert_eq!(
        probe_issue_of(&pool, &audio_path).await.as_deref(),
        Some("no_video_stream")
    );

    let _ = fs::remove_dir_all(&root);
}

/// One group's data is rejected by the database (an audio language tag longer
/// than its column); the scan reports it and finishes the other group.
#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
async fn a_group_the_database_rejects_does_not_stop_the_scan(pool: sqlx::PgPool) {
    if !media_tools_available() {
        eprintln!("skipping: ffmpeg/ffprobe are not available");
        return;
    }
    let root = std::env::temp_dir().join(format!("mova-reject-{}", uuid::Uuid::new_v4()));
    let cache = root.join("cache");
    fs::create_dir_all(&cache).unwrap();
    let media = root.join("media");
    let good = media.join("Good (2020)").join("Good (2020).mkv");
    let rejected = media.join("Rejected (2021)").join("Rejected (2021).mkv");
    write_video(&good, &[]);
    write_video(
        &rejected,
        &[
            "-metadata:s:a:0",
            "language=abcdefghijklmnopqrstuvwxyzabcdefghijklmnop",
        ],
    );
    let library = mova_db::create_library(
        &pool,
        mova_db::CreateLibraryParams {
            name: "Reject".to_string(),
            description: None,
            metadata_language: "en-US".to_string(),
            root_path: media.to_string_lossy().into_owned(),
        },
    )
    .await
    .unwrap();

    let outcome = run_scan(&pool, library.id, &cache, storage())
        .await
        .unwrap();
    let ExecuteScanJobOutcome::Completed(job) = outcome else {
        panic!("the scan must complete: {outcome:?}");
    };
    assert_eq!(job.status, "success");
    assert_eq!(
        library_file_paths(&pool, library.id).await,
        vec![good.to_string_lossy().into_owned()]
    );
    let notification: (String, i32, Option<String>) = sqlx::query_as(
        r#"
        select notification_type, (payload->>'failed_files')::int, payload->'issues'->0->>'reason_code'
        from notifications where library_id = $1 order by id desc limit 1
        "#,
    )
    .bind(library.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(notification.0, "scan.completed_with_issues");
    assert_eq!(notification.1, 1);
    assert_eq!(
        notification.2.as_deref(),
        Some(super::SCAN_GROUP_COMMIT_FAILED)
    );

    let _ = fs::remove_dir_all(&root);
}
