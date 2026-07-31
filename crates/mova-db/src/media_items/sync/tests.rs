use super::{
    advance_scan_group_progress, get_existing_library_media_file_by_path, local_artwork_path,
    patch_library_media_entries_remote_by_file_path, should_preserve_existing_parent,
    sync_library_media, sync_library_media_changes, upsert_library_media_entries_by_file_path,
    upsert_media_entry_with_policy, validate_authoritative_discovery, ScanGroupCommitStage,
};
use crate::{
    claim_background_job, create_library, delete_library, enqueue_scan_job, finalize_scan_job,
    get_scan_job, initialize_scan_job_work, mark_scan_group_analyzed, mark_scan_job_running,
    BackgroundJobFence, CreateAudioTrackParams, CreateLibraryParams, CreateMediaEntryParams,
    CreateScanJobParams, CreateSubtitleTrackParams, BACKGROUND_JOB_FENCE_LOST_MESSAGE,
};
use mova_domain::{
    ScanNotificationSummary, METADATA_FAILURE_PROVIDER_DISABLED, METADATA_FAILURE_PROVIDER_ERROR,
    METADATA_STATUS_FAILED, METADATA_STATUS_MATCHED, METADATA_STATUS_PENDING,
    METADATA_STATUS_UNMATCHED, REMOTE_MEDIA_TYPE_MOVIE, REMOTE_MEDIA_TYPE_SERIES,
};

fn build_movie_entry(library_id: i64, file_path: &str) -> CreateMediaEntryParams {
    CreateMediaEntryParams {
        library_id,
        media_type: "movie".to_string(),
        metadata_provider: Some("tmdb".to_string()),
        metadata_provider_item_id: Some("101".to_string()),
        metadata_status: METADATA_STATUS_MATCHED.to_string(),
        metadata_failure_reason: None,
        allow_artwork_clear: true,
        replace_remote_data: true,
        tmdb_remote_snapshot_json: None,
        tmdb_remote_snapshot_renews_retention: false,
        remote_media_type: Some(REMOTE_MEDIA_TYPE_MOVIE.to_string()),
        title: "A Writer's Odyssey".to_string(),
        source_title: "A Writer's Odyssey".to_string(),
        original_title: Some("刺杀小说家".to_string()),
        sort_title: None,
        year: Some(2025),
        external_ids: Vec::new(),
        ratings: Vec::new(),
        country: Some("China".to_string()),
        genres: Some("Fantasy · Adventure".to_string()),
        studio: Some("Huayi Brothers".to_string()),
        season_number: None,
        season_title: None,
        season_overview: None,
        season_poster_path: None,
        season_backdrop_path: None,
        episode_number: None,
        episode_title: None,
        overview: Some("A fantasy adventure.".to_string()),
        series_poster_path: None,
        series_backdrop_path: None,
        series_logo_path: None,
        poster_path: None,
        backdrop_path: None,
        logo_path: None,
        file_path: file_path.to_string(),
        container: Some("mkv".to_string()),
        file_size: 1,
        duration_seconds: Some(7800),
        video_title: None,
        video_codec: Some("hevc".to_string()),
        video_profile: Some("Main 10".to_string()),
        video_level: Some("5.1".to_string()),
        audio_codec: Some("eac3".to_string()),
        width: Some(3840),
        height: Some(2160),
        bitrate: Some(18_000_000),
        video_bitrate: Some(17_000_000),
        video_frame_rate: Some(23.976),
        video_aspect_ratio: Some("16:9".to_string()),
        video_scan_type: Some("progressive".to_string()),
        video_color_primaries: Some("bt2020".to_string()),
        video_color_space: Some("bt2020nc".to_string()),
        video_color_transfer: Some("smpte2084".to_string()),
        video_bit_depth: Some(10),
        video_pixel_format: Some("yuv420p10le".to_string()),
        video_reference_frames: Some(4),
        technical_tags: vec!["HDR10".to_string(), "Atmos".to_string()],
        audio_tracks: Vec::new(),
        subtitle_tracks: Vec::new(),
        local_analysis_version: 1,
        scan_hash: Some(format!("movie-{file_path}")),
    }
}

fn build_episode_entry(library_id: i64, file_path: &str) -> CreateMediaEntryParams {
    CreateMediaEntryParams {
        library_id,
        media_type: "episode".to_string(),
        metadata_provider: Some("tmdb".to_string()),
        metadata_provider_item_id: Some("202".to_string()),
        metadata_status: METADATA_STATUS_MATCHED.to_string(),
        metadata_failure_reason: None,
        allow_artwork_clear: true,
        replace_remote_data: true,
        tmdb_remote_snapshot_json: None,
        tmdb_remote_snapshot_renews_retention: false,
        remote_media_type: Some(REMOTE_MEDIA_TYPE_SERIES.to_string()),
        title: "Interstellar Classroom".to_string(),
        source_title: "Interstellar Classroom".to_string(),
        original_title: Some("Interstellar Classroom".to_string()),
        sort_title: None,
        year: Some(2024),
        external_ids: Vec::new(),
        ratings: Vec::new(),
        country: Some("Japan".to_string()),
        genres: Some("Animation · Sci-Fi".to_string()),
        studio: Some("Studio Trigger".to_string()),
        season_number: Some(1),
        season_title: Some("Season 01".to_string()),
        season_overview: None,
        season_poster_path: None,
        season_backdrop_path: None,
        episode_number: Some(1),
        episode_title: Some("Pilot".to_string()),
        overview: Some("Pilot episode".to_string()),
        series_poster_path: None,
        series_backdrop_path: None,
        series_logo_path: None,
        poster_path: None,
        backdrop_path: None,
        logo_path: None,
        file_path: file_path.to_string(),
        container: Some("mkv".to_string()),
        file_size: 1,
        duration_seconds: Some(1800),
        video_title: None,
        video_codec: Some("h264".to_string()),
        video_profile: None,
        video_level: None,
        audio_codec: Some("aac".to_string()),
        width: Some(1920),
        height: Some(1080),
        bitrate: Some(4_000_000),
        video_bitrate: Some(3_500_000),
        video_frame_rate: Some(23.976),
        video_aspect_ratio: Some("16:9".to_string()),
        video_scan_type: Some("progressive".to_string()),
        video_color_primaries: None,
        video_color_space: None,
        video_color_transfer: None,
        video_bit_depth: Some(8),
        video_pixel_format: Some("yuv420p".to_string()),
        video_reference_frames: None,
        technical_tags: Vec::new(),
        audio_tracks: Vec::new(),
        subtitle_tracks: Vec::new(),
        local_analysis_version: 1,
        scan_hash: Some(format!("episode-{file_path}")),
    }
}

async fn seed_running_scan(
    pool: &sqlx::postgres::PgPool,
    library_name: &str,
    root_path: &str,
    worker_id: &str,
) -> (i64, i64, BackgroundJobFence) {
    let library = create_library(
        pool,
        CreateLibraryParams {
            name: library_name.to_string(),
            description: None,
            metadata_language: "en-US".to_string(),
            root_path: root_path.to_string(),
        },
    )
    .await
    .unwrap();
    let scan_job = enqueue_scan_job(
        pool,
        CreateScanJobParams {
            library_id: library.id,
        },
    )
    .await
    .unwrap()
    .scan_job;
    let background_job = claim_background_job(pool, worker_id, 60)
        .await
        .unwrap()
        .claimed_job
        .unwrap();
    let fence = background_job.execution_fence().unwrap();
    mark_scan_job_running(pool, scan_job.id, &fence)
        .await
        .unwrap()
        .unwrap();

    (library.id, scan_job.id, fence)
}

#[test]
fn scan_group_parent_preservation_covers_local_and_transient_remote_stages() {
    let mut entry = build_movie_entry(1, "/media/movies/movie.mkv");

    assert!(should_preserve_existing_parent(
        ScanGroupCommitStage::Local,
        std::slice::from_ref(&entry),
    ));
    assert!(!should_preserve_existing_parent(
        ScanGroupCommitStage::Remote,
        std::slice::from_ref(&entry),
    ));

    entry.replace_remote_data = false;
    assert!(should_preserve_existing_parent(
        ScanGroupCommitStage::Remote,
        std::slice::from_ref(&entry),
    ));

    entry.replace_remote_data = true;
    entry.metadata_failure_reason = Some(METADATA_FAILURE_PROVIDER_ERROR.to_string());
    assert!(should_preserve_existing_parent(
        ScanGroupCommitStage::Remote,
        std::slice::from_ref(&entry),
    ));

    entry.metadata_failure_reason = Some(METADATA_FAILURE_PROVIDER_DISABLED.to_string());
    assert!(should_preserve_existing_parent(
        ScanGroupCommitStage::Remote,
        std::slice::from_ref(&entry),
    ));
}

#[test]
fn cached_artwork_promotion_requires_an_absolute_local_path() {
    assert_eq!(
        local_artwork_path(Some("/data/cache/poster.jpg")),
        Some("/data/cache/poster.jpg")
    );
    assert_eq!(local_artwork_path(Some("poster.jpg")), None);
    assert_eq!(
        local_artwork_path(Some("https://image.tmdb.org/t/p/poster.jpg")),
        None
    );
    assert_eq!(local_artwork_path(Some("   ")), None);
}

#[test]
fn authoritative_empty_discovery_is_only_valid_for_an_empty_library() {
    assert!(validate_authoritative_discovery(7, 0, 0).is_ok());
    assert!(validate_authoritative_discovery(7, 2, 1).is_ok());

    let error = validate_authoritative_discovery(7, 2, 0).unwrap_err();
    assert!(error
        .to_string()
        .contains("non-empty library 7: discovery returned zero media files"));
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
async fn authoritative_empty_discovery_preserves_existing_media(pool: sqlx::postgres::PgPool) {
    let library = create_library(
        &pool,
        CreateLibraryParams {
            name: "Movies".to_string(),
            description: None,
            metadata_language: "en-US".to_string(),
            root_path: "/media/movies".to_string(),
        },
    )
    .await
    .unwrap();
    let entry = build_movie_entry(library.id, "/media/movies/Movie/Movie.mkv");
    sync_library_media(&pool, library.id, std::slice::from_ref(&entry))
        .await
        .unwrap();
    let scan_job = enqueue_scan_job(
        &pool,
        CreateScanJobParams {
            library_id: library.id,
        },
    )
    .await
    .unwrap()
    .scan_job;
    let claimed_job = claim_background_job(&pool, "reconciliation-test-worker", 60)
        .await
        .unwrap()
        .claimed_job
        .unwrap();
    let fence = claimed_job.execution_fence().unwrap();
    mark_scan_job_running(&pool, scan_job.id, &fence)
        .await
        .unwrap()
        .unwrap();

    let error = sync_library_media_changes(&pool, library.id, scan_job.id, &[], &[], &fence)
        .await
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("discovery returned zero media files"));

    let persisted_path =
        sqlx::query_scalar::<_, String>("select file_path from media_files where library_id = $1")
            .bind(library.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let media_item_count =
        sqlx::query_scalar::<_, i64>("select count(*) from media_items where library_id = $1")
            .bind(library.id)
            .fetch_one(&pool)
            .await
            .unwrap();

    assert_eq!(persisted_path, entry.file_path);
    assert_eq!(media_item_count, 1);
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
async fn wrong_scan_fence_cannot_write_another_scan_or_library(pool: sqlx::postgres::PgPool) {
    let (library_a_id, scan_a_id, fence_a) =
        seed_running_scan(&pool, "Library A", "/media/library-a", "worker-a").await;
    let library_b = create_library(
        &pool,
        CreateLibraryParams {
            name: "Library B".to_string(),
            description: None,
            metadata_language: "en-US".to_string(),
            root_path: "/media/library-b".to_string(),
        },
    )
    .await
    .unwrap();
    let scan_b = enqueue_scan_job(
        &pool,
        CreateScanJobParams {
            library_id: library_b.id,
        },
    )
    .await
    .unwrap()
    .scan_job;
    let entry_b = build_movie_entry(library_b.id, "/media/library-b/movie-b.mkv");

    let wrong_scan_error = upsert_library_media_entries_by_file_path(
        &pool,
        scan_b.id,
        library_b.id,
        "wrong-scan",
        ScanGroupCommitStage::Local,
        std::slice::from_ref(&entry_b),
        &fence_a,
    )
    .await
    .unwrap_err();
    assert!(wrong_scan_error
        .to_string()
        .contains(BACKGROUND_JOB_FENCE_LOST_MESSAGE));

    let wrong_library_error = upsert_library_media_entries_by_file_path(
        &pool,
        scan_a_id,
        library_b.id,
        "wrong-library",
        ScanGroupCommitStage::Local,
        std::slice::from_ref(&entry_b),
        &fence_a,
    )
    .await
    .unwrap_err();
    assert!(wrong_library_error
        .to_string()
        .contains(BACKGROUND_JOB_FENCE_LOST_MESSAGE));

    let media_count =
        sqlx::query_scalar::<_, i64>("select count(*) from media_files where library_id = $1")
            .bind(library_b.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let checkpoint_count = sqlx::query_scalar::<_, i64>(
        "select count(*) from scan_job_groups where scan_job_id in ($1, $2)",
    )
    .bind(scan_a_id)
    .bind(scan_b.id)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_ne!(library_a_id, library_b.id);
    assert_eq!(media_count, 0);
    assert_eq!(checkpoint_count, 0);
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
async fn scan_group_upsert_promotes_legal_predecessors_monotonically(pool: sqlx::postgres::PgPool) {
    let (_, scan_job_id, _) =
        seed_running_scan(&pool, "Stages", "/media/stages", "stage-worker").await;
    sqlx::query(
        r#"
        update scan_jobs
        set total_files = 2,
            local_analyzed_files = 1
        where id = $1
        "#,
    )
    .bind(scan_job_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"
        insert into scan_job_groups (
            scan_job_id,
            group_key,
            file_count,
            local_analyzed,
            local_committed,
            remote_completed
        )
        values
            ($1, 'local-predecessor', 1, false, false, false),
            ($1, 'remote-predecessor', 1, true, false, false)
        "#,
    )
    .bind(scan_job_id)
    .execute(&pool)
    .await
    .unwrap();

    let mut tx = pool.begin().await.unwrap();
    advance_scan_group_progress(
        &mut tx,
        scan_job_id,
        "local-predecessor",
        1,
        ScanGroupCommitStage::Local,
    )
    .await
    .unwrap();
    advance_scan_group_progress(
        &mut tx,
        scan_job_id,
        "remote-predecessor",
        1,
        ScanGroupCommitStage::Remote,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let local_flags = sqlx::query_as::<_, (bool, bool, bool)>(
        r#"
        select local_analyzed, local_committed, remote_completed
        from scan_job_groups
        where scan_job_id = $1 and group_key = 'local-predecessor'
        "#,
    )
    .bind(scan_job_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let remote_flags = sqlx::query_as::<_, (bool, bool, bool)>(
        r#"
        select local_analyzed, local_committed, remote_completed
        from scan_job_groups
        where scan_job_id = $1 and group_key = 'remote-predecessor'
        "#,
    )
    .bind(scan_job_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let work_counters = sqlx::query_as::<_, (i32, i32, i32)>(
        r#"
        select
            local_analyzed_files,
            local_committed_files,
            remote_completed_files
        from scan_jobs
        where id = $1
        "#,
    )
    .bind(scan_job_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(local_flags, (true, true, false));
    assert_eq!(remote_flags, (true, true, true));
    assert_eq!(work_counters, (2, 2, 1));
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
async fn terminal_scan_rejects_late_group_and_media_write(pool: sqlx::postgres::PgPool) {
    let (library_id, scan_job_id, fence) =
        seed_running_scan(&pool, "Terminal", "/media/terminal", "terminal-worker").await;
    initialize_scan_job_work(&pool, scan_job_id, 1, 1, &fence)
        .await
        .unwrap()
        .unwrap();
    sqlx::query(
        r#"
        update scan_jobs
        set status = 'success',
            phase = 'finished',
            progress_percent = 100,
            finished_at = now()
        where id = $1
        "#,
    )
    .bind(scan_job_id)
    .execute(&pool)
    .await
    .unwrap();

    let entry = build_movie_entry(library_id, "/media/terminal/late.mkv");
    let error = upsert_library_media_entries_by_file_path(
        &pool,
        scan_job_id,
        library_id,
        "late-group",
        ScanGroupCommitStage::Local,
        std::slice::from_ref(&entry),
        &fence,
    )
    .await
    .unwrap_err();
    assert!(error
        .to_string()
        .contains(BACKGROUND_JOB_FENCE_LOST_MESSAGE));

    let media_count =
        sqlx::query_scalar::<_, i64>("select count(*) from media_files where library_id = $1")
            .bind(library_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let checkpoint_count =
        sqlx::query_scalar::<_, i64>("select count(*) from scan_job_groups where scan_job_id = $1")
            .bind(scan_job_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let scan_job = get_scan_job(&pool, scan_job_id).await.unwrap().unwrap();

    assert_eq!(media_count, 0);
    assert_eq!(checkpoint_count, 0);
    assert_eq!(scan_job.status, "success");
    assert_eq!(scan_job.phase.as_deref(), Some("finished"));
    assert_eq!(scan_job.progress_percent, 100);
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
async fn scan_and_background_terminal_states_commit_atomically(pool: sqlx::postgres::PgPool) {
    let (success_library_id, success_scan_job_id, success_fence) =
        seed_running_scan(&pool, "Success", "/media/success", "success-worker").await;
    let success_summary = ScanNotificationSummary {
        matched_files: 1,
        ..Default::default()
    };
    let success_scan = finalize_scan_job(
        &pool,
        success_library_id,
        success_scan_job_id,
        "success",
        1,
        1,
        None,
        Some(&success_summary),
        &success_fence,
    )
    .await
    .unwrap()
    .unwrap();
    let success_background = sqlx::query_as::<_, (String, bool, bool, bool)>(
        r#"
        select
            status,
            locked_by is null,
            lease_expires_at is null,
            finished_at is not null
        from background_jobs
        where id = $1
        "#,
    )
    .bind(success_fence.job_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let success_summary_available = sqlx::query_scalar::<_, bool>(
        r#"
        select (payload ->> 'summary_available')::boolean
        from notifications
        where source_key = $1
        "#,
    )
    .bind(format!("scan-job:{success_scan_job_id}"))
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(success_scan.status, "success");
    assert!(success_summary_available);
    assert_eq!(
        success_background,
        ("succeeded".to_string(), true, true, true)
    );

    let (cancel_library_id, cancel_scan_job_id, cancel_fence) =
        seed_running_scan(&pool, "Cancelled", "/media/cancelled", "cancel-worker").await;
    sqlx::query("update background_jobs set status = 'cancel_requested' where id = $1")
        .bind(cancel_fence.job_id)
        .execute(&pool)
        .await
        .unwrap();
    let cancelled_scan = finalize_scan_job(
        &pool,
        cancel_library_id,
        cancel_scan_job_id,
        "success",
        1,
        1,
        None,
        None,
        &cancel_fence,
    )
    .await
    .unwrap()
    .unwrap();
    let cancelled_background =
        sqlx::query_scalar::<_, String>("select status from background_jobs where id = $1")
            .bind(cancel_fence.job_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let cancelled_summary_available = sqlx::query_scalar::<_, bool>(
        r#"
        select (payload ->> 'summary_available')::boolean
        from notifications
        where source_key = $1
        "#,
    )
    .bind(format!("scan-job:{cancel_scan_job_id}"))
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(cancelled_scan.status, "cancelled");
    assert_eq!(cancelled_background, "cancelled");
    assert!(!cancelled_summary_available);
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
async fn deleted_library_scan_can_complete_cancellation_after_scan_cascade(
    pool: sqlx::postgres::PgPool,
) {
    let (library_id, scan_job_id, fence) =
        seed_running_scan(&pool, "Deleted", "/media/deleted", "deleted-worker").await;
    delete_library(&pool, library_id).await.unwrap().unwrap();

    let finalized_scan = finalize_scan_job(
        &pool,
        library_id,
        scan_job_id,
        "cancelled",
        0,
        0,
        Some("scan cancelled"),
        None,
        &fence,
    )
    .await
    .unwrap();
    let background_status =
        sqlx::query_scalar::<_, String>("select status from background_jobs where id = $1")
            .bind(fence.job_id)
            .fetch_one(&pool)
            .await
            .unwrap();

    assert!(finalized_scan.is_none());
    assert_eq!(background_status, "cancelled");
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
async fn remote_patch_preserves_local_file_and_track_state(pool: sqlx::postgres::PgPool) {
    let (library_id, scan_job_id, fence) = seed_running_scan(
        &pool,
        "Remote patch",
        "/media/remote-patch",
        "remote-worker",
    )
    .await;
    initialize_scan_job_work(&pool, scan_job_id, 1, 1, &fence)
        .await
        .unwrap()
        .unwrap();
    mark_scan_group_analyzed(&pool, scan_job_id, "movie", 1, &fence)
        .await
        .unwrap()
        .unwrap();

    let file_path = "/media/remote-patch/movie.mkv";
    let mut local = build_movie_entry(library_id, file_path);
    local.metadata_provider = None;
    local.metadata_provider_item_id = None;
    local.metadata_status = METADATA_STATUS_PENDING.to_string();
    local.replace_remote_data = false;
    local.title = "Local NFO title".to_string();
    local.source_title = "Local source title".to_string();
    local.sort_title = Some("Local sort title".to_string());
    local.file_size = 777;
    local.video_codec = Some("local-hevc".to_string());
    local.audio_tracks = vec![CreateAudioTrackParams {
        stream_index: 1,
        language: Some("en".to_string()),
        audio_codec: Some("truehd".to_string()),
        label: Some("Main".to_string()),
        channel_layout: Some("7.1".to_string()),
        channels: Some(8),
        bitrate: Some(4_000_000),
        sample_rate: Some(48_000),
        is_default: true,
    }];
    local.subtitle_tracks = vec![CreateSubtitleTrackParams {
        source_kind: "external".to_string(),
        file_path: Some("/media/remote-patch/movie.en.srt".to_string()),
        stream_index: None,
        language: Some("en".to_string()),
        subtitle_format: "srt".to_string(),
        label: None,
        is_default: false,
        is_forced: false,
        is_hearing_impaired: false,
    }];
    upsert_library_media_entries_by_file_path(
        &pool,
        scan_job_id,
        library_id,
        "movie",
        ScanGroupCommitStage::Local,
        std::slice::from_ref(&local),
        &fence,
    )
    .await
    .unwrap();

    let mut remote = local.clone();
    remote.metadata_provider = Some("tmdb".to_string());
    remote.metadata_provider_item_id = Some("987".to_string());
    remote.metadata_status = METADATA_STATUS_MATCHED.to_string();
    remote.remote_media_type = Some(REMOTE_MEDIA_TYPE_MOVIE.to_string());
    remote.replace_remote_data = true;
    remote.title = "Remote display title".to_string();
    remote.original_title = Some("Remote original title".to_string());
    remote.source_title = "must not replace local source".to_string();
    remote.sort_title = Some("must not replace local sort".to_string());
    remote.file_size = 999_999;
    remote.video_codec = Some("must-not-replace".to_string());
    remote.audio_tracks.clear();
    remote.subtitle_tracks.clear();

    let patched = patch_library_media_entries_remote_by_file_path(
        &pool,
        scan_job_id,
        library_id,
        "movie",
        std::slice::from_ref(&remote),
        &fence,
    )
    .await
    .unwrap();

    let state = sqlx::query_as::<
        _,
        (
            String,
            String,
            Option<String>,
            Option<String>,
            i64,
            Option<String>,
            i64,
            i64,
            bool,
        ),
    >(
        r#"
        select
            mi.title,
            mi.source_title,
            mi.sort_title,
            mi.metadata_provider_item_id,
            mf.file_size,
            mf.video_codec,
            (select count(*) from audio_tracks where media_file_id = mf.id),
            (select count(*) from subtitle_files where media_file_id = mf.id),
            sjg.remote_completed
        from media_files mf
        join media_items mi on mi.id = mf.media_item_id
        join scan_job_groups sjg
          on sjg.scan_job_id = $2
         and sjg.group_key = 'movie'
        where mf.library_id = $1
          and mf.file_path = $3
        "#,
    )
    .bind(library_id)
    .bind(scan_job_id)
    .bind(file_path)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(patched, 1);
    assert_eq!(state.0, "Remote display title");
    assert_eq!(state.1, "Local source title");
    assert_eq!(state.2.as_deref(), Some("Local sort title"));
    assert_eq!(state.3.as_deref(), Some("987"));
    assert_eq!(state.4, 777);
    assert_eq!(state.5.as_deref(), Some("local-hevc"));
    assert_eq!(state.6, 1);
    assert_eq!(state.7, 1);
    assert!(state.8);
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
async fn remote_patch_uses_committed_parent_without_reinterpreting_episode_coordinates(
    pool: sqlx::postgres::PgPool,
) {
    let (library_id, scan_job_id, fence) =
        seed_running_scan(&pool, "Episodes", "/media/episodes", "episode-worker").await;
    initialize_scan_job_work(&pool, scan_job_id, 1, 1, &fence)
        .await
        .unwrap()
        .unwrap();
    mark_scan_group_analyzed(&pool, scan_job_id, "series", 1, &fence)
        .await
        .unwrap()
        .unwrap();

    let file_path = "/media/episodes/show.S01E01.mkv";
    let mut local = build_episode_entry(library_id, file_path);
    local.metadata_provider = None;
    local.metadata_provider_item_id = None;
    local.metadata_status = METADATA_STATUS_PENDING.to_string();
    local.replace_remote_data = false;
    local.year = None;
    local.audio_tracks = vec![CreateAudioTrackParams {
        stream_index: 1,
        language: Some("ja".to_string()),
        audio_codec: Some("aac".to_string()),
        label: None,
        channel_layout: Some("stereo".to_string()),
        channels: Some(2),
        bitrate: Some(192_000),
        sample_rate: Some(48_000),
        is_default: true,
    }];
    upsert_library_media_entries_by_file_path(
        &pool,
        scan_job_id,
        library_id,
        "series",
        ScanGroupCommitStage::Local,
        std::slice::from_ref(&local),
        &fence,
    )
    .await
    .unwrap();

    let stored_series_id = sqlx::query_scalar::<_, i64>(
        r#"
        select s.series_id
        from media_files mf
        join episodes e on e.media_item_id = mf.media_item_id
        join seasons s on s.id = e.season_id
        where mf.library_id = $1
          and mf.file_path = $2
        "#,
    )
    .bind(library_id)
    .bind(file_path)
    .fetch_one(&pool)
    .await
    .unwrap();

    let mut invalid_remote = local.clone();
    invalid_remote.metadata_provider = Some("tmdb".to_string());
    invalid_remote.metadata_provider_item_id = Some("202".to_string());
    invalid_remote.metadata_status = METADATA_STATUS_MATCHED.to_string();
    invalid_remote.remote_media_type = Some(REMOTE_MEDIA_TYPE_SERIES.to_string());
    invalid_remote.replace_remote_data = true;
    invalid_remote.year = Some(2025);
    invalid_remote.season_number = Some(2);
    invalid_remote.episode_number = Some(8);
    let error = patch_library_media_entries_remote_by_file_path(
        &pool,
        scan_job_id,
        library_id,
        "series",
        std::slice::from_ref(&invalid_remote),
        &fence,
    )
    .await
    .unwrap_err();
    assert!(error
        .to_string()
        .contains("remote metadata cannot change local episode coordinates"));

    let mut remote = invalid_remote;
    remote.season_number = Some(1);
    remote.episode_number = Some(1);
    remote.episode_title = Some("Remote Pilot".to_string());
    patch_library_media_entries_remote_by_file_path(
        &pool,
        scan_job_id,
        library_id,
        "series",
        std::slice::from_ref(&remote),
        &fence,
    )
    .await
    .unwrap();

    let state = sqlx::query_as::<
        _,
        (
            i64,
            i32,
            i32,
            Option<i32>,
            Option<String>,
            Option<String>,
            i64,
            bool,
        ),
    >(
        r#"
        select
            s.series_id,
            s.season_number,
            e.episode_number,
            series.year,
            series.metadata_provider,
            series.metadata_provider_item_id,
            (select count(*) from audio_tracks where media_file_id = mf.id),
            sjg.remote_completed
        from media_files mf
        join episodes e on e.media_item_id = mf.media_item_id
        join seasons s on s.id = e.season_id
        join media_items series on series.id = s.series_id
        join scan_job_groups sjg
          on sjg.scan_job_id = $2
         and sjg.group_key = 'series'
        where mf.library_id = $1
          and mf.file_path = $3
        "#,
    )
    .bind(library_id)
    .bind(scan_job_id)
    .bind(file_path)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        state,
        (
            stored_series_id,
            1,
            1,
            Some(2025),
            Some("tmdb".to_string()),
            Some("202".to_string()),
            1,
            true,
        )
    );
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
async fn remote_patch_updates_every_episode_without_replacing_local_file_state(
    pool: sqlx::postgres::PgPool,
) {
    let (library_id, scan_job_id, fence) = seed_running_scan(
        &pool,
        "Episode group",
        "/media/episode-group",
        "episode-group-worker",
    )
    .await;
    initialize_scan_job_work(&pool, scan_job_id, 2, 1, &fence)
        .await
        .unwrap()
        .unwrap();
    mark_scan_group_analyzed(&pool, scan_job_id, "series", 2, &fence)
        .await
        .unwrap()
        .unwrap();

    let mut first_local = build_episode_entry(library_id, "/media/episode-group/show.S01E01.mkv");
    first_local.metadata_provider = None;
    first_local.metadata_provider_item_id = None;
    first_local.metadata_status = METADATA_STATUS_PENDING.to_string();
    first_local.replace_remote_data = false;
    first_local.episode_title = Some("Local episode one".to_string());
    first_local.overview = Some("Local overview one".to_string());
    first_local.file_size = 101;
    first_local.video_codec = Some("local-h264".to_string());
    first_local.audio_tracks = vec![CreateAudioTrackParams {
        stream_index: 1,
        language: Some("en".to_string()),
        audio_codec: Some("aac".to_string()),
        label: None,
        channel_layout: Some("stereo".to_string()),
        channels: Some(2),
        bitrate: Some(192_000),
        sample_rate: Some(48_000),
        is_default: true,
    }];
    first_local.subtitle_tracks = vec![CreateSubtitleTrackParams {
        source_kind: "external".to_string(),
        file_path: Some("/media/episode-group/show.S01E01.en.srt".to_string()),
        stream_index: None,
        language: Some("en".to_string()),
        subtitle_format: "srt".to_string(),
        label: None,
        is_default: false,
        is_forced: false,
        is_hearing_impaired: false,
    }];

    let mut second_local = first_local.clone();
    second_local.file_path = "/media/episode-group/show.S01E02.mkv".to_string();
    second_local.episode_number = Some(2);
    second_local.episode_title = Some("Local episode two".to_string());
    second_local.overview = Some("Local overview two".to_string());
    second_local.file_size = 202;
    second_local.subtitle_tracks[0].file_path =
        Some("/media/episode-group/show.S01E02.en.srt".to_string());

    upsert_library_media_entries_by_file_path(
        &pool,
        scan_job_id,
        library_id,
        "series",
        ScanGroupCommitStage::Local,
        &[first_local.clone(), second_local.clone()],
        &fence,
    )
    .await
    .unwrap();

    let mut first_remote = first_local;
    first_remote.metadata_provider = Some("tmdb".to_string());
    first_remote.metadata_provider_item_id = Some("202".to_string());
    first_remote.metadata_status = METADATA_STATUS_MATCHED.to_string();
    first_remote.remote_media_type = Some(REMOTE_MEDIA_TYPE_SERIES.to_string());
    first_remote.replace_remote_data = true;
    first_remote.episode_title = Some("Remote episode one".to_string());
    first_remote.overview = Some("Remote overview one".to_string());
    first_remote.poster_path = Some("/cache/episode-one.jpg".to_string());
    first_remote.file_size = 9_001;
    first_remote.video_codec = Some("must-not-replace".to_string());
    first_remote.audio_tracks.clear();
    first_remote.subtitle_tracks.clear();

    let mut second_remote = second_local;
    second_remote.metadata_provider = Some("tmdb".to_string());
    second_remote.metadata_provider_item_id = Some("202".to_string());
    second_remote.metadata_status = METADATA_STATUS_MATCHED.to_string();
    second_remote.remote_media_type = Some(REMOTE_MEDIA_TYPE_SERIES.to_string());
    // Only the first entry owns the one-time replacement of shared series
    // external IDs and ratings. Episode-owned fields must still be applied.
    second_remote.replace_remote_data = false;
    second_remote.episode_title = Some("Remote episode two".to_string());
    second_remote.overview = Some("Remote overview two".to_string());
    second_remote.poster_path = Some("/cache/episode-two.jpg".to_string());
    second_remote.file_size = 9_002;
    second_remote.video_codec = Some("must-not-replace".to_string());
    second_remote.audio_tracks.clear();
    second_remote.subtitle_tracks.clear();

    patch_library_media_entries_remote_by_file_path(
        &pool,
        scan_job_id,
        library_id,
        "series",
        &[first_remote, second_remote],
        &fence,
    )
    .await
    .unwrap();

    let rows = sqlx::query_as::<
        _,
        (
            i32,
            String,
            Option<String>,
            Option<String>,
            i64,
            Option<String>,
            i64,
            i64,
        ),
    >(
        r#"
        select
            e.episode_number,
            mi.title,
            mi.overview,
            mi.poster_path,
            mf.file_size,
            mf.video_codec,
            (select count(*) from audio_tracks where media_file_id = mf.id),
            (select count(*) from subtitle_files where media_file_id = mf.id)
        from media_files mf
        join media_items mi on mi.id = mf.media_item_id
        join episodes e on e.media_item_id = mi.id
        where mf.library_id = $1
        order by e.episode_number
        "#,
    )
    .bind(library_id)
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(
        rows,
        vec![
            (
                1,
                "Remote episode one".to_string(),
                Some("Remote overview one".to_string()),
                Some("/cache/episode-one.jpg".to_string()),
                101,
                Some("local-h264".to_string()),
                1,
                1,
            ),
            (
                2,
                "Remote episode two".to_string(),
                Some("Remote overview two".to_string()),
                Some("/cache/episode-two.jpg".to_string()),
                202,
                Some("local-h264".to_string()),
                1,
                1,
            ),
        ]
    );
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
async fn scan_group_file_count_is_immutable_across_retries_and_stages(
    pool: sqlx::postgres::PgPool,
) {
    let (library_id, scan_job_id, fence) =
        seed_running_scan(&pool, "Counts", "/media/counts", "count-worker").await;
    initialize_scan_job_work(&pool, scan_job_id, 2, 2, &fence)
        .await
        .unwrap()
        .unwrap();
    mark_scan_group_analyzed(&pool, scan_job_id, "versions", 1, &fence)
        .await
        .unwrap()
        .unwrap();

    let entry_a = build_movie_entry(library_id, "/media/counts/movie-a.mkv");
    let entry_b = build_movie_entry(library_id, "/media/counts/movie-b.mkv");
    upsert_library_media_entries_by_file_path(
        &pool,
        scan_job_id,
        library_id,
        "versions",
        ScanGroupCommitStage::Local,
        std::slice::from_ref(&entry_a),
        &fence,
    )
    .await
    .unwrap();
    upsert_library_media_entries_by_file_path(
        &pool,
        scan_job_id,
        library_id,
        "versions",
        ScanGroupCommitStage::Local,
        std::slice::from_ref(&entry_a),
        &fence,
    )
    .await
    .unwrap();

    let changed_analysis_error =
        mark_scan_group_analyzed(&pool, scan_job_id, "versions", 2, &fence)
            .await
            .unwrap_err();
    assert!(changed_analysis_error
        .to_string()
        .contains("file count changed from 1 to 2"));

    let entries = vec![entry_a, entry_b];
    let changed_local_error = upsert_library_media_entries_by_file_path(
        &pool,
        scan_job_id,
        library_id,
        "versions",
        ScanGroupCommitStage::Local,
        &entries,
        &fence,
    )
    .await
    .unwrap_err();
    assert!(changed_local_error
        .to_string()
        .contains("file count changed from 1 to 2"));
    let changed_remote_error = upsert_library_media_entries_by_file_path(
        &pool,
        scan_job_id,
        library_id,
        "versions",
        ScanGroupCommitStage::Remote,
        &entries,
        &fence,
    )
    .await
    .unwrap_err();
    assert!(changed_remote_error
        .to_string()
        .contains("file count changed from 1 to 2"));

    let checkpoint = sqlx::query_as::<_, (i32, bool, bool, bool)>(
        r#"
        select file_count, local_analyzed, local_committed, remote_completed
        from scan_job_groups
        where scan_job_id = $1 and group_key = 'versions'
        "#,
    )
    .bind(scan_job_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let counters = sqlx::query_as::<_, (i32, i32, i32)>(
        r#"
        select local_analyzed_files, local_committed_files, remote_completed_files
        from scan_jobs
        where id = $1
        "#,
    )
    .bind(scan_job_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let media_count =
        sqlx::query_scalar::<_, i64>("select count(*) from media_files where library_id = $1")
            .bind(library_id)
            .fetch_one(&pool)
            .await
            .unwrap();

    assert_eq!(checkpoint, (1, true, true, false));
    assert_eq!(counters, (1, 1, 0));
    assert_eq!(media_count, 1);
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
async fn non_authoritative_remote_commit_promotes_cached_movie_and_series_artwork(
    pool: sqlx::postgres::PgPool,
) {
    let library = create_library(
        &pool,
        CreateLibraryParams {
            name: "Mixed".to_string(),
            description: None,
            metadata_language: "en-US".to_string(),
            root_path: "/media".to_string(),
        },
    )
    .await
    .unwrap();

    let mut movie = build_movie_entry(library.id, "/media/movies/Movie.mkv");
    movie.title = "Trusted Movie".to_string();
    movie.poster_path = Some("https://image.tmdb.org/t/p/original/movie-poster.jpg".to_string());
    movie.backdrop_path =
        Some("https://image.tmdb.org/t/p/original/movie-backdrop.jpg".to_string());
    movie.logo_path = Some("https://image.tmdb.org/t/p/original/movie-logo.png".to_string());

    let mut episode = build_episode_entry(library.id, "/media/series/Show/Show.S01E01.mkv");
    episode.title = "Trusted Series".to_string();
    episode.episode_title = Some("Trusted Episode".to_string());
    episode.series_poster_path =
        Some("https://image.tmdb.org/t/p/original/series-poster.jpg".to_string());
    episode.series_backdrop_path =
        Some("https://image.tmdb.org/t/p/original/series-backdrop.jpg".to_string());
    episode.series_logo_path =
        Some("https://image.tmdb.org/t/p/original/series-logo.png".to_string());
    episode.season_poster_path =
        Some("https://image.tmdb.org/t/p/original/season-poster.jpg".to_string());
    episode.season_backdrop_path =
        Some("https://image.tmdb.org/t/p/original/season-backdrop.jpg".to_string());
    episode.poster_path =
        Some("https://image.tmdb.org/t/p/original/episode-poster.jpg".to_string());
    episode.backdrop_path =
        Some("https://image.tmdb.org/t/p/original/episode-backdrop.jpg".to_string());
    episode.logo_path = Some("https://image.tmdb.org/t/p/original/episode-logo.png".to_string());

    sync_library_media(&pool, library.id, &[movie.clone(), episode.clone()])
        .await
        .unwrap();

    let mut cached_movie = movie;
    cached_movie.title = "Untrusted Local Movie".to_string();
    cached_movie.replace_remote_data = false;
    cached_movie.allow_artwork_clear = false;
    cached_movie.poster_path = Some("/data/cache/movie-poster.jpg".to_string());
    cached_movie.backdrop_path = Some("/data/cache/movie-backdrop.jpg".to_string());
    cached_movie.logo_path = Some("/data/cache/movie-logo.png".to_string());
    let mut tx = pool.begin().await.unwrap();
    let existing =
        get_existing_library_media_file_by_path(&mut tx, library.id, &cached_movie.file_path)
            .await
            .unwrap();
    upsert_media_entry_with_policy(&mut tx, &cached_movie, existing, true)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    let mut cached_episode = episode;
    cached_episode.title = "Untrusted Local Series".to_string();
    cached_episode.episode_title = Some("Untrusted Local Episode".to_string());
    cached_episode.replace_remote_data = false;
    cached_episode.allow_artwork_clear = false;
    cached_episode.series_poster_path = Some("/data/cache/series-poster.jpg".to_string());
    cached_episode.series_backdrop_path = Some("/data/cache/series-backdrop.jpg".to_string());
    cached_episode.series_logo_path = Some("/data/cache/series-logo.png".to_string());
    cached_episode.season_poster_path = Some("/data/cache/season-poster.jpg".to_string());
    cached_episode.season_backdrop_path = Some("/data/cache/season-backdrop.jpg".to_string());
    cached_episode.poster_path = Some("/data/cache/episode-poster.jpg".to_string());
    cached_episode.backdrop_path = Some("/data/cache/episode-backdrop.jpg".to_string());
    cached_episode.logo_path = Some("/data/cache/episode-logo.png".to_string());
    let mut tx = pool.begin().await.unwrap();
    let existing =
        get_existing_library_media_file_by_path(&mut tx, library.id, &cached_episode.file_path)
            .await
            .unwrap();
    upsert_media_entry_with_policy(&mut tx, &cached_episode, existing, true)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    let movie_row = sqlx::query_as::<_, (String, String, String, String)>(
        r#"
        select title, poster_path, backdrop_path, logo_path
        from media_items
        where library_id = $1 and media_type = 'movie'
        "#,
    )
    .bind(library.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(movie_row.0, "Trusted Movie");
    assert_eq!(movie_row.1, "/data/cache/movie-poster.jpg");
    assert_eq!(movie_row.2, "/data/cache/movie-backdrop.jpg");
    assert_eq!(movie_row.3, "/data/cache/movie-logo.png");

    let series_row = sqlx::query_as::<_, (String, String, String, String)>(
        r#"
        select title, poster_path, backdrop_path, logo_path
        from media_items
        where library_id = $1 and media_type = 'series'
        "#,
    )
    .bind(library.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(series_row.0, "Trusted Series");
    assert_eq!(series_row.1, "/data/cache/series-poster.jpg");
    assert_eq!(series_row.2, "/data/cache/series-backdrop.jpg");
    assert_eq!(series_row.3, "/data/cache/series-logo.png");

    let season_row =
        sqlx::query_as::<_, (String, String)>("select poster_path, backdrop_path from seasons")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(season_row.0, "/data/cache/season-poster.jpg");
    assert_eq!(season_row.1, "/data/cache/season-backdrop.jpg");

    let episode_row = sqlx::query_as::<_, (String, String, String, String)>(
        r#"
        select mi.title, mi.poster_path, mi.backdrop_path, mi.logo_path
        from media_items mi
        join episodes e on e.media_item_id = mi.id
        "#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(episode_row.0, "Trusted Episode");
    assert_eq!(episode_row.1, "/data/cache/episode-poster.jpg");
    assert_eq!(episode_row.2, "/data/cache/episode-backdrop.jpg");
    assert_eq!(episode_row.3, "/data/cache/episode-logo.png");
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
async fn local_and_provider_error_movie_writes_preserve_existing_parent(
    pool: sqlx::postgres::PgPool,
) {
    let library = create_library(
        &pool,
        CreateLibraryParams {
            name: "Movies".to_string(),
            description: None,
            metadata_language: "en-US".to_string(),
            root_path: "/media/movies".to_string(),
        },
    )
    .await
    .unwrap();

    let mut trusted = build_movie_entry(library.id, "/media/movies/Trusted/Trusted.1080p.mkv");
    trusted.title = "Trusted Remote Title".to_string();
    trusted.poster_path = Some("/cache/trusted-poster.jpg".to_string());
    sync_library_media(&pool, library.id, std::slice::from_ref(&trusted))
        .await
        .unwrap();

    let mut new_version = build_movie_entry(library.id, "/media/movies/Trusted/Trusted.2160p.mkv");
    new_version.metadata_provider = None;
    new_version.metadata_provider_item_id = None;
    new_version.metadata_status = METADATA_STATUS_PENDING.to_string();
    new_version.metadata_failure_reason = None;
    new_version.allow_artwork_clear = false;
    new_version.replace_remote_data = false;
    new_version.title = "Local Pending Title".to_string();
    new_version.poster_path = None;

    let mut tx = pool.begin().await.unwrap();
    let existing =
        get_existing_library_media_file_by_path(&mut tx, library.id, &new_version.file_path)
            .await
            .unwrap();
    upsert_media_entry_with_policy(&mut tx, &new_version, existing, true)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    new_version.metadata_provider = Some("tmdb".to_string());
    new_version.metadata_provider_item_id = Some("101".to_string());
    new_version.remote_media_type = Some(REMOTE_MEDIA_TYPE_MOVIE.to_string());
    new_version.metadata_status = METADATA_STATUS_FAILED.to_string();
    new_version.metadata_failure_reason = Some(METADATA_FAILURE_PROVIDER_ERROR.to_string());
    let mut tx = pool.begin().await.unwrap();
    let existing =
        get_existing_library_media_file_by_path(&mut tx, library.id, &new_version.file_path)
            .await
            .unwrap();
    upsert_media_entry_with_policy(&mut tx, &new_version, existing, true)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    let parent = sqlx::query_as::<
        _,
        (
            Option<String>,
            Option<String>,
            String,
            Option<String>,
            String,
            Option<String>,
        ),
    >(
        r#"
        select metadata_provider, metadata_provider_item_id, metadata_status,
               metadata_failure_reason, title, poster_path
        from media_items
        where library_id = $1 and media_type = 'movie'
        "#,
    )
    .bind(library.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let file_count =
        sqlx::query_scalar::<_, i64>("select count(*) from media_files where library_id = $1")
            .bind(library.id)
            .fetch_one(&pool)
            .await
            .unwrap();

    assert_eq!(parent.0.as_deref(), Some("tmdb"));
    assert_eq!(parent.1.as_deref(), Some("101"));
    assert_eq!(parent.2, METADATA_STATUS_MATCHED);
    assert_eq!(parent.3.as_deref(), Some(METADATA_FAILURE_PROVIDER_ERROR));
    assert_eq!(parent.4, "Trusted Remote Title");
    assert_eq!(parent.5.as_deref(), Some("/cache/trusted-poster.jpg"));
    assert_eq!(file_count, 2);
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
async fn provider_error_keeps_new_local_fields_for_an_unbound_movie(pool: sqlx::postgres::PgPool) {
    let library = create_library(
        &pool,
        CreateLibraryParams {
            name: "Movies".to_string(),
            description: None,
            metadata_language: "en-US".to_string(),
            root_path: "/media/movies".to_string(),
        },
    )
    .await
    .unwrap();
    let file_path = "/media/movies/Local/Local.mkv";
    let mut initial = build_movie_entry(library.id, file_path);
    initial.metadata_provider = None;
    initial.metadata_provider_item_id = None;
    initial.metadata_status = METADATA_STATUS_UNMATCHED.to_string();
    initial.replace_remote_data = false;
    initial.title = "Old Local Title".to_string();
    initial.overview = Some("Old local overview".to_string());
    initial.poster_path = Some("/media/movies/Local/old-poster.jpg".to_string());
    sync_library_media(&pool, library.id, std::slice::from_ref(&initial))
        .await
        .unwrap();

    let mut refreshed = initial.clone();
    refreshed.metadata_status = METADATA_STATUS_PENDING.to_string();
    refreshed.title = "New NFO Title".to_string();
    refreshed.overview = Some("New NFO overview".to_string());
    refreshed.poster_path = Some("/media/movies/Local/new-poster.jpg".to_string());
    let mut tx = pool.begin().await.unwrap();
    let existing = get_existing_library_media_file_by_path(&mut tx, library.id, file_path)
        .await
        .unwrap();
    upsert_media_entry_with_policy(&mut tx, &refreshed, existing, true)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    refreshed.metadata_status = METADATA_STATUS_FAILED.to_string();
    refreshed.metadata_failure_reason = Some(METADATA_FAILURE_PROVIDER_ERROR.to_string());
    let mut tx = pool.begin().await.unwrap();
    let existing = get_existing_library_media_file_by_path(&mut tx, library.id, file_path)
        .await
        .unwrap();
    upsert_media_entry_with_policy(&mut tx, &refreshed, existing, true)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    let row = sqlx::query_as::<
        _,
        (
            Option<String>,
            Option<String>,
            String,
            Option<String>,
            String,
            Option<String>,
            Option<String>,
        ),
    >(
        r#"
        select metadata_provider, metadata_provider_item_id, metadata_status,
               metadata_failure_reason, title, overview, poster_path
        from media_items
        where library_id = $1 and media_type = 'movie'
        "#,
    )
    .bind(library.id)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(row.0, None);
    assert_eq!(row.1, None);
    assert_eq!(row.2, METADATA_STATUS_FAILED);
    assert_eq!(row.3.as_deref(), Some(METADATA_FAILURE_PROVIDER_ERROR));
    assert_eq!(row.4, "New NFO Title");
    assert_eq!(row.5.as_deref(), Some("New NFO overview"));
    assert_eq!(row.6.as_deref(), Some("/media/movies/Local/new-poster.jpg"));
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
async fn local_and_provider_error_episode_writes_preserve_existing_series(
    pool: sqlx::postgres::PgPool,
) {
    let library = create_library(
        &pool,
        CreateLibraryParams {
            name: "Series".to_string(),
            description: None,
            metadata_language: "en-US".to_string(),
            root_path: "/media/series".to_string(),
        },
    )
    .await
    .unwrap();

    let mut trusted = build_episode_entry(library.id, "/media/series/Show/Show.S01E01.mkv");
    trusted.title = "Trusted Series Title".to_string();
    trusted.series_poster_path = Some("/cache/trusted-series-poster.jpg".to_string());
    sync_library_media(&pool, library.id, std::slice::from_ref(&trusted))
        .await
        .unwrap();

    let mut new_episode = build_episode_entry(library.id, "/media/series/Show/Show.S01E02.mkv");
    new_episode.metadata_provider = None;
    new_episode.metadata_provider_item_id = None;
    new_episode.metadata_status = METADATA_STATUS_PENDING.to_string();
    new_episode.metadata_failure_reason = None;
    new_episode.allow_artwork_clear = false;
    new_episode.replace_remote_data = false;
    new_episode.title = "Local Series Title".to_string();
    new_episode.episode_number = Some(2);
    new_episode.episode_title = Some("Local Episode Two".to_string());
    new_episode.series_poster_path = None;

    let mut tx = pool.begin().await.unwrap();
    let existing =
        get_existing_library_media_file_by_path(&mut tx, library.id, &new_episode.file_path)
            .await
            .unwrap();
    upsert_media_entry_with_policy(&mut tx, &new_episode, existing, true)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    new_episode.metadata_provider = Some("tmdb".to_string());
    new_episode.metadata_provider_item_id = Some("202".to_string());
    new_episode.remote_media_type = Some(REMOTE_MEDIA_TYPE_SERIES.to_string());
    new_episode.metadata_status = METADATA_STATUS_FAILED.to_string();
    new_episode.metadata_failure_reason = Some(METADATA_FAILURE_PROVIDER_ERROR.to_string());
    let mut tx = pool.begin().await.unwrap();
    let existing =
        get_existing_library_media_file_by_path(&mut tx, library.id, &new_episode.file_path)
            .await
            .unwrap();
    upsert_media_entry_with_policy(&mut tx, &new_episode, existing, true)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    let series = sqlx::query_as::<
        _,
        (
            Option<String>,
            Option<String>,
            String,
            Option<String>,
            String,
            Option<String>,
        ),
    >(
        r#"
        select metadata_provider, metadata_provider_item_id, metadata_status,
               metadata_failure_reason, title, poster_path
        from media_items
        where library_id = $1 and media_type = 'series'
        "#,
    )
    .bind(library.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let new_episode_status = sqlx::query_scalar::<_, String>(
        r#"
        select mi.metadata_status
        from media_items mi
        join episodes e on e.media_item_id = mi.id
        where e.episode_number = 2
        "#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(series.0.as_deref(), Some("tmdb"));
    assert_eq!(series.1.as_deref(), Some("202"));
    assert_eq!(series.2, METADATA_STATUS_MATCHED);
    assert_eq!(series.3.as_deref(), Some(METADATA_FAILURE_PROVIDER_ERROR));
    assert_eq!(series.4, "Trusted Series Title");
    assert_eq!(
        series.5.as_deref(),
        Some("/cache/trusted-series-poster.jpg")
    );
    assert_eq!(new_episode_status, METADATA_STATUS_FAILED);
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
async fn provider_error_preserves_existing_episode_when_adding_a_second_version(
    pool: sqlx::postgres::PgPool,
) {
    let library = create_library(
        &pool,
        CreateLibraryParams {
            name: "Series".to_string(),
            description: None,
            metadata_language: "en-US".to_string(),
            root_path: "/media/series".to_string(),
        },
    )
    .await
    .unwrap();
    let mut trusted = build_episode_entry(library.id, "/media/series/Show/Show.S01E01.1080p.mkv");
    trusted.episode_title = Some("Trusted Episode Title".to_string());
    trusted.overview = Some("Trusted episode overview".to_string());
    trusted.poster_path = Some("/cache/trusted-episode-poster.jpg".to_string());
    sync_library_media(&pool, library.id, std::slice::from_ref(&trusted))
        .await
        .unwrap();

    let mut second_version =
        build_episode_entry(library.id, "/media/series/Show/Show.S01E01.2160p.mkv");
    second_version.metadata_provider = None;
    second_version.metadata_provider_item_id = None;
    second_version.metadata_status = METADATA_STATUS_PENDING.to_string();
    second_version.replace_remote_data = false;
    second_version.allow_artwork_clear = false;
    second_version.episode_title = Some("Local Episode Title".to_string());
    second_version.overview = Some("Local episode overview".to_string());
    second_version.poster_path = None;
    let mut tx = pool.begin().await.unwrap();
    let existing =
        get_existing_library_media_file_by_path(&mut tx, library.id, &second_version.file_path)
            .await
            .unwrap();
    upsert_media_entry_with_policy(&mut tx, &second_version, existing, true)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    second_version.metadata_provider = Some("tmdb".to_string());
    second_version.metadata_provider_item_id = Some("202".to_string());
    second_version.remote_media_type = Some(REMOTE_MEDIA_TYPE_SERIES.to_string());
    second_version.metadata_status = METADATA_STATUS_FAILED.to_string();
    second_version.metadata_failure_reason = Some(METADATA_FAILURE_PROVIDER_ERROR.to_string());
    let mut tx = pool.begin().await.unwrap();
    let existing =
        get_existing_library_media_file_by_path(&mut tx, library.id, &second_version.file_path)
            .await
            .unwrap();
    upsert_media_entry_with_policy(&mut tx, &second_version, existing, true)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    let episode = sqlx::query_as::<_, (String, Option<String>, Option<String>)>(
        r#"
        select mi.title, mi.overview, mi.poster_path
        from media_items mi
        join episodes e on e.media_item_id = mi.id
        where e.episode_number = 1
        "#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let file_count =
        sqlx::query_scalar::<_, i64>("select count(*) from media_files where library_id = $1")
            .bind(library.id)
            .fetch_one(&pool)
            .await
            .unwrap();

    assert_eq!(episode.0, "Trusted Episode Title");
    assert_eq!(episode.1.as_deref(), Some("Trusted episode overview"));
    assert_eq!(
        episode.2.as_deref(),
        Some("/cache/trusted-episode-poster.jpg")
    );
    assert_eq!(file_count, 2);
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
async fn provider_error_keeps_new_local_season_fields_for_an_unbound_series(
    pool: sqlx::postgres::PgPool,
) {
    let library = create_library(
        &pool,
        CreateLibraryParams {
            name: "Series".to_string(),
            description: None,
            metadata_language: "en-US".to_string(),
            root_path: "/media/series".to_string(),
        },
    )
    .await
    .unwrap();
    let file_path = "/media/series/Local/Local.S01E01.mkv";
    let mut initial = build_episode_entry(library.id, file_path);
    initial.metadata_provider = None;
    initial.metadata_provider_item_id = None;
    initial.metadata_status = METADATA_STATUS_UNMATCHED.to_string();
    initial.replace_remote_data = false;
    initial.title = "Old Local Series".to_string();
    initial.season_title = Some("Old Local Season".to_string());
    initial.season_overview = Some("Old season overview".to_string());
    initial.season_poster_path = Some("/media/series/Local/old-season.jpg".to_string());
    sync_library_media(&pool, library.id, std::slice::from_ref(&initial))
        .await
        .unwrap();

    let mut refreshed = initial.clone();
    refreshed.metadata_status = METADATA_STATUS_PENDING.to_string();
    refreshed.title = "New Local Series".to_string();
    refreshed.season_title = Some("New Local Season".to_string());
    refreshed.season_overview = Some("New season overview".to_string());
    refreshed.season_poster_path = Some("/media/series/Local/new-season.jpg".to_string());
    let mut tx = pool.begin().await.unwrap();
    let existing = get_existing_library_media_file_by_path(&mut tx, library.id, file_path)
        .await
        .unwrap();
    upsert_media_entry_with_policy(&mut tx, &refreshed, existing, true)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    refreshed.metadata_status = METADATA_STATUS_FAILED.to_string();
    refreshed.metadata_failure_reason = Some(METADATA_FAILURE_PROVIDER_ERROR.to_string());
    let mut tx = pool.begin().await.unwrap();
    let existing = get_existing_library_media_file_by_path(&mut tx, library.id, file_path)
        .await
        .unwrap();
    upsert_media_entry_with_policy(&mut tx, &refreshed, existing, true)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    let series_title = sqlx::query_scalar::<_, String>(
        "select title from media_items where library_id = $1 and media_type = 'series'",
    )
    .bind(library.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let season = sqlx::query_as::<_, (String, Option<String>, Option<String>)>(
        "select title, overview, poster_path from seasons",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(series_title, "New Local Series");
    assert_eq!(season.0, "New Local Season");
    assert_eq!(season.1.as_deref(), Some("New season overview"));
    assert_eq!(
        season.2.as_deref(),
        Some("/media/series/Local/new-season.jpg")
    );
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
async fn sync_library_media_reuses_one_movie_record_for_multiple_files(
    pool: sqlx::postgres::PgPool,
) {
    let library = create_library(
        &pool,
        CreateLibraryParams {
            name: "Movies".to_string(),
            description: None,
            metadata_language: "en-US".to_string(),
            root_path: "/media/movies".to_string(),
        },
    )
    .await
    .unwrap();

    let entries = vec![
        build_movie_entry(
            library.id,
            "/media/movies/A Writer's Odyssey (2025)/A Writer's Odyssey (2025).2160p.mkv",
        ),
        build_movie_entry(
            library.id,
            "/media/movies/A Writer's Odyssey (2025)/A Writer's Odyssey (2025).remux.mkv",
        ),
    ];

    sync_library_media(&pool, library.id, &entries)
        .await
        .unwrap();

    let movie_media_item_count =
        sqlx::query_scalar::<_, i64>("select count(*) from media_items where media_type = 'movie'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let media_file_count = sqlx::query_scalar::<_, i64>("select count(*) from media_files")
        .fetch_one(&pool)
        .await
        .unwrap();
    let linked_file_count = sqlx::query_scalar::<_, i64>(
        r#"
        select count(*)
        from media_files
        where media_item_id = (
            select id
            from media_items
            where media_type = 'movie'
            limit 1
        )
        "#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(movie_media_item_count, 1);
    assert_eq!(media_file_count, 2);
    assert_eq!(linked_file_count, 2);
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
async fn sync_library_media_reuses_one_movie_record_for_same_remote_id(
    pool: sqlx::postgres::PgPool,
) {
    let library = create_library(
        &pool,
        CreateLibraryParams {
            name: "Movies".to_string(),
            description: None,
            metadata_language: "zh-CN".to_string(),
            root_path: "/media/movies".to_string(),
        },
    )
    .await
    .unwrap();

    let mut first_entry = build_movie_entry(
        library.id,
        "/media/movies/Avatar: Fire and Ash/Avatar: Fire and Ash.2025.2160p.mkv",
    );
    first_entry.metadata_provider_item_id = Some("999_001".to_string());
    first_entry.title = "阿凡达：火与烬".to_string();
    first_entry.source_title = "Avatar: Fire and Ash".to_string();
    first_entry.original_title = Some("Avatar: Fire and Ash".to_string());

    let mut second_entry = build_movie_entry(
        library.id,
        "/media/movies/阿凡达.2025/Avatar： Fire and Ash (2025) - 1080p WEB-DL.mkv",
    );
    second_entry.metadata_provider_item_id = Some("999_001".to_string());
    second_entry.title = "阿凡达：火与烬".to_string();
    second_entry.source_title = "Avatar： Fire and Ash".to_string();
    second_entry.original_title = Some("Avatar: Fire and Ash".to_string());

    sync_library_media(&pool, library.id, &[first_entry, second_entry])
        .await
        .unwrap();

    let movie_media_item_count =
        sqlx::query_scalar::<_, i64>("select count(*) from media_items where media_type = 'movie'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let media_file_count = sqlx::query_scalar::<_, i64>("select count(*) from media_files")
        .fetch_one(&pool)
        .await
        .unwrap();
    let linked_file_count = sqlx::query_scalar::<_, i64>(
        r#"
        select count(*)
        from media_files
        where media_item_id = (
            select id
            from media_items
            where media_type = 'movie'
            limit 1
        )
        "#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(movie_media_item_count, 1);
    assert_eq!(media_file_count, 2);
    assert_eq!(linked_file_count, 2);
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
async fn sync_library_media_reuses_one_episode_record_for_multiple_files(
    pool: sqlx::postgres::PgPool,
) {
    let library = create_library(
        &pool,
        CreateLibraryParams {
            name: "Shows".to_string(),
            description: None,
            metadata_language: "en-US".to_string(),
            root_path: "/media/shows".to_string(),
        },
    )
    .await
    .unwrap();

    let entries = vec![
        build_episode_entry(
            library.id,
            "/media/shows/Interstellar Classroom/Season 01/S01E01.1080p.mkv",
        ),
        build_episode_entry(
            library.id,
            "/media/shows/Interstellar Classroom/Season 01/S01E01.4k.mkv",
        ),
    ];

    sync_library_media(&pool, library.id, &entries)
        .await
        .unwrap();

    let episode_count = sqlx::query_scalar::<_, i64>("select count(*) from episodes")
        .fetch_one(&pool)
        .await
        .unwrap();
    let episode_media_item_count = sqlx::query_scalar::<_, i64>(
        "select count(*) from media_items where media_type = 'episode'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let series_media_item_count = sqlx::query_scalar::<_, i64>(
        "select count(*) from media_items where media_type = 'series'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let media_file_count = sqlx::query_scalar::<_, i64>("select count(*) from media_files")
        .fetch_one(&pool)
        .await
        .unwrap();
    let linked_file_count = sqlx::query_scalar::<_, i64>(
        r#"
        select count(*)
        from media_files
        where media_item_id = (
            select media_item_id
            from episodes
            limit 1
        )
        "#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(series_media_item_count, 1);
    assert_eq!(episode_media_item_count, 1);
    assert_eq!(episode_count, 1);
    assert_eq!(media_file_count, 2);
    assert_eq!(linked_file_count, 2);
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
async fn sync_library_media_merges_episode_versions_with_the_same_remote_series_id(
    pool: sqlx::postgres::PgPool,
) {
    let library = create_library(
        &pool,
        CreateLibraryParams {
            name: "Shows".to_string(),
            description: None,
            metadata_language: "zh-CN".to_string(),
            root_path: "/media/shows".to_string(),
        },
    )
    .await
    .unwrap();

    let mut localized_entry = build_episode_entry(
        library.id,
        "/media/shows/金斯敦市长/S01/金斯敦市长.S01E01.1080p.mkv",
    );
    localized_entry.metadata_provider_item_id = Some("97_951".to_string());
    localized_entry.title = "金斯敦市长".to_string();
    localized_entry.source_title = "金斯敦市长".to_string();
    localized_entry.original_title = Some("Mayor of Kingstown".to_string());
    localized_entry.year = Some(2021);

    let mut original_title_entry = build_episode_entry(
        library.id,
        "/media/shows/Mayor of Kingstown/Season 01/Mayor of Kingstown.S01E01.2160p.mkv",
    );
    original_title_entry.metadata_provider_item_id = Some("97_951".to_string());
    original_title_entry.title = "金斯敦市长".to_string();
    original_title_entry.source_title = "Mayor of Kingstown".to_string();
    original_title_entry.original_title = Some("Mayor of Kingstown".to_string());
    original_title_entry.year = Some(2021);

    sync_library_media(&pool, library.id, &[localized_entry, original_title_entry])
        .await
        .unwrap();

    let series_media_item_count = sqlx::query_scalar::<_, i64>(
        "select count(*) from media_items where media_type = 'series'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let episode_media_item_count = sqlx::query_scalar::<_, i64>(
        "select count(*) from media_items where media_type = 'episode'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let media_file_count = sqlx::query_scalar::<_, i64>("select count(*) from media_files")
        .fetch_one(&pool)
        .await
        .unwrap();
    let linked_file_count = sqlx::query_scalar::<_, i64>(
        r#"
        select count(*)
        from media_files
        where media_item_id = (
            select media_item_id
            from episodes
            limit 1
        )
        "#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(series_media_item_count, 1);
    assert_eq!(episode_media_item_count, 1);
    assert_eq!(media_file_count, 2);
    assert_eq!(linked_file_count, 2);
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
async fn staged_scan_merges_local_series_after_remote_match_and_keeps_selected_version(
    pool: sqlx::postgres::PgPool,
) {
    let (library_id, scan_job_id, fence) = seed_running_scan(
        &pool,
        "Overseas TV",
        "/media/overseas_tv",
        "series-merge-worker",
    )
    .await;
    initialize_scan_job_work(&pool, scan_job_id, 2, 2, &fence)
        .await
        .unwrap()
        .unwrap();

    let localized_group = "series-folder://media/overseas_tv/金斯敦市长";
    let original_group = "series-folder://media/overseas_tv/金斯敦市长 (2021)";
    mark_scan_group_analyzed(&pool, scan_job_id, localized_group, 1, &fence)
        .await
        .unwrap()
        .unwrap();
    mark_scan_group_analyzed(&pool, scan_job_id, original_group, 1, &fence)
        .await
        .unwrap()
        .unwrap();

    let localized_path = "/media/overseas_tv/金斯敦市长/S01/金斯敦市长.S01E01.1080p.mkv";
    let original_path =
        "/media/overseas_tv/金斯敦市长 (2021)/Season 01/Mayor of Kingstown.S01E01.2160p.mkv";

    let mut localized_local = build_episode_entry(library_id, localized_path);
    localized_local.metadata_provider = None;
    localized_local.metadata_provider_item_id = None;
    localized_local.metadata_status = METADATA_STATUS_PENDING.to_string();
    localized_local.replace_remote_data = false;
    localized_local.title = "金斯敦市长".to_string();
    localized_local.source_title = "金斯敦市长".to_string();
    localized_local.original_title = None;
    localized_local.year = None;
    localized_local.episode_title = Some("本地第 1 集".to_string());

    let mut original_local = build_episode_entry(library_id, original_path);
    original_local.metadata_provider = None;
    original_local.metadata_provider_item_id = None;
    original_local.metadata_status = METADATA_STATUS_PENDING.to_string();
    original_local.replace_remote_data = false;
    original_local.title = "Mayor of Kingstown".to_string();
    original_local.source_title = "Mayor of Kingstown".to_string();
    original_local.original_title = None;
    original_local.year = Some(2021);
    original_local.episode_title = Some("Local Episode 1".to_string());

    upsert_library_media_entries_by_file_path(
        &pool,
        scan_job_id,
        library_id,
        localized_group,
        ScanGroupCommitStage::Local,
        std::slice::from_ref(&localized_local),
        &fence,
    )
    .await
    .unwrap();
    upsert_library_media_entries_by_file_path(
        &pool,
        scan_job_id,
        library_id,
        original_group,
        ScanGroupCommitStage::Local,
        std::slice::from_ref(&original_local),
        &fence,
    )
    .await
    .unwrap();

    let local_counts = sqlx::query_as::<_, (i64, i64, i64)>(
        r#"
        select
            (select count(*) from media_items where library_id = $1 and media_type = 'series'),
            (select count(*) from media_items where library_id = $1 and media_type = 'episode'),
            (select count(*) from media_files where library_id = $1)
        "#,
    )
    .bind(library_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(local_counts, (2, 2, 2));

    let (original_series_id, original_episode_id, original_media_file_id) =
        sqlx::query_as::<_, (i64, i64, i64)>(
            r#"
            select series.id, episode.id, mf.id
            from media_files mf
            join media_items episode on episode.id = mf.media_item_id
            join episodes e on e.media_item_id = episode.id
            join seasons s on s.id = e.season_id
            join media_items series on series.id = s.series_id
            where mf.library_id = $1
              and mf.file_path = $2
            "#,
        )
        .bind(library_id)
        .bind(original_path)
        .fetch_one(&pool)
        .await
        .unwrap();
    let user_id = sqlx::query_scalar::<_, i64>(
        r#"
        insert into users (
            username,
            username_normalized,
            nickname,
            password_hash,
            role
        )
        values ('viewer', 'viewer', 'Viewer', 'hash', 'viewer')
        returning id
        "#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"
        insert into playback_progress (
            user_id,
            media_item_id,
            last_media_file_id,
            position_seconds,
            duration_seconds,
            last_watched_at
        )
        values ($1, $2, $3, 480, 1800, '2026-07-30T00:00:00Z')
        "#,
    )
    .bind(user_id)
    .bind(original_episode_id)
    .bind(original_media_file_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"
        insert into continue_watching (
            user_id,
            media_item_id,
            last_played_media_item_id,
            last_watched_at
        )
        values ($1, $2, $3, '2026-07-30T00:00:00Z')
        "#,
    )
    .bind(user_id)
    .bind(original_series_id)
    .bind(original_episode_id)
    .execute(&pool)
    .await
    .unwrap();

    let mut localized_remote = localized_local;
    localized_remote.metadata_provider = Some("tmdb".to_string());
    localized_remote.metadata_provider_item_id = Some("97951".to_string());
    localized_remote.metadata_status = METADATA_STATUS_MATCHED.to_string();
    localized_remote.remote_media_type = Some(REMOTE_MEDIA_TYPE_SERIES.to_string());
    localized_remote.replace_remote_data = true;
    localized_remote.title = "金斯敦市长".to_string();
    localized_remote.original_title = Some("Mayor of Kingstown".to_string());
    localized_remote.year = Some(2021);
    localized_remote.episode_title = Some("第 1 集".to_string());

    let mut original_remote = original_local;
    original_remote.metadata_provider = Some("tmdb".to_string());
    original_remote.metadata_provider_item_id = Some("97951".to_string());
    original_remote.metadata_status = METADATA_STATUS_MATCHED.to_string();
    original_remote.remote_media_type = Some(REMOTE_MEDIA_TYPE_SERIES.to_string());
    original_remote.replace_remote_data = true;
    original_remote.title = "金斯敦市长".to_string();
    original_remote.original_title = Some("Mayor of Kingstown".to_string());
    original_remote.year = Some(2021);
    original_remote.episode_title = Some("第 1 集".to_string());

    patch_library_media_entries_remote_by_file_path(
        &pool,
        scan_job_id,
        library_id,
        localized_group,
        std::slice::from_ref(&localized_remote),
        &fence,
    )
    .await
    .unwrap();
    patch_library_media_entries_remote_by_file_path(
        &pool,
        scan_job_id,
        library_id,
        original_group,
        std::slice::from_ref(&original_remote),
        &fence,
    )
    .await
    .unwrap();

    let final_counts = sqlx::query_as::<_, (i64, i64, i64, i64, i64, i64)>(
        r#"
        select
            (select count(*) from media_items where library_id = $1 and media_type = 'series'),
            (select count(*) from media_items where library_id = $1 and media_type = 'episode'),
            (select count(*) from seasons where library_id = $1),
            (select count(*) from episodes where library_id = $1),
            (select count(*) from media_files where library_id = $1),
            (
                select count(*)
                from media_files mf
                join episodes e on e.media_item_id = mf.media_item_id
                join seasons s on s.id = e.season_id
                where mf.library_id = $1
                  and s.season_number = 1
                  and e.episode_number = 1
            )
        "#,
    )
    .bind(library_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(final_counts, (1, 1, 1, 1, 2, 2));

    let orphan_counts = sqlx::query_as::<_, (i64, i64, i64)>(
        r#"
        select
            (
                select count(*)
                from media_items mi
                left join episodes e on e.media_item_id = mi.id
                where mi.library_id = $1
                  and mi.media_type = 'episode'
                  and e.media_item_id is null
            ),
            (
                select count(*)
                from media_items mi
                left join seasons s on s.series_id = mi.id
                where mi.library_id = $1
                  and mi.media_type = 'series'
                  and s.id is null
            ),
            (
                select count(*)
                from seasons s
                left join episodes e on e.season_id = s.id
                where s.library_id = $1
                  and e.media_item_id is null
            )
        "#,
    )
    .bind(library_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(orphan_counts, (0, 0, 0));

    let (final_series_id, final_episode_id) = sqlx::query_as::<_, (i64, i64)>(
        r#"
        select s.series_id, e.media_item_id
        from episodes e
        join seasons s on s.id = e.season_id
        where e.library_id = $1
          and s.season_number = 1
          and e.episode_number = 1
        "#,
    )
    .bind(library_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let progress_state = sqlx::query_as::<_, (i64, Option<i64>, i32)>(
        r#"
        select media_item_id, last_media_file_id, position_seconds
        from playback_progress
        where user_id = $1
        "#,
    )
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        progress_state,
        (final_episode_id, Some(original_media_file_id), 480)
    );
    let continue_state = sqlx::query_as::<_, (i64, i64)>(
        r#"
        select media_item_id, last_played_media_item_id
        from continue_watching
        where user_id = $1
        "#,
    )
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(continue_state, (final_series_id, final_episode_id));
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
async fn sync_library_media_best_effort_keeps_healthy_entries_when_one_entry_is_invalid(
    pool: sqlx::postgres::PgPool,
) {
    let library = create_library(
        &pool,
        CreateLibraryParams {
            name: "Movies".to_string(),
            description: None,
            metadata_language: "en-US".to_string(),
            root_path: "/media/movies".to_string(),
        },
    )
    .await
    .unwrap();

    let mut invalid_entry =
        build_movie_entry(library.id, "/media/movies/Broken/Broken.invalid.mkv");
    invalid_entry.metadata_provider = Some("x".repeat(33));

    let valid_entry = build_movie_entry(library.id, "/media/movies/Healthy/Healthy.mkv");

    let outcome =
        super::sync_library_media_best_effort(&pool, library.id, &[invalid_entry, valid_entry])
            .await
            .unwrap();

    let media_item_count =
        sqlx::query_scalar::<_, i64>("select count(*) from media_items where library_id = $1")
            .bind(library.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let media_file_count =
        sqlx::query_scalar::<_, i64>("select count(*) from media_files where library_id = $1")
            .bind(library.id)
            .fetch_one(&pool)
            .await
            .unwrap();

    assert_eq!(outcome.failed_count, 1);
    assert_eq!(outcome.upserted_count, 1);
    assert_eq!(media_item_count, 1);
    assert_eq!(media_file_count, 1);
}
