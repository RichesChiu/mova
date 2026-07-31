use crate::metadata::{MetadataLookup, MetadataProvider, RemoteMetadata};
use async_trait::async_trait;
use mova_db::ExistingMediaMetadataSummary;
use mova_domain::{
    Library, MediaExternalId, MediaRating, METADATA_FAILURE_NO_REMOTE_MATCH,
    METADATA_FAILURE_PROVIDER_ERROR, METADATA_STATUS_FAILED, METADATA_STATUS_MATCHED,
    METADATA_STATUS_PENDING, METADATA_STATUS_SKIPPED, METADATA_STATUS_UNMATCHED,
    REMOTE_MEDIA_TYPE_MOVIE, REMOTE_MEDIA_TYPE_SERIES,
};
use mova_scan::{
    discovered_media_file_inventory_scan_hash, discovered_media_file_scan_hash,
    DiscoveredMediaFile, DiscoveredMediaFileInventory,
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc},
    time::Instant,
};
use time::OffsetDateTime;

fn build_discovered_file() -> DiscoveredMediaFile {
    DiscoveredMediaFile {
        file_path: PathBuf::from("/media/series/Arcane/Arcane.S01E01.mkv"),
        file_modified_at_ms: Some(1_700_000_000_000),
        sidecar_fingerprint: String::new(),
        probe_error: None,
        metadata_provider: None,
        metadata_provider_item_id: None,
        title: "Arcane".to_string(),
        source_title: "Arcane.S01E01".to_string(),
        original_title: None,
        sort_title: None,
        series_sidecar_title: None,
        series_sidecar_year: None,
        year: Some(2021),
        external_ids: Vec::new(),
        ratings: Vec::new(),
        metadata_status: Some(METADATA_STATUS_MATCHED.to_string()),
        metadata_failure_reason: None,
        remote_media_type: None,
        country: None,
        genres: None,
        studio: None,
        season_number: Some(1),
        season_title: None,
        season_overview: None,
        season_poster_path: None,
        season_backdrop_path: None,
        episode_number: Some(1),
        episode_title: Some("Welcome to the Playground".to_string()),
        overview: None,
        series_poster_path: None,
        series_backdrop_path: None,
        series_logo_path: None,
        poster_path: None,
        backdrop_path: None,
        logo_path: None,
        file_size: 1,
        container: Some("mkv".to_string()),
        duration_seconds: Some(2400),
        video_title: None,
        video_codec: None,
        video_profile: None,
        video_level: None,
        audio_codec: None,
        width: None,
        height: None,
        bitrate: None,
        video_bitrate: None,
        video_frame_rate: None,
        video_aspect_ratio: None,
        video_scan_type: None,
        video_color_primaries: None,
        video_color_space: None,
        video_color_transfer: None,
        video_bit_depth: None,
        video_pixel_format: None,
        video_reference_frames: None,
        technical_tags: Vec::new(),
        audio_tracks: Vec::new(),
        subtitle_tracks: Vec::new(),
    }
}

#[test]
fn scan_notification_summary_keeps_matched_provider_failure_and_probe_warning_separate() {
    let mut file = build_discovered_file();
    file.metadata_status = Some(METADATA_STATUS_MATCHED.to_string());
    file.metadata_failure_reason = Some(METADATA_FAILURE_PROVIDER_ERROR.to_string());
    file.probe_error = Some("ffprobe failed:\n EBML header parsing failed".to_string());
    let group = super::ScanDiscoveredGroup {
        presentation: super::ScanPresentationGroup {
            item_key: "movie:a-minecraft-movie:2025".to_string(),
            media_type: "movie".to_string(),
            title: "A Minecraft Movie".to_string(),
            lookup_title: "A Minecraft Movie".to_string(),
            year: Some(2025),
            season_air_year: None,
        },
        files: vec![file],
        metadata_lookup_hint: None,
        metadata_binding_conflict: false,
    };

    let mut summary = mova_domain::ScanNotificationSummary::default();
    super::record_scan_notification_group(&mut summary, &group, Some("operation\n timed out"));
    let result = &summary.issues[0];

    assert_eq!(summary.matched_files, 1);
    assert_eq!(summary.failed_files, 0);
    assert_eq!(summary.probe_warning_count, 1);
    assert_eq!(summary.issue_count, 1);
    assert_eq!(result.metadata_status, METADATA_STATUS_MATCHED);
    assert_eq!(result.reason_code, METADATA_FAILURE_PROVIDER_ERROR);
    assert_eq!(
        result.diagnostic_message.as_deref(),
        Some("operation timed out")
    );
    assert_eq!(result.probe_warning_count, 1);
    assert_eq!(
        result.probe_warning_code.as_deref(),
        Some("media_probe_warning")
    );
    assert_eq!(
        result.probe_warning_diagnostic.as_deref(),
        Some("ffprobe failed: EBML header parsing failed")
    );
}

#[test]
fn scan_notification_summary_counts_all_issues_but_bounds_payload_details() {
    let issue_total = mova_domain::MAX_SCAN_NOTIFICATION_ISSUES + 5;
    let mut summary = mova_domain::ScanNotificationSummary::default();

    for index in 0..issue_total {
        let mut file = build_discovered_file();
        file.metadata_status = Some(METADATA_STATUS_UNMATCHED.to_string());
        file.metadata_failure_reason = Some(METADATA_FAILURE_NO_REMOTE_MATCH.to_string());
        let group = super::ScanDiscoveredGroup {
            presentation: super::ScanPresentationGroup {
                item_key: format!("movie:unmatched:{index}"),
                media_type: "movie".to_string(),
                title: format!("Unmatched {index}"),
                lookup_title: format!("Unmatched {index}"),
                year: None,
                season_air_year: None,
            },
            files: vec![file],
            metadata_lookup_hint: None,
            metadata_binding_conflict: false,
        };

        super::record_scan_notification_group(&mut summary, &group, None);
    }

    assert_eq!(summary.unmatched_files, issue_total as i32);
    assert_eq!(summary.issue_count, issue_total as i32);
    assert_eq!(
        summary.issues.len(),
        mova_domain::MAX_SCAN_NOTIFICATION_ISSUES
    );
}

fn build_pending_scan_file(file: DiscoveredMediaFile) -> super::PendingScanFile {
    super::PendingScanFile {
        changed_file: super::IncrementalScanFile {
            inventory: DiscoveredMediaFileInventory {
                file_path: file.file_path.clone(),
                file_size: file.file_size,
                file_modified_at_ms: file.file_modified_at_ms,
                sidecar_fingerprint: file.sidecar_fingerprint.clone(),
            },
            existing_metadata: None,
        },
        file,
    }
}

#[derive(Debug, Clone)]
struct FixedMetadataProvider {
    enabled: bool,
}

#[async_trait]
impl MetadataProvider for FixedMetadataProvider {
    fn is_enabled(&self) -> bool {
        self.enabled
    }

    async fn lookup(&self, _lookup: &MetadataLookup) -> anyhow::Result<Option<RemoteMetadata>> {
        Ok(None)
    }
}

fn build_library() -> Library {
    Library {
        id: 7,
        name: "Library".to_string(),
        description: None,
        metadata_language: "zh-CN".to_string(),
        root_path: "/media".to_string(),
        created_at: OffsetDateTime::now_utc(),
        updated_at: OffsetDateTime::now_utc(),
    }
}

fn build_existing_movie_metadata() -> ExistingMediaMetadataSummary {
    ExistingMediaMetadataSummary {
        media_file_id: 11,
        file_path: "/media/movies/Arcane.mkv".to_string(),
        media_type: "movie".to_string(),
        metadata_provider: Some("tmdb".to_string()),
        metadata_provider_item_id: Some("77".to_string()),
        metadata_status: METADATA_STATUS_MATCHED.to_string(),
        metadata_failure_reason: None,
        remote_media_type: Some(REMOTE_MEDIA_TYPE_MOVIE.to_string()),
        title: "Arcane".to_string(),
        source_title: "Arcane".to_string(),
        original_title: Some("Arcane Original".to_string()),
        sort_title: Some("Arcane, The".to_string()),
        year: Some(2021),
        country: Some("United States".to_string()),
        genres: Some("Animation, Drama".to_string()),
        studio: Some("Fortiche".to_string()),
        overview: Some("Stored overview".to_string()),
        poster_path: Some("/cache/poster.jpg".to_string()),
        backdrop_path: Some("/cache/backdrop.jpg".to_string()),
        logo_path: Some("/cache/logo.png".to_string()),
        scan_hash: Some("movie-hash".to_string()),
        container: Some("mkv".to_string()),
        file_size: 1024,
        duration_seconds: Some(600),
        video_title: Some("Video stream".to_string()),
        video_codec: Some("h264".to_string()),
        video_profile: Some("main".to_string()),
        video_level: Some("4.1".to_string()),
        audio_codec: Some("aac".to_string()),
        width: Some(1920),
        height: Some(1080),
        bitrate: Some(1_000_000),
        video_bitrate: Some(800_000),
        video_frame_rate: Some(24.0),
        video_aspect_ratio: Some("16:9".to_string()),
        video_scan_type: Some("Progressive".to_string()),
        video_color_primaries: Some("bt709".to_string()),
        video_color_space: Some("bt709".to_string()),
        video_color_transfer: Some("bt709".to_string()),
        video_bit_depth: Some(8),
        video_pixel_format: Some("yuv420p".to_string()),
        video_reference_frames: Some(4),
        technical_tags: vec!["HDR10".to_string()],
        local_analysis_version: super::LOCAL_ANALYSIS_VERSION,
        audio_tracks: vec![mova_db::CreateAudioTrackParams {
            stream_index: 1,
            language: Some("eng".to_string()),
            audio_codec: Some("aac".to_string()),
            label: Some("English AAC".to_string()),
            channel_layout: Some("stereo".to_string()),
            channels: Some(2),
            bitrate: Some(160_000),
            sample_rate: Some(48_000),
            is_default: true,
        }],
        subtitle_tracks: vec![mova_db::CreateSubtitleTrackParams {
            source_kind: "embedded".to_string(),
            file_path: None,
            stream_index: Some(2),
            language: Some("eng".to_string()),
            subtitle_format: "subrip".to_string(),
            label: Some("English".to_string()),
            is_default: false,
            is_forced: false,
            is_hearing_impaired: false,
        }],
        series_title: None,
        series_metadata_provider: None,
        series_metadata_provider_item_id: None,
        series_source_title: None,
        series_original_title: None,
        series_sort_title: None,
        series_year: None,
        series_country: None,
        series_genres: None,
        series_studio: None,
        series_overview: None,
        series_poster_path: None,
        series_backdrop_path: None,
        series_logo_path: None,
        season_title: None,
        season_number: None,
        season_overview: None,
        season_poster_path: None,
        season_backdrop_path: None,
        episode_title: None,
        episode_number: None,
    }
}

fn build_existing_episode_metadata() -> ExistingMediaMetadataSummary {
    ExistingMediaMetadataSummary {
        media_file_id: 22,
        file_path: "/media/series/Arcane/Arcane.S01E01.mkv".to_string(),
        media_type: "episode".to_string(),
        metadata_provider: Some("tmdb".to_string()),
        metadata_provider_item_id: Some("88".to_string()),
        metadata_status: METADATA_STATUS_MATCHED.to_string(),
        metadata_failure_reason: None,
        remote_media_type: Some(REMOTE_MEDIA_TYPE_SERIES.to_string()),
        title: "Welcome to the Playground".to_string(),
        source_title: "Arcane.S01E01".to_string(),
        original_title: None,
        sort_title: None,
        year: None,
        country: None,
        genres: None,
        studio: None,
        overview: Some("Episode overview".to_string()),
        poster_path: Some("/cache/episode-poster.jpg".to_string()),
        backdrop_path: Some("/cache/episode-backdrop.jpg".to_string()),
        logo_path: None,
        scan_hash: Some("episode-hash".to_string()),
        container: Some("mkv".to_string()),
        file_size: 2048,
        duration_seconds: Some(1200),
        video_title: Some("Episode video".to_string()),
        video_codec: Some("hevc".to_string()),
        video_profile: Some("main10".to_string()),
        video_level: Some("5.1".to_string()),
        audio_codec: Some("eac3".to_string()),
        width: Some(3840),
        height: Some(2160),
        bitrate: Some(8_000_000),
        video_bitrate: Some(7_000_000),
        video_frame_rate: Some(24.0),
        video_aspect_ratio: Some("16:9".to_string()),
        video_scan_type: Some("Progressive".to_string()),
        video_color_primaries: Some("bt2020".to_string()),
        video_color_space: Some("bt2020nc".to_string()),
        video_color_transfer: Some("smpte2084".to_string()),
        video_bit_depth: Some(10),
        video_pixel_format: Some("yuv420p10le".to_string()),
        video_reference_frames: Some(5),
        technical_tags: vec!["Dolby Vision".to_string()],
        local_analysis_version: super::LOCAL_ANALYSIS_VERSION,
        audio_tracks: vec![mova_db::CreateAudioTrackParams {
            stream_index: 1,
            language: Some("eng".to_string()),
            audio_codec: Some("eac3".to_string()),
            label: Some("English EAC3".to_string()),
            channel_layout: Some("5.1".to_string()),
            channels: Some(6),
            bitrate: Some(768_000),
            sample_rate: Some(48_000),
            is_default: true,
        }],
        subtitle_tracks: Vec::new(),
        series_title: Some("Arcane".to_string()),
        series_metadata_provider: Some("tmdb".to_string()),
        series_metadata_provider_item_id: Some("88".to_string()),
        series_source_title: Some("Arcane".to_string()),
        series_original_title: Some("Arcane Original".to_string()),
        series_sort_title: Some("Arcane, The".to_string()),
        series_year: Some(2021),
        series_country: Some("United States".to_string()),
        series_genres: Some("Animation, Drama".to_string()),
        series_studio: Some("Fortiche".to_string()),
        series_overview: Some("Series overview".to_string()),
        series_poster_path: Some("/cache/series-poster.jpg".to_string()),
        series_backdrop_path: Some("/cache/series-backdrop.jpg".to_string()),
        series_logo_path: Some("/cache/series-logo.png".to_string()),
        season_title: Some("Season 01".to_string()),
        season_number: Some(1),
        season_overview: Some("Season overview".to_string()),
        season_poster_path: Some("/cache/season-poster.jpg".to_string()),
        season_backdrop_path: Some("/cache/season-backdrop.jpg".to_string()),
        episode_title: Some("Welcome to the Playground".to_string()),
        episode_number: Some(1),
    }
}

#[test]
fn container_binding_index_reuses_one_accepted_tmdb_id_per_series_container() {
    let mut first = build_existing_episode_metadata();
    first.file_path = "/media/overseas_tv/金斯敦市长 (2021)/S01/金斯敦市长.S01E01.mkv".to_string();
    first.series_metadata_provider_item_id = Some("97951".to_string());
    first.metadata_provider_item_id = Some("97951".to_string());
    let mut second = first.clone();
    second.media_file_id = 23;
    second.file_path = "/media/overseas_tv/金斯敦市长 (2021)/S01/金斯敦市长.S01E02.mkv".to_string();

    let bindings =
        super::build_container_binding_index(&[first, second], Path::new("/media/overseas_tv"));
    let key = super::metadata_container_key_for_path(
        Path::new("/media/overseas_tv/金斯敦市长 (2021)/S01/金斯敦市长.S01E03.mkv"),
        Path::new("/media/overseas_tv"),
        "series",
    )
    .expect("series container key");

    assert_eq!(
        bindings.get(&key),
        Some(&super::ContainerBindingResolution::Unique(
            "97951".to_string()
        ))
    );
}

#[test]
fn container_binding_index_marks_distinct_ids_in_one_container_as_conflicting() {
    let mut first = build_existing_episode_metadata();
    first.file_path = "/media/series/Show/S01/Show.S01E01.mkv".to_string();
    first.series_metadata_provider_item_id = Some("100".to_string());
    first.metadata_provider_item_id = Some("100".to_string());
    let mut second = first.clone();
    second.media_file_id = 23;
    second.file_path = "/media/series/Show/S01/Show.S01E02.mkv".to_string();
    second.series_metadata_provider_item_id = Some("200".to_string());
    second.metadata_provider_item_id = Some("200".to_string());

    let bindings =
        super::build_container_binding_index(&[first, second], Path::new("/media/series"));
    let key = super::metadata_container_key_for_path(
        Path::new("/media/series/Show/S01/Show.S01E03.mkv"),
        Path::new("/media/series"),
        "series",
    )
    .expect("series container key");

    assert_eq!(
        bindings.get(&key),
        Some(&super::ContainerBindingResolution::Conflict)
    );
}

#[test]
fn can_skip_existing_media_summary_only_skips_successful_rows() {
    let mut summary = build_existing_movie_metadata();
    summary.scan_hash = Some("same-hash".to_string());

    assert!(super::can_skip_existing_media_summary(
        &summary,
        "same-hash",
        true,
        "zh-CN",
        Path::new("/media/movies/Arcane.mkv"),
    ));

    summary.metadata_failure_reason = Some(METADATA_FAILURE_PROVIDER_ERROR.to_string());
    assert!(!super::can_skip_existing_media_summary(
        &summary,
        "same-hash",
        true,
        "zh-CN",
        Path::new("/media/movies/Arcane.mkv"),
    ));
    summary.metadata_failure_reason = None;

    summary.metadata_status = METADATA_STATUS_UNMATCHED.to_string();
    assert!(!super::can_skip_existing_media_summary(
        &summary,
        "same-hash",
        true,
        "zh-CN",
        Path::new("/media/movies/Arcane.mkv"),
    ));

    summary.metadata_status = METADATA_STATUS_FAILED.to_string();
    assert!(!super::can_skip_existing_media_summary(
        &summary,
        "same-hash",
        true,
        "zh-CN",
        Path::new("/media/movies/Arcane.mkv"),
    ));

    summary.metadata_status = METADATA_STATUS_SKIPPED.to_string();
    assert!(!super::can_skip_existing_media_summary(
        &summary,
        "same-hash",
        true,
        "zh-CN",
        Path::new("/media/movies/Arcane.mkv"),
    ));
    assert!(super::can_skip_existing_media_summary(
        &summary,
        "same-hash",
        false,
        "zh-CN",
        Path::new("/media/movies/Arcane.mkv"),
    ));

    assert!(!super::can_skip_existing_media_summary(
        &summary,
        "changed-hash",
        false,
        "zh-CN",
        Path::new("/media/movies/Arcane.mkv"),
    ));

    summary.scan_hash = Some("same-hash".to_string());
    summary.local_analysis_version = super::LOCAL_ANALYSIS_VERSION - 1;
    assert!(!super::can_skip_existing_media_summary(
        &summary,
        "same-hash",
        false,
        "zh-CN",
        Path::new("/media/movies/Arcane.mkv"),
    ));
}

#[test]
fn can_skip_existing_media_summary_rescans_other_review_rows_even_without_provider() {
    let mut summary = build_existing_movie_metadata();
    summary.scan_hash = Some("same-hash".to_string());
    summary.metadata_status = METADATA_STATUS_SKIPPED.to_string();
    summary.metadata_provider = None;
    summary.metadata_provider_item_id = None;
    summary.original_title = None;
    summary.overview = None;
    summary.poster_path = None;
    summary.backdrop_path = None;
    summary.logo_path = None;

    assert!(!super::can_skip_existing_media_summary(
        &summary,
        "same-hash",
        false,
        "zh-CN",
        Path::new("/media/movies/Arcane.mkv"),
    ));
}

#[test]
fn prepare_scan_groups_marks_rows_as_pending_before_remote_confirmation() {
    let mut file = build_discovered_file();
    file.file_path = PathBuf::from("/media/movies/狂野时代 (2025)/狂野时代.2025.mp4");
    file.season_number = None;
    file.episode_number = None;
    file.title = "狂野时代".to_string();
    file.source_title = "狂野时代".to_string();
    file.metadata_status = Some(METADATA_STATUS_SKIPPED.to_string());
    file.metadata_failure_reason = None;

    let presentation = super::build_scan_presentation_group(&file);
    let mut groups = vec![super::ScanDiscoveredGroup {
        presentation,
        files: vec![file],
        metadata_lookup_hint: None,
        metadata_binding_conflict: false,
    }];

    super::prepare_scan_groups_for_metadata_lookup(&mut groups);

    assert_eq!(
        groups[0].files[0].metadata_status.as_deref(),
        Some(METADATA_STATUS_PENDING)
    );
    assert_eq!(groups[0].files[0].metadata_failure_reason, None);
}

#[test]
fn can_skip_existing_media_summary_rescans_review_rows_with_visible_metadata() {
    let mut summary = build_existing_movie_metadata();
    summary.scan_hash = Some("same-hash".to_string());
    summary.metadata_status = METADATA_STATUS_UNMATCHED.to_string();
    summary.metadata_provider = None;
    summary.metadata_provider_item_id = None;

    assert!(!super::can_skip_existing_media_summary(
        &summary,
        "same-hash",
        true,
        "zh-CN",
        Path::new("/media/movies/Avatar.Fire.and.Ash.2025.mkv"),
    ));
    assert!(!super::is_existing_summary_in_other_review_section(
        &summary
    ));
}

#[test]
fn can_skip_existing_media_summary_keeps_matched_movies_without_poster_stable() {
    let mut summary = build_existing_movie_metadata();
    summary.scan_hash = Some("same-hash".to_string());
    summary.poster_path = None;

    assert!(super::can_skip_existing_media_summary(
        &summary,
        "same-hash",
        true,
        "zh-CN",
        Path::new("/media/movies/Arcane.mkv"),
    ));

    summary.poster_path = Some("https://image.tmdb.org/t/p/original/poster.jpg".to_string());

    assert!(!super::can_skip_existing_media_summary(
        &summary,
        "same-hash",
        true,
        "zh-CN",
        Path::new("/media/movies/Arcane.mkv"),
    ));

    summary.poster_path = Some("/cache/poster.jpg".to_string());
    summary.backdrop_path = Some("https://image.tmdb.org/t/p/original/backdrop.jpg".to_string());

    assert!(!super::can_skip_existing_media_summary(
        &summary,
        "same-hash",
        true,
        "zh-CN",
        Path::new("/media/movies/Arcane.mkv"),
    ));
}

#[test]
fn can_skip_existing_media_summary_retries_matched_rows_without_tmdb_binding() {
    let mut summary = build_existing_movie_metadata();
    summary.scan_hash = Some("same-hash".to_string());
    summary.metadata_provider_item_id = None;

    assert!(!super::can_skip_existing_media_summary(
        &summary,
        "same-hash",
        true,
        "zh-CN",
        Path::new("/media/movies/Arcane.mkv"),
    ));

    summary.metadata_provider_item_id = Some("77".to_string());
    summary.metadata_provider = None;

    assert!(!super::can_skip_existing_media_summary(
        &summary,
        "same-hash",
        true,
        "zh-CN",
        Path::new("/media/movies/Arcane.mkv"),
    ));
}

#[test]
fn discovered_file_from_existing_local_analysis_preserves_cached_probe_data() {
    let summary = build_existing_episode_metadata();
    let inventory = DiscoveredMediaFileInventory {
        file_path: PathBuf::from("/media/series/Arcane/Arcane.S01E01.mkv"),
        file_size: 2048,
        file_modified_at_ms: Some(1_700_000_000_000),
        sidecar_fingerprint: "sidecars".to_string(),
    };

    let file = super::discovered_file_from_existing_local_analysis(&inventory, &summary)
        .expect("cached local analysis should rebuild discovered file");

    assert_eq!(file.title, "Arcane");
    assert_eq!(file.source_title, "Arcane");
    assert_eq!(file.season_number, Some(1));
    assert_eq!(file.episode_number, Some(1));
    assert_eq!(file.metadata_provider.as_deref(), Some("tmdb"));
    assert_eq!(file.metadata_provider_item_id.as_deref(), Some("88"));
    assert_eq!(file.video_codec.as_deref(), Some("hevc"));
    assert_eq!(file.technical_tags, vec!["Dolby Vision".to_string()]);
    assert_eq!(file.audio_tracks.len(), 1);
    assert_eq!(file.audio_tracks[0].channel_layout.as_deref(), Some("5.1"));
    assert_eq!(
        discovered_media_file_scan_hash(&file),
        discovered_media_file_inventory_scan_hash(&inventory)
    );
}

#[test]
fn failed_episode_does_not_inherit_an_accepted_series_binding_for_retry_status() {
    let mut summary = build_existing_episode_metadata();
    summary.metadata_status = METADATA_STATUS_FAILED.to_string();
    summary.metadata_failure_reason = Some(METADATA_FAILURE_PROVIDER_ERROR.to_string());
    summary.metadata_provider = None;
    summary.metadata_provider_item_id = None;
    let inventory = DiscoveredMediaFileInventory {
        file_path: PathBuf::from("/media/series/Arcane/Arcane.S01E02.mkv"),
        file_size: 2048,
        file_modified_at_ms: Some(1_700_000_000_000),
        sidecar_fingerprint: "sidecars".to_string(),
    };

    let file = super::discovered_file_from_existing_local_analysis(&inventory, &summary)
        .expect("failed episode should retain cached local analysis");

    assert_eq!(file.metadata_provider, None);
    assert_eq!(file.metadata_provider_item_id, None);
    assert_eq!(
        file.metadata_status.as_deref(),
        Some(METADATA_STATUS_FAILED)
    );
    assert_eq!(
        file.metadata_failure_reason.as_deref(),
        Some(METADATA_FAILURE_PROVIDER_ERROR)
    );
}

#[test]
fn can_skip_existing_media_summary_keeps_matched_episodes_without_series_poster_stable() {
    let mut summary = build_existing_episode_metadata();
    summary.scan_hash = Some("same-hash".to_string());
    summary.series_poster_path = None;

    assert!(super::can_skip_existing_media_summary(
        &summary,
        "same-hash",
        true,
        "zh-CN",
        Path::new("/media/series/Arcane/Arcane.S01E01.mkv"),
    ));

    summary.series_poster_path =
        Some("https://image.tmdb.org/t/p/original/series-poster.jpg".to_string());

    assert!(!super::can_skip_existing_media_summary(
        &summary,
        "same-hash",
        true,
        "zh-CN",
        Path::new("/media/series/Arcane/Arcane.S01E01.mkv"),
    ));
}

#[test]
fn can_skip_existing_media_summary_ignores_series_directory_title() {
    let mut summary = build_existing_episode_metadata();
    summary.scan_hash = Some("same-hash".to_string());
    summary.series_title = Some("Resolved Series".to_string());
    summary.series_source_title = Some("All Her Fault".to_string());

    assert!(super::can_skip_existing_media_summary(
        &summary,
        "same-hash",
        true,
        "zh-CN",
        Path::new(
            "/media/overseas_tv/都是她的错.2025/Season 01/All.Her.Fault.2025.S01E01.2160p.PCOK.WEB-DL.DDP5.1.H.265-KRATOS.mkv",
        ),
    ));

    assert!(super::can_skip_existing_media_summary(
        &summary,
        "same-hash",
        true,
        "zh-CN",
        Path::new(
            "/media/overseas_tv/莎拉的真伪人生(2026)/The.Art.of.Sarah.S01E01.2160p.NF.WEB-DL.DDP.5.1.DV.H.265.mkv",
        ),
    ));
}

#[test]
fn can_skip_existing_media_summary_retries_local_display_title_override() {
    let mut summary = build_existing_episode_metadata();
    summary.scan_hash = Some("same-hash".to_string());
    summary.series_title = Some("Alls Fair (2025)".to_string());
    summary.series_source_title = Some("Alls Fair".to_string());
    summary.series_metadata_provider_item_id = Some("259909".to_string());
    summary.series_poster_path = Some("/cache/series-poster.jpg".to_string());
    summary.series_backdrop_path = Some("/cache/series-backdrop.jpg".to_string());

    assert!(!super::can_skip_existing_media_summary(
        &summary,
        "same-hash",
        true,
        "zh-CN",
        Path::new("/media/overseas_tv/All's Fair (2025)/Season 01/Alls Fair (2025) - S01E01.mkv",),
    ));

    summary.series_title = Some("诉讼女王".to_string());
    assert!(super::can_skip_existing_media_summary(
        &summary,
        "same-hash",
        true,
        "zh-CN",
        Path::new("/media/overseas_tv/All's Fair (2025)/Season 01/Alls Fair (2025) - S01E01.mkv",),
    ));
}

#[test]
fn file_name_classification_recognizes_episode_like_paths() {
    assert_eq!(
        super::classify_media_type(Path::new("Arcane.S01E01.mkv")),
        "episode"
    );
}

#[test]
fn file_name_classification_recognizes_movie_like_paths() {
    assert_eq!(
        super::classify_media_type(Path::new("How.to.Train.Your.Dragon.2025.mkv")),
        "movie"
    );
}

#[test]
fn scan_phase_label_returns_user_facing_stage_name() {
    assert_eq!(
        super::scan_phase_label(super::SCAN_PHASE_DISCOVERING),
        "Directory scan failed"
    );
    assert_eq!(
        super::scan_phase_label(super::SCAN_PHASE_PROCESSING),
        "Media processing failed"
    );
    assert_eq!(
        super::scan_phase_label(super::SCAN_PHASE_FINALIZING),
        "Library finalization failed"
    );
}

#[test]
fn format_scan_phase_error_prefixes_stage_context() {
    assert_eq!(
        super::format_scan_phase_error(
            super::SCAN_PHASE_DISCOVERING,
            "Failed to scan library files: No such file or directory"
        ),
        "Directory scan failed: Failed to scan library files: No such file or directory"
    );
}

#[test]
fn should_flush_discovery_progress_for_first_visible_count() {
    let now = Instant::now();

    assert!(super::should_flush_discovery_progress(0, 1, None, now));
}

#[test]
fn should_flush_discovery_progress_after_file_delta_or_interval() {
    let now = Instant::now();
    let last_flush_at = now
        .checked_sub(super::SCAN_DISCOVERY_PROGRESS_MIN_INTERVAL)
        .expect("test instant should support subtraction");

    assert!(!super::should_flush_discovery_progress(
        10,
        20,
        Some(now),
        now
    ));
    assert!(super::should_flush_discovery_progress(
        10,
        10 + super::SCAN_DISCOVERY_PROGRESS_MIN_FILE_DELTA,
        Some(now),
        now
    ));
    assert!(super::should_flush_discovery_progress(
        10,
        20,
        Some(last_flush_at),
        now
    ));
}

#[test]
fn discovery_progress_signal_is_bounded_and_keeps_the_latest_count() {
    let latest = std::sync::atomic::AtomicI32::new(0);
    let (signal, mut receiver) = tokio::sync::mpsc::channel(1);

    super::publish_discovery_progress(&latest, &signal, 1);
    super::publish_discovery_progress(&latest, &signal, 2);
    super::publish_discovery_progress(&latest, &signal, 3);

    assert_eq!(latest.load(std::sync::atomic::Ordering::SeqCst), 3);
    assert_eq!(receiver.try_recv(), Ok(()));
    assert!(receiver.try_recv().is_err());
}

#[test]
fn build_scan_item_progress_update_emits_group_level_series_payload() {
    let presentation = super::build_scan_presentation_group(&build_discovered_file());
    let progress = super::build_scan_group_progress_update(
        41,
        7,
        &presentation,
        None,
        1,
        3,
        super::ScanItemStage::Analyzed,
    );

    assert_eq!(progress.scan_job_id, 41);
    assert_eq!(progress.library_id, 7);
    assert_eq!(progress.media_type, "series");
    assert_eq!(progress.title, "Arcane");
    assert_eq!(progress.season_number, None);
    assert_eq!(progress.episode_number, None);
    assert_eq!(progress.stage, "analyzed");
    assert_eq!(progress.progress_percent, 30);
    assert_eq!(progress.item_index, 1);
    assert_eq!(progress.total_items, 3);
    assert_eq!(progress.item_key, "series-title:arcane");
}

#[test]
fn build_scan_item_progress_update_holds_artwork_until_completed_and_browser_visible() {
    let mut file = build_discovered_file();
    file.series_poster_path = Some("https://image.tmdb.org/t/p/original/poster.jpg".to_string());
    file.series_backdrop_path =
        Some("https://image.tmdb.org/t/p/original/backdrop.jpg".to_string());
    let presentation = super::build_scan_presentation_group(&file);

    let artwork_progress = super::build_scan_group_progress_update(
        41,
        7,
        &presentation,
        Some(&file),
        1,
        3,
        super::ScanItemStage::Artwork,
    );
    assert_eq!(artwork_progress.poster_path, None);
    assert_eq!(artwork_progress.backdrop_path, None);

    let completed_progress = super::build_scan_group_progress_update(
        41,
        7,
        &presentation,
        Some(&file),
        1,
        3,
        super::ScanItemStage::Completed,
    );
    assert_eq!(
        completed_progress.poster_path.as_deref(),
        Some("https://image.tmdb.org/t/p/original/poster.jpg")
    );
    assert_eq!(
        completed_progress.backdrop_path.as_deref(),
        Some("https://image.tmdb.org/t/p/original/backdrop.jpg")
    );

    file.series_poster_path = Some("/media/series/Arcane/poster.jpg".to_string());
    let completed_with_local_artwork = super::build_scan_group_progress_update(
        41,
        7,
        &presentation,
        Some(&file),
        1,
        3,
        super::ScanItemStage::Completed,
    );
    assert_eq!(completed_with_local_artwork.poster_path, None);
}

#[test]
fn build_scan_item_progress_update_does_not_promote_episode_artwork_to_series() {
    let mut file = build_discovered_file();
    file.series_poster_path = None;
    file.series_backdrop_path = None;
    file.season_poster_path = None;
    file.season_backdrop_path = None;
    file.poster_path = Some("https://image.tmdb.org/t/p/original/episode-still.jpg".to_string());
    file.backdrop_path =
        Some("https://image.tmdb.org/t/p/original/episode-backdrop.jpg".to_string());
    let presentation = super::build_scan_presentation_group(&file);

    let completed_progress = super::build_scan_group_progress_update(
        41,
        7,
        &presentation,
        Some(&file),
        1,
        3,
        super::ScanItemStage::Completed,
    );

    assert_eq!(completed_progress.poster_path, None);
    assert_eq!(completed_progress.backdrop_path, None);
}

#[test]
fn build_scan_item_progress_update_does_not_promote_season_artwork_to_series() {
    let mut file = build_discovered_file();
    file.series_poster_path = None;
    file.series_backdrop_path = None;
    file.season_poster_path =
        Some("https://image.tmdb.org/t/p/original/season-poster.jpg".to_string());
    file.season_backdrop_path =
        Some("https://image.tmdb.org/t/p/original/season-backdrop.jpg".to_string());
    let presentation = super::build_scan_presentation_group(&file);

    let completed_progress = super::build_scan_group_progress_update(
        41,
        7,
        &presentation,
        Some(&file),
        1,
        3,
        super::ScanItemStage::Completed,
    );

    assert_eq!(completed_progress.poster_path, None);
    assert_eq!(completed_progress.backdrop_path, None);
}

#[test]
fn group_discovered_files_for_scan_merges_episode_files_by_series_folder() {
    let mut first_file = build_discovered_file();
    first_file.file_path = PathBuf::from("Arcane/Season 01/Arcane.S01E01.mkv");
    first_file.episode_number = Some(1);
    first_file.episode_title = Some("Welcome to the Playground".to_string());

    let mut second_file = build_discovered_file();
    second_file.file_path = PathBuf::from("Arcane/Season 01/Arcane.S01E02.mkv");
    second_file.episode_number = Some(2);
    second_file.episode_title = Some("Some Mysteries Are Better Left Unsolved".to_string());

    let groups = super::group_discovered_files_for_scan(vec![first_file, second_file]);

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].presentation.media_type, "series");
    assert_eq!(groups[0].presentation.title, "Arcane");
    assert_eq!(groups[0].files.len(), 2);
    assert_eq!(groups[0].presentation.item_key, "series-folder:arcane");
}

#[test]
fn root_aware_grouping_uses_container_identity_for_titleless_episodes() {
    let root = Path::new("/media/mainland");
    let mut first_file = build_discovered_file();
    first_file.file_path = root.join("千香/S01E01.2026.2160p.60fps.WEB-DL.H265.10bit.AAC.mp4");
    first_file.title = "S01E01 2026 2160p 60fps WEB DL H265 10bit AAC".to_string();
    first_file.source_title = first_file.title.clone();
    first_file.year = None;
    first_file.episode_title = None;

    let mut second_file = first_file.clone();
    second_file.file_path = root.join("千香/S01E02.2026.2160p.WEB-DL.H265.AAC.mp4");
    second_file.episode_number = Some(2);

    let groups =
        super::group_discovered_files_for_scan_with_root(vec![first_file, second_file], root);

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].presentation.media_type, "series");
    assert_eq!(groups[0].presentation.title, "千香");
    assert_eq!(groups[0].presentation.lookup_title, "千香");
    assert_eq!(groups[0].presentation.item_key, "series-folder:千香");
    assert_eq!(groups[0].files.len(), 2);
    assert!(groups[0]
        .files
        .iter()
        .all(|file| file.source_title == "千香"));
    assert!(groups[0]
        .files
        .iter()
        .all(|file| file.episode_title.is_none()));
}

#[test]
fn root_aware_grouping_does_not_take_container_year_when_episode_filename_has_series_title() {
    let root = Path::new("/media/series");
    let mut file = build_discovered_file();
    file.file_path = root.join("Wrong Parent (2030)/Fallout.S01E01.mkv");
    file.title = "Fallout".to_string();
    file.source_title = "Fallout".to_string();
    file.year = None;
    file.episode_title = None;

    let groups = super::group_discovered_files_for_scan_with_root(vec![file], root);

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].presentation.lookup_title, "Fallout");
    assert_eq!(groups[0].presentation.year, None);
    assert_eq!(groups[0].presentation.season_air_year, None);
    assert_eq!(groups[0].files[0].year, None);
}

#[test]
fn root_aware_grouping_keeps_filename_season_air_year_without_taking_container_year() {
    let root = Path::new("/media/series");
    let mut file = build_discovered_file();
    file.file_path = root.join("Wrong Parent (2030)/Fallout.S02E01.2025.mkv");
    file.title = "Fallout".to_string();
    file.source_title = "Fallout".to_string();
    file.year = Some(2025);
    file.season_number = Some(2);
    file.episode_title = None;

    let groups = super::group_discovered_files_for_scan_with_root(vec![file], root);

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].presentation.lookup_title, "Fallout");
    assert_eq!(groups[0].presentation.year, None);
    assert_eq!(
        groups[0].presentation.season_air_year,
        Some(crate::metadata::MetadataSeasonAirYearHint {
            season_number: 2,
            year: 2025,
        })
    );
    assert_eq!(groups[0].files[0].year, None);
}

#[test]
fn root_aware_grouping_takes_container_year_when_episode_filename_has_no_series_title() {
    let root = Path::new("/media/series");
    let mut file = build_discovered_file();
    file.file_path = root.join("Fallout (2021)/S01/S01E01.mkv");
    file.title = "S01E01".to_string();
    file.source_title = "S01E01".to_string();
    file.year = None;
    file.episode_title = None;

    let groups = super::group_discovered_files_for_scan_with_root(vec![file], root);

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].presentation.lookup_title, "Fallout");
    assert_eq!(groups[0].presentation.year, Some(2021));
    assert_eq!(groups[0].presentation.season_air_year, None);
    assert_eq!(groups[0].files[0].year, Some(2021));
}

#[test]
fn root_aware_grouping_uses_container_identity_for_titleless_movie() {
    let root = Path::new("/media/movies");
    let mut file = build_discovered_file();
    file.file_path =
        root.join("星球大战曼达洛人与古古(2026)/2026.2160p.iT.WEB-DL.DV.DDP5.1.Atmos.2Audio.mkv");
    file.title = "2026".to_string();
    file.source_title = "2026".to_string();
    file.year = Some(2026);
    file.season_number = None;
    file.episode_number = None;
    file.episode_title = None;

    let groups = super::group_discovered_files_for_scan_with_root(vec![file], root);

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].presentation.media_type, "movie");
    assert_eq!(groups[0].presentation.title, "星球大战曼达洛人与古古(2026)");
    assert_eq!(
        groups[0].presentation.lookup_title,
        "星球大战曼达洛人与古古"
    );
    assert_eq!(groups[0].presentation.year, Some(2026));
    assert_eq!(
        groups[0].presentation.item_key,
        "movie-folder:星球大战曼达洛人与古古(2026)"
    );
    assert_eq!(groups[0].files[0].source_title, "星球大战曼达洛人与古古");
}

#[test]
fn root_aware_movie_container_fallback_preserves_sidecar_display_title() {
    let root = Path::new("/media/movies");
    let mut file = build_discovered_file();
    file.file_path = root.join("Container Movie (2026)/2026.2160p.WEB-DL.mkv");
    file.title = "Sidecar Display Title".to_string();
    file.source_title = "2026".to_string();
    file.year = Some(2026);
    file.season_number = None;
    file.episode_number = None;
    file.episode_title = None;

    let groups = super::group_discovered_files_for_scan_with_root(vec![file], root);

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].presentation.title, "Sidecar Display Title");
    assert_eq!(groups[0].presentation.lookup_title, "Container Movie");
}

#[test]
fn root_aware_grouping_keeps_explicit_movie_file_title_and_uses_container_tmdb_hint() {
    let root = Path::new("/media/movies");
    let mut file = build_discovered_file();
    file.file_path = root.join("Container Title (2026) {tmdb-123456}/Actual.Movie.2025.2160p.mkv");
    file.title = "Actual Movie".to_string();
    file.source_title = "Actual Movie".to_string();
    file.year = Some(2025);
    file.season_number = None;
    file.episode_number = None;
    file.episode_title = None;

    let groups = super::group_discovered_files_for_scan_with_root(vec![file], root);

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].presentation.title, "Actual Movie");
    assert_eq!(groups[0].presentation.lookup_title, "Actual Movie");
    assert_eq!(groups[0].presentation.year, Some(2025));
    assert_eq!(groups[0].metadata_lookup_hint.as_deref(), Some("123456"));
    assert!(!groups[0].metadata_binding_conflict);
}

#[test]
fn scan_group_rejects_conflicting_container_tmdb_hints() {
    let root = Path::new("/media/series");
    let mut first_file = build_discovered_file();
    first_file.file_path = root.join("Same Show {tmdb-111}/Same.Show.S01E01.mkv");
    first_file.title = "Same Show".to_string();
    first_file.source_title = "Same Show".to_string();
    first_file.year = None;
    first_file.episode_title = None;

    let mut groups = super::group_discovered_files_for_scan_with_root(vec![first_file], root);

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].metadata_lookup_hint.as_deref(), Some("111"));
    super::merge_metadata_lookup_hint(&mut groups[0], Some("222".to_string()));

    assert_eq!(groups[0].metadata_lookup_hint, None);
    assert!(groups[0].metadata_binding_conflict);
}

#[test]
fn metadata_container_key_is_root_relative_and_skips_season_directory() {
    let root = Path::new("/media/series");
    let file_path = root.join("千香/Season 01/4K/S01E01.mkv");

    assert_eq!(
        super::metadata_container_key_for_path(&file_path, root, "series").as_deref(),
        Some("series:千香")
    );
    assert_eq!(
        super::metadata_container_key_for_path(&file_path, Path::new("/media/other"), "series"),
        None
    );
}

#[test]
fn group_discovered_files_for_scan_merges_multi_season_series_years_by_title() {
    let mut first_file = build_discovered_file();
    first_file.file_path = PathBuf::from("黑袍纠察队/Season 01/The Boys (2019) - S01E01.mkv");
    first_file.title = "The Boys".to_string();
    first_file.source_title = "The Boys".to_string();
    first_file.year = Some(2019);
    first_file.season_number = Some(1);
    first_file.episode_number = Some(1);

    let mut second_file = build_discovered_file();
    second_file.file_path = PathBuf::from("黑袍纠察队/Season 02/The Boys (2020) - S02E01.mkv");
    second_file.title = "The Boys".to_string();
    second_file.source_title = "The Boys".to_string();
    second_file.year = Some(2020);
    second_file.season_number = Some(2);
    second_file.episode_number = Some(1);

    let mut third_file = build_discovered_file();
    third_file.file_path = PathBuf::from("黑袍纠察队/Season 05/黑袍纠察队.S05E01.2026.2160p.mkv");
    third_file.title = "黑袍纠察队".to_string();
    third_file.source_title = "黑袍纠察队".to_string();
    third_file.year = None;
    third_file.season_number = Some(5);
    third_file.episode_number = Some(1);

    let groups = super::group_discovered_files_for_scan(vec![first_file, second_file, third_file]);

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].presentation.media_type, "series");
    assert_eq!(groups[0].presentation.title, "The Boys (2019)");
    assert_eq!(groups[0].presentation.lookup_title, "The Boys");
    assert_eq!(groups[0].presentation.year, Some(2019));
    assert_eq!(groups[0].presentation.item_key, "series-folder:黑袍纠察队");
    assert_eq!(groups[0].files.len(), 3);
    assert!(groups[0].files.iter().all(|file| file.year == Some(2019)));
    assert!(groups[0]
        .files
        .iter()
        .all(|file| file.source_title == "The Boys"));
}

#[test]
fn group_discovered_files_for_scan_prefers_first_season_year_as_series_year() {
    let mut later_file = build_discovered_file();
    later_file.file_path = PathBuf::from("The Boys/A Season 02/The Boys (2020) - S02E01.mkv");
    later_file.title = "The Boys".to_string();
    later_file.source_title = "The Boys".to_string();
    later_file.year = Some(2020);
    later_file.season_number = Some(2);
    later_file.episode_number = Some(1);

    let mut earlier_file = build_discovered_file();
    earlier_file.file_path = PathBuf::from("The Boys/Z Season 01/The Boys (2019) - S01E01.mkv");
    earlier_file.title = "The Boys".to_string();
    earlier_file.source_title = "The Boys".to_string();
    earlier_file.year = Some(2019);
    earlier_file.season_number = Some(1);
    earlier_file.episode_number = Some(1);

    let groups = super::group_discovered_files_for_scan(vec![later_file, earlier_file]);

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].presentation.title, "The Boys (2019)");
    assert_eq!(groups[0].presentation.lookup_title, "The Boys");
    assert_eq!(groups[0].presentation.year, Some(2019));
    assert!(groups[0].files.iter().all(|file| file.year == Some(2019)));
}

#[test]
fn group_discovered_files_for_scan_does_not_promote_later_season_year_when_s01_exists() {
    let mut first_season = build_discovered_file();
    first_season.file_path = PathBuf::from("Fallout/S01/Fallout.S01E01.mkv");
    first_season.title = "Fallout".to_string();
    first_season.source_title = "Fallout".to_string();
    first_season.year = None;
    first_season.season_number = Some(1);
    first_season.episode_number = Some(1);

    let mut second_season = build_discovered_file();
    second_season.file_path = PathBuf::from("Fallout/S02/Fallout.S02E01.2025.2160p.mkv");
    second_season.title = "Fallout".to_string();
    second_season.source_title = "Fallout".to_string();
    second_season.year = Some(2025);
    second_season.season_number = Some(2);
    second_season.episode_number = Some(1);

    let groups = super::group_discovered_files_for_scan(vec![second_season, first_season]);

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].presentation.lookup_title, "Fallout");
    assert_eq!(groups[0].presentation.year, None);
    assert_eq!(groups[0].presentation.season_air_year, None);
    assert!(groups[0].files.iter().all(|file| file.year.is_none()));
}

#[test]
fn group_discovered_files_for_scan_uses_later_season_year_only_when_s01_is_absent() {
    let mut second_season = build_discovered_file();
    second_season.file_path = PathBuf::from("Fallout/S02/Fallout.S02E01.2025.2160p.mkv");
    second_season.title = "Fallout".to_string();
    second_season.source_title = "Fallout".to_string();
    second_season.year = Some(2025);
    second_season.season_number = Some(2);
    second_season.episode_number = Some(1);

    let groups = super::group_discovered_files_for_scan(vec![second_season]);

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].presentation.lookup_title, "Fallout");
    assert_eq!(groups[0].presentation.year, None);
    assert_eq!(
        groups[0].presentation.season_air_year,
        Some(crate::metadata::MetadataSeasonAirYearHint {
            season_number: 2,
            year: 2025,
        })
    );
    assert!(groups[0].files.iter().all(|file| file.year.is_none()));
}

#[test]
fn group_discovered_files_for_scan_prefers_tvshow_nfo_identity() {
    let root = std::env::temp_dir().join(format!(
        "mova-series-nfo-{}",
        OffsetDateTime::now_utc().unix_timestamp_nanos()
    ));
    let series_root = root.join("目录标题 2030");
    let file_path = series_root
        .join("S02")
        .join("Fallback.Title.S02E01.2025.mkv");
    fs::create_dir_all(file_path.parent().unwrap()).unwrap();
    fs::write(
        series_root.join("tvshow.nfo"),
        "<tvshow><title>Authoritative Show</title><year>2021</year></tvshow>",
    )
    .unwrap();

    let mut file = build_discovered_file();
    file.file_path = file_path;
    file.title = "Fallback Title".to_string();
    file.source_title = "Fallback Title".to_string();
    super::populate_series_sidecar_metadata(&mut file, &mut std::collections::HashMap::new());
    file.year = Some(2025);
    file.season_number = Some(2);
    file.episode_number = Some(1);

    let groups = super::group_discovered_files_for_scan(vec![file]);
    let _ = fs::remove_dir_all(&root);

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].presentation.lookup_title, "Authoritative Show");
    assert_eq!(groups[0].presentation.title, "Authoritative Show");
    assert_eq!(groups[0].presentation.year, Some(2021));
    assert_eq!(groups[0].presentation.season_air_year, None);
}

#[test]
fn build_media_entries_normalizes_multi_season_series_years_before_sync() {
    let mut first_file = build_discovered_file();
    first_file.file_path = PathBuf::from("黑袍纠察队/Season 01/The Boys (2019) - S01E01.mkv");
    first_file.title = "The Boys".to_string();
    first_file.source_title = "The Boys".to_string();
    first_file.year = Some(2019);
    first_file.season_number = Some(1);
    first_file.episode_number = Some(1);

    let mut second_file = build_discovered_file();
    second_file.file_path = PathBuf::from("黑袍纠察队/Season 02/The Boys (2020) - S02E01.mkv");
    second_file.title = "The Boys".to_string();
    second_file.source_title = "The Boys".to_string();
    second_file.year = Some(2020);
    second_file.season_number = Some(2);
    second_file.episode_number = Some(1);

    let mut third_file = build_discovered_file();
    third_file.file_path = PathBuf::from("黑袍纠察队/Season 05/黑袍纠察队.S05E01.2026.2160p.mkv");
    third_file.title = "黑袍纠察队".to_string();
    third_file.source_title = "黑袍纠察队".to_string();
    third_file.year = None;
    third_file.season_number = Some(5);
    third_file.episode_number = Some(1);

    let entries = super::build_media_entries(
        &build_library(),
        vec![first_file, second_file, third_file],
        true,
        true,
        None,
        false,
    )
    .unwrap();

    assert_eq!(entries.len(), 3);
    assert!(entries.iter().all(|entry| entry.media_type == "episode"));
    assert!(entries.iter().all(|entry| entry.source_title == "The Boys"));
    assert!(entries.iter().all(|entry| entry.year == Some(2019)));
}

#[test]
fn build_media_entries_preserves_tmdb_series_title_after_local_grouping() {
    let mut file = build_discovered_file();
    file.file_path = PathBuf::from(
        "/media/overseas_tv/All's Fair (2025)/Season 01/Alls Fair (2025) - S01E01.mkv",
    );
    file.metadata_provider = Some("tmdb".to_string());
    file.metadata_provider_item_id = Some("259909".to_string());
    file.metadata_status = Some(METADATA_STATUS_MATCHED.to_string());
    file.remote_media_type = Some(REMOTE_MEDIA_TYPE_SERIES.to_string());
    file.title = "诉讼女王".to_string();
    file.source_title = "Alls Fair".to_string();
    file.original_title = Some("All's Fair".to_string());
    file.year = Some(2025);
    file.season_number = Some(1);
    file.episode_number = Some(1);

    let entries =
        super::build_media_entries(&build_library(), vec![file], true, true, None, false).unwrap();

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].media_type, "episode");
    assert_eq!(entries[0].title, "诉讼女王");
    assert_eq!(entries[0].source_title, "Alls Fair");
    assert_eq!(entries[0].original_title.as_deref(), Some("All's Fair"));
    assert_eq!(entries[0].year, Some(2025));
}

#[test]
fn build_media_entries_applies_authoritative_group_flags_and_keeps_each_episode_metadata() {
    let mut file = build_discovered_file();
    file.metadata_status = Some(METADATA_STATUS_MATCHED.to_string());
    file.metadata_provider_item_id = Some("259909".to_string());

    let mut second_file = file.clone();
    second_file.file_path = PathBuf::from("shows/example/S01E02.mkv");
    second_file.episode_number = Some(2);
    second_file.episode_title = Some("Remote episode two".to_string());
    second_file.overview = Some("Remote overview two".to_string());
    second_file.poster_path = Some("/cache/episode-two.jpg".to_string());

    let pending_entries = super::build_media_entries(
        &build_library(),
        vec![file.clone()],
        false,
        false,
        None,
        false,
    )
    .unwrap();
    assert!(!pending_entries[0].allow_artwork_clear);
    assert!(!pending_entries[0].replace_remote_data);

    let matched_entries = super::build_media_entries(
        &build_library(),
        vec![file.clone(), second_file],
        true,
        true,
        None,
        false,
    )
    .unwrap();
    assert!(matched_entries
        .iter()
        .all(|entry| entry.allow_artwork_clear));
    assert_eq!(
        matched_entries
            .iter()
            .filter(|entry| entry.replace_remote_data)
            .count(),
        1
    );
    assert_eq!(
        matched_entries[1].episode_title.as_deref(),
        Some("Remote episode two")
    );
    assert_eq!(
        matched_entries[1].overview.as_deref(),
        Some("Remote overview two")
    );
    assert_eq!(
        matched_entries[1].poster_path.as_deref(),
        Some("/cache/episode-two.jpg")
    );

    file.metadata_status = Some(METADATA_STATUS_UNMATCHED.to_string());
    file.metadata_provider_item_id = None;
    let unmatched_entries =
        super::build_media_entries(&build_library(), vec![file], true, true, None, false).unwrap();
    assert!(!unmatched_entries[0].allow_artwork_clear);
    assert!(unmatched_entries[0].replace_remote_data);
}

#[test]
fn build_pending_scan_groups_groups_series_before_full_inspection() {
    let mut first_file = build_discovered_file();
    first_file.file_path = PathBuf::from(
        "/media/overseas_tv/All's Fair (2025)/Season 01/Alls Fair (2025) - S01E01.mkv",
    );
    first_file.title = "Alls Fair (2025)".to_string();
    first_file.source_title = "Alls Fair".to_string();
    first_file.year = Some(2025);

    let mut second_file = first_file.clone();
    second_file.file_path = PathBuf::from(
        "/media/overseas_tv/All's Fair (2025)/Season 01/Alls Fair (2025) - S01E02.mkv",
    );
    second_file.episode_number = Some(2);

    let groups = super::build_pending_scan_groups_from_files(
        vec![
            build_pending_scan_file(first_file),
            build_pending_scan_file(second_file),
        ],
        Path::new("/media/overseas_tv"),
        &std::collections::HashMap::new(),
    );

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].files.len(), 2);
}

#[test]
fn pending_titleless_episode_inherits_unique_same_container_tmdb_binding() {
    let root = Path::new("/media/mainland");
    let mut file = build_discovered_file();
    file.file_path = root.join("千香/S01E02.2026.2160p.WEB-DL.H265.AAC.mp4");
    file.title = "S01E02 2026 2160p WEB DL H265 AAC".to_string();
    file.source_title = file.title.clone();
    file.year = None;
    file.episode_number = Some(2);
    file.episode_title = None;
    let container_key = super::metadata_container_key_for_path(&file.file_path, root, "series")
        .expect("series container key");
    let bindings = std::collections::HashMap::from([(
        container_key,
        super::ContainerBindingResolution::Unique("123456".to_string()),
    )]);

    let groups = super::build_pending_scan_groups_from_files(
        vec![build_pending_scan_file(file)],
        root,
        &bindings,
    );

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].metadata_lookup_hint.as_deref(), Some("123456"));
    assert!(!groups[0].metadata_binding_conflict);
}

#[test]
fn pending_named_movie_does_not_inherit_container_binding() {
    let root = Path::new("/media/movies");
    let mut file = build_discovered_file();
    file.file_path = root.join("Mixed Folder/Actual.Movie.2025.2160p.mkv");
    file.title = "Actual Movie".to_string();
    file.source_title = "Actual Movie".to_string();
    file.year = Some(2025);
    file.season_number = None;
    file.episode_number = None;
    file.episode_title = None;
    let container_key = super::metadata_container_key_for_path(&file.file_path, root, "movie")
        .expect("movie container key");
    let bindings = std::collections::HashMap::from([(
        container_key,
        super::ContainerBindingResolution::Unique("123456".to_string()),
    )]);

    let groups = super::build_pending_scan_groups_from_files(
        vec![build_pending_scan_file(file)],
        root,
        &bindings,
    );

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].metadata_lookup_hint, None);
    assert!(!groups[0].metadata_binding_conflict);
}

#[tokio::test]
async fn inspect_incremental_scan_files_shallow_ignores_stale_existing_titles() {
    let mut summary = build_existing_episode_metadata();
    summary.series_title = Some("Wrong Old Title".to_string());
    summary.series_source_title = Some("Wrong Old Title".to_string());
    summary.title = "Wrong Old Episode".to_string();
    summary.source_title = "Wrong Old Episode".to_string();

    let pending_files =
        super::inspect_incremental_scan_files_shallow(vec![super::IncrementalScanFile {
            inventory: DiscoveredMediaFileInventory {
                file_path: PathBuf::from(
                    "/media/overseas_tv/All's Fair (2025)/Season 01/Alls Fair (2025) - S01E01.mkv",
                ),
                file_size: 2048,
                file_modified_at_ms: Some(1_700_000_000_000),
                sidecar_fingerprint: String::new(),
            },
            existing_metadata: Some(summary),
        }])
        .await
        .expect("shallow inspection should parse without touching the filesystem");

    assert_eq!(pending_files.len(), 1);
    assert_eq!(pending_files[0].file.title, "Alls Fair");
    assert_eq!(pending_files[0].file.source_title, "Alls Fair");
    assert_eq!(pending_files[0].file.year, Some(2025));
    assert_eq!(
        pending_files[0]
            .changed_file
            .existing_metadata
            .as_ref()
            .and_then(|metadata| metadata.series_title.as_deref()),
        Some("Wrong Old Title")
    );
}

#[tokio::test]
async fn incremental_scan_inspection_returns_cancelled_before_touching_files() {
    let outcome = super::inspect_incremental_scan_files(
        vec![super::IncrementalScanFile {
            inventory: DiscoveredMediaFileInventory {
                file_path: PathBuf::from("/media/missing-file.mkv"),
                file_size: 2048,
                file_modified_at_ms: Some(1_700_000_000_000),
                sidecar_fingerprint: String::new(),
            },
            existing_metadata: None,
        }],
        Arc::new(AtomicBool::new(true)),
    )
    .await
    .expect("cancellation should be a normal scan outcome");

    assert!(matches!(
        outcome,
        super::InspectIncrementalScanFilesOutcome::Cancelled
    ));
}

#[test]
fn group_discovered_files_for_scan_merges_named_season_files_by_file_title() {
    let mut first_file = build_discovered_file();
    first_file.file_path =
        PathBuf::from("布里杰顿家族 (2020)/布里杰顿家族 - S01/布里杰顿家族 - S01E01.mkv");
    first_file.title = "布里杰顿家族".to_string();
    first_file.source_title = "布里杰顿家族".to_string();
    first_file.year = None;
    first_file.season_number = Some(1);
    first_file.episode_number = Some(1);

    let mut second_file = build_discovered_file();
    second_file.file_path =
        PathBuf::from("布里杰顿家族 (2020)/布里杰顿家族 - S02/布里杰顿家族 - S02E01.mkv");
    second_file.title = "布里杰顿家族".to_string();
    second_file.source_title = "布里杰顿家族".to_string();
    second_file.year = None;
    second_file.season_number = Some(2);
    second_file.episode_number = Some(1);

    let groups = super::group_discovered_files_for_scan(vec![first_file, second_file]);

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].presentation.media_type, "series");
    assert_eq!(groups[0].presentation.title, "布里杰顿家族");
    assert_eq!(groups[0].presentation.lookup_title, "布里杰顿家族");
    assert_eq!(groups[0].presentation.year, None);
    assert_eq!(groups[0].files.len(), 2);
    assert_eq!(
        groups[0].presentation.item_key,
        "series-folder:布里杰顿家族 (2020)"
    );
}

#[test]
fn group_discovered_files_for_scan_prefers_explicit_episode_file_title_over_folders() {
    let mut first_file = build_discovered_file();
    first_file.file_path = PathBuf::from("乱七八糟/版本A/我是电视剧.S01E01.mkv");
    first_file.title = "我是电视剧".to_string();
    first_file.source_title = "我是电视剧".to_string();
    first_file.year = None;
    first_file.season_number = Some(1);
    first_file.episode_number = Some(1);

    let mut second_file = build_discovered_file();
    second_file.file_path = PathBuf::from("另一个目录/完全不重要/我是电视剧.S02E01.mkv");
    second_file.title = "我是电视剧".to_string();
    second_file.source_title = "我是电视剧".to_string();
    second_file.year = None;
    second_file.season_number = Some(2);
    second_file.episode_number = Some(1);

    let groups = super::group_discovered_files_for_scan(vec![first_file, second_file]);

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].presentation.media_type, "series");
    assert_eq!(groups[0].presentation.title, "我是电视剧");
    assert_eq!(groups[0].presentation.lookup_title, "我是电视剧");
    assert_eq!(groups[0].presentation.item_key, "series-title:我是电视剧");
    assert_eq!(groups[0].files.len(), 2);
}

#[test]
fn group_discovered_files_for_scan_extracts_embedded_sxxexx_suffix_as_series() {
    let mut first_file = build_discovered_file();
    first_file.file_path = PathBuf::from("美丽毒素/S01/The.BeautyS01E01.2026.2160p.WEB-DL.mkv");
    first_file.title = "The Beauty".to_string();
    first_file.source_title = "The Beauty".to_string();
    first_file.year = Some(2026);
    first_file.season_number = Some(1);
    first_file.episode_number = Some(1);
    first_file.episode_title = None;

    let mut second_file = build_discovered_file();
    second_file.file_path = PathBuf::from("美丽毒素/S01/The.BeautyS01E02.2026.2160p.WEB-DL.mkv");
    second_file.title = "The Beauty".to_string();
    second_file.source_title = "The Beauty".to_string();
    second_file.year = Some(2026);
    second_file.season_number = Some(1);
    second_file.episode_number = Some(2);
    second_file.episode_title = None;

    let groups = super::group_discovered_files_for_scan(vec![first_file, second_file]);

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].presentation.media_type, "series");
    assert_eq!(groups[0].presentation.title, "The Beauty");
    assert_eq!(groups[0].presentation.lookup_title, "The Beauty");
    assert_eq!(groups[0].presentation.year, Some(2026));
    assert_eq!(groups[0].files.len(), 2);
}

#[test]
fn group_discovered_files_for_scan_keeps_plain_multi_file_folder_as_movies() {
    let mut first_file = build_discovered_file();
    first_file.file_path = PathBuf::from("a/aa/Pilot.mkv");
    first_file.title = "Pilot".to_string();
    first_file.source_title = "Pilot".to_string();
    first_file.season_number = None;
    first_file.episode_number = None;
    first_file.episode_title = None;

    let mut second_file = build_discovered_file();
    second_file.file_path = PathBuf::from("a/aa/Finale.mkv");
    second_file.title = "Finale".to_string();
    second_file.source_title = "Finale".to_string();
    second_file.season_number = None;
    second_file.episode_number = None;
    second_file.episode_title = None;

    let mut movie_file = build_discovered_file();
    movie_file.file_path = PathBuf::from("a/ab/How.to.Train.Your.Dragon.2025.mkv");
    movie_file.title = "How to Train Your Dragon".to_string();
    movie_file.source_title = "How to Train Your Dragon".to_string();
    movie_file.year = Some(2025);
    movie_file.season_number = None;
    movie_file.episode_number = None;
    movie_file.episode_title = None;

    let groups = super::group_discovered_files_for_scan(vec![first_file, second_file, movie_file]);

    assert_eq!(groups.len(), 3);
    assert!(groups
        .iter()
        .all(|group| group.presentation.media_type == "movie"));
    assert!(groups
        .iter()
        .any(|group| group.presentation.title == "Pilot"));
    assert!(groups
        .iter()
        .any(|group| group.presentation.title == "Finale"));
    assert!(groups
        .iter()
        .any(|group| group.presentation.title == "How to Train Your Dragon"));
}

#[test]
fn resolve_group_metadata_lookup_type_routes_files_without_episode_coordinates_to_movie() {
    let mut file = build_discovered_file();
    file.file_path = PathBuf::from("movies/Dune.2021.mkv");
    file.title = "Dune".to_string();
    file.source_title = "Dune".to_string();
    file.year = Some(2021);
    file.season_number = None;
    file.episode_number = None;
    file.episode_title = None;

    let groups = super::group_discovered_files_for_scan(vec![file]);
    let provider = FixedMetadataProvider { enabled: true };

    let decision = super::resolve_group_metadata_lookup_type(&provider, &groups[0]);

    assert_eq!(decision.lookup_type, Some("movie"));
    assert_eq!(decision.remote_media_type, None);
    assert_eq!(decision.metadata_status, METADATA_STATUS_PENDING);
    assert_eq!(
        decision.metadata_failure_reason,
        Some(METADATA_FAILURE_NO_REMOTE_MATCH)
    );
}

#[test]
fn resolve_group_metadata_lookup_type_routes_explicit_episode_coordinates_to_series() {
    let file = build_discovered_file();
    let groups = super::group_discovered_files_for_scan(vec![file]);
    let provider = FixedMetadataProvider { enabled: true };

    let decision = super::resolve_group_metadata_lookup_type(&provider, &groups[0]);

    assert_eq!(decision.lookup_type, Some("series"));
    assert_eq!(decision.remote_media_type, None);
    assert_eq!(decision.metadata_status, METADATA_STATUS_PENDING);
}

#[test]
fn resolve_group_metadata_lookup_type_trusts_existing_movie_binding() {
    let mut file = build_discovered_file();
    file.file_path = PathBuf::from("movies/Unexpected Family (2026).mkv");
    file.title = "过家家".to_string();
    file.source_title = "Unexpected Family".to_string();
    file.year = Some(2026);
    file.season_number = None;
    file.episode_number = None;
    file.episode_title = None;
    file.metadata_provider = Some(super::TMDB_PROVIDER_NAME.to_string());
    file.metadata_provider_item_id = Some("1_234_567".to_string());
    file.metadata_status = Some(METADATA_STATUS_UNMATCHED.to_string());
    file.remote_media_type = Some(REMOTE_MEDIA_TYPE_MOVIE.to_string());

    let groups = super::group_discovered_files_for_scan(vec![file]);
    let provider = FixedMetadataProvider { enabled: true };

    let decision = super::resolve_group_metadata_lookup_type(&provider, &groups[0]);

    assert_eq!(decision.lookup_type, Some("movie"));
    assert_eq!(decision.remote_media_type, Some(REMOTE_MEDIA_TYPE_MOVIE));
}

#[test]
fn resolve_group_metadata_lookup_type_skips_tmdb_when_provider_is_disabled() {
    let file = build_discovered_file();
    let groups = super::group_discovered_files_for_scan(vec![file]);
    let provider = FixedMetadataProvider { enabled: false };

    let decision = super::resolve_group_metadata_lookup_type(&provider, &groups[0]);

    assert_eq!(decision.lookup_type, Some("series"));
    assert_eq!(decision.metadata_status, METADATA_STATUS_SKIPPED);
    assert_eq!(decision.remote_media_type, None);
}

#[test]
fn disabled_provider_preserves_existing_remote_binding_and_metadata() {
    let mut file = build_discovered_file();
    file.metadata_provider = Some("tmdb".to_string());
    file.metadata_provider_item_id = Some("123".to_string());
    file.metadata_status = Some(METADATA_STATUS_PENDING.to_string());
    file.metadata_failure_reason = Some(METADATA_FAILURE_PROVIDER_ERROR.to_string());
    file.remote_media_type = Some(REMOTE_MEDIA_TYPE_SERIES.to_string());
    file.original_title = Some("Trusted Original".to_string());
    file.overview = Some("Trusted overview".to_string());
    file.poster_path = Some("/cache/trusted-poster.jpg".to_string());

    super::finalize_file_metadata_status(&mut file, false, Some(REMOTE_MEDIA_TYPE_SERIES));

    assert_eq!(
        file.metadata_status.as_deref(),
        Some(METADATA_STATUS_MATCHED)
    );
    assert_eq!(
        file.metadata_failure_reason.as_deref(),
        Some(METADATA_FAILURE_PROVIDER_ERROR)
    );
    assert_eq!(file.metadata_provider.as_deref(), Some("tmdb"));
    assert_eq!(file.metadata_provider_item_id.as_deref(), Some("123"));
    assert_eq!(
        file.remote_media_type.as_deref(),
        Some(REMOTE_MEDIA_TYPE_SERIES)
    );
    assert_eq!(file.original_title.as_deref(), Some("Trusted Original"));
    assert_eq!(file.overview.as_deref(), Some("Trusted overview"));
    assert_eq!(
        file.poster_path.as_deref(),
        Some("/cache/trusted-poster.jpg")
    );
}

#[test]
fn disabled_provider_marks_only_unbound_file_as_skipped() {
    let mut file = build_discovered_file();
    file.metadata_provider = None;
    file.metadata_provider_item_id = None;
    file.metadata_status = Some(METADATA_STATUS_PENDING.to_string());

    super::finalize_file_metadata_status(&mut file, false, Some(REMOTE_MEDIA_TYPE_SERIES));

    assert_eq!(
        file.metadata_status.as_deref(),
        Some(METADATA_STATUS_SKIPPED)
    );
    assert_eq!(
        file.metadata_failure_reason.as_deref(),
        Some(mova_domain::METADATA_FAILURE_PROVIDER_DISABLED)
    );
    assert_eq!(file.remote_media_type, None);
}

#[test]
fn clear_remote_metadata_for_review_restores_local_title_and_unbinds_remote_fields() {
    let mut file = build_discovered_file();
    file.title = "Remote Movie Title".to_string();
    file.source_title = "Local File Title".to_string();
    file.metadata_provider = Some("tmdb".to_string());
    file.metadata_provider_item_id = Some("123".to_string());
    file.original_title = Some("Remote Original".to_string());
    file.poster_path = Some("/cache/tmdb/poster.jpg".to_string());
    file.backdrop_path = Some("/cache/tmdb/backdrop.jpg".to_string());

    super::clear_remote_metadata_for_review(
        &mut file,
        METADATA_STATUS_UNMATCHED,
        Some(METADATA_FAILURE_NO_REMOTE_MATCH),
        None,
    );

    assert_eq!(file.title, "Local File Title");
    assert_eq!(file.metadata_provider, None);
    assert_eq!(file.metadata_provider_item_id, None);
    assert_eq!(
        file.metadata_status.as_deref(),
        Some(METADATA_STATUS_UNMATCHED)
    );
    assert_eq!(
        file.metadata_failure_reason.as_deref(),
        Some(METADATA_FAILURE_NO_REMOTE_MATCH)
    );
    assert_eq!(file.remote_media_type, None);
    assert_eq!(file.original_title, None);
    assert_eq!(file.poster_path, None);
    assert_eq!(file.backdrop_path, None);
}

#[test]
fn provider_error_restores_trusted_metadata_and_preserves_remote_data() {
    let mut file = build_discovered_file();
    file.metadata_provider = Some("tmdb".to_string());
    file.metadata_provider_item_id = Some("123".to_string());
    file.title = "Trusted Title".to_string();
    file.source_title = "Local Title".to_string();
    file.original_title = Some("Trusted Original".to_string());
    file.sort_title = Some("Trusted, The".to_string());
    file.country = Some("United States".to_string());
    file.genres = Some("Drama".to_string());
    file.studio = Some("Trusted Studio".to_string());
    file.overview = Some("Trusted overview".to_string());
    file.poster_path = Some("/cache/trusted-poster.jpg".to_string());
    file.backdrop_path = Some("/cache/trusted-backdrop.jpg".to_string());
    file.logo_path = Some("/cache/trusted-logo.png".to_string());
    file.external_ids = vec![MediaExternalId {
        provider: "imdb".to_string(),
        external_id: "tt123".to_string(),
    }];
    file.ratings = vec![MediaRating {
        source: "tmdb".to_string(),
        kind: "audience".to_string(),
        score: 8.4,
        scale: 10.0,
        rating_count: Some(42),
        retrieved_via: "tmdb".to_string(),
        attributes: serde_json::json!({}),
        fetched_at: OffsetDateTime::now_utc(),
    }];
    file.remote_media_type = Some(REMOTE_MEDIA_TYPE_MOVIE.to_string());
    let preserved_file = file.clone();

    // Simulate a provider pipeline that changed some fields before a later
    // request failed.
    file.metadata_provider_item_id = Some("999".to_string());
    file.title = "Partial Remote Title".to_string();
    file.original_title = None;
    file.overview = None;
    file.poster_path = None;
    file.backdrop_path = None;
    file.logo_path = None;
    file.external_ids.clear();
    file.ratings.clear();

    let mut files = vec![file];
    super::restore_group_after_provider_error(
        &mut files,
        vec![preserved_file.clone()],
        Some(REMOTE_MEDIA_TYPE_MOVIE),
    );
    let restored = &files[0];

    assert_eq!(
        restored.metadata_status.as_deref(),
        Some(METADATA_STATUS_MATCHED)
    );
    assert_eq!(
        restored.metadata_failure_reason.as_deref(),
        Some(METADATA_FAILURE_PROVIDER_ERROR)
    );
    assert_eq!(restored.metadata_provider, preserved_file.metadata_provider);
    assert_eq!(
        restored.metadata_provider_item_id,
        preserved_file.metadata_provider_item_id
    );
    assert_eq!(restored.title, preserved_file.title);
    assert_eq!(restored.original_title, preserved_file.original_title);
    assert_eq!(restored.sort_title, preserved_file.sort_title);
    assert_eq!(restored.country, preserved_file.country);
    assert_eq!(restored.genres, preserved_file.genres);
    assert_eq!(restored.studio, preserved_file.studio);
    assert_eq!(restored.overview, preserved_file.overview);
    assert_eq!(restored.poster_path, preserved_file.poster_path);
    assert_eq!(restored.backdrop_path, preserved_file.backdrop_path);
    assert_eq!(restored.logo_path, preserved_file.logo_path);
    assert_eq!(restored.external_ids, preserved_file.external_ids);
    assert_eq!(restored.ratings, preserved_file.ratings);

    let entries =
        super::build_media_entries(&build_library(), files, false, false, None, false).unwrap();
    assert_eq!(entries.len(), 1);
    assert!(!entries[0].replace_remote_data);
    assert!(!entries[0].allow_artwork_clear);
    assert_eq!(entries[0].external_ids, preserved_file.external_ids);
    assert_eq!(entries[0].ratings, preserved_file.ratings);
}

#[test]
fn provider_error_preserves_new_file_local_and_nfo_metadata() {
    let mut file = build_discovered_file();
    file.metadata_provider = None;
    file.metadata_provider_item_id = None;
    file.title = "Local NFO Title".to_string();
    file.source_title = "Local File Title".to_string();
    file.overview = Some("Local NFO overview".to_string());
    file.poster_path = Some("/media/movie/poster.jpg".to_string());
    file.backdrop_path = Some("/media/movie/fanart.jpg".to_string());
    file.metadata_status = Some(METADATA_STATUS_PENDING.to_string());
    let preserved_file = file.clone();

    file.title = "Partial Remote Title".to_string();
    file.overview = None;
    file.poster_path = None;
    file.backdrop_path = None;

    let mut files = vec![file];
    super::restore_group_after_provider_error(
        &mut files,
        vec![preserved_file],
        Some(REMOTE_MEDIA_TYPE_MOVIE),
    );
    let restored = &files[0];

    assert_eq!(restored.title, "Local NFO Title");
    assert_eq!(restored.overview.as_deref(), Some("Local NFO overview"));
    assert_eq!(
        restored.poster_path.as_deref(),
        Some("/media/movie/poster.jpg")
    );
    assert_eq!(
        restored.backdrop_path.as_deref(),
        Some("/media/movie/fanart.jpg")
    );
    assert_eq!(restored.metadata_provider, None);
    assert_eq!(restored.metadata_provider_item_id, None);
    assert_eq!(
        restored.metadata_status.as_deref(),
        Some(METADATA_STATUS_FAILED)
    );
    assert_eq!(
        restored.metadata_failure_reason.as_deref(),
        Some(METADATA_FAILURE_PROVIDER_ERROR)
    );
}

#[test]
fn provider_error_keeps_mixed_movie_group_on_the_existing_remote_identity() {
    let mut existing = build_discovered_file();
    existing.file_path = PathBuf::from("/media/Movie/Movie.1080p.mkv");
    existing.metadata_provider = Some("tmdb".to_string());
    existing.metadata_provider_item_id = Some("123".to_string());
    existing.metadata_status = Some(METADATA_STATUS_MATCHED.to_string());
    existing.remote_media_type = Some(REMOTE_MEDIA_TYPE_MOVIE.to_string());
    existing.title = "Trusted Movie".to_string();

    let mut new_version = build_discovered_file();
    new_version.file_path = PathBuf::from("/media/Movie/Movie.2160p.mkv");
    new_version.metadata_provider = None;
    new_version.metadata_provider_item_id = None;
    new_version.metadata_status = Some(METADATA_STATUS_PENDING.to_string());
    new_version.title = "Local Movie".to_string();

    let mut files = vec![existing.clone(), new_version];
    let snapshot = files.clone();
    super::restore_group_after_provider_error(&mut files, snapshot, Some(REMOTE_MEDIA_TYPE_MOVIE));

    assert_eq!(
        files[0].file_path,
        PathBuf::from("/media/Movie/Movie.2160p.mkv")
    );
    assert_eq!(files[0].metadata_provider.as_deref(), Some("tmdb"));
    assert_eq!(files[0].metadata_provider_item_id.as_deref(), Some("123"));
    assert_eq!(files[0].title, "Local Movie");
    assert_eq!(
        files[0].metadata_status.as_deref(),
        Some(METADATA_STATUS_FAILED)
    );
    assert_eq!(files[1].file_path, existing.file_path);
    assert_eq!(files[1].title, "Trusted Movie");
    assert_eq!(
        files[1].metadata_status.as_deref(),
        Some(METADATA_STATUS_MATCHED)
    );
}

#[test]
fn provider_error_keeps_new_episode_fields_and_commits_bound_series_last() {
    let mut existing_episode = build_discovered_file();
    existing_episode.file_path = PathBuf::from("/media/Show/S01E01.mkv");
    existing_episode.metadata_provider = Some("tmdb".to_string());
    existing_episode.metadata_provider_item_id = Some("456".to_string());
    existing_episode.metadata_status = Some(METADATA_STATUS_MATCHED.to_string());
    existing_episode.remote_media_type = Some(REMOTE_MEDIA_TYPE_SERIES.to_string());
    existing_episode.title = "Trusted Series".to_string();
    existing_episode.season_number = Some(1);
    existing_episode.episode_number = Some(1);
    existing_episode.episode_title = Some("Pilot".to_string());

    let mut new_episode = build_discovered_file();
    new_episode.file_path = PathBuf::from("/media/Show/S01E02.mkv");
    new_episode.metadata_provider = None;
    new_episode.metadata_provider_item_id = None;
    new_episode.metadata_status = Some(METADATA_STATUS_PENDING.to_string());
    new_episode.season_number = Some(1);
    new_episode.episode_number = Some(2);
    new_episode.episode_title = Some("Local Episode Two".to_string());

    let mut files = vec![existing_episode.clone(), new_episode];
    let snapshot = files.clone();
    super::restore_group_after_provider_error(&mut files, snapshot, Some(REMOTE_MEDIA_TYPE_SERIES));

    assert_eq!(files[0].episode_number, Some(2));
    assert_eq!(files[0].episode_title.as_deref(), Some("Local Episode Two"));
    assert_eq!(files[0].metadata_provider.as_deref(), Some("tmdb"));
    assert_eq!(files[0].metadata_provider_item_id.as_deref(), Some("456"));
    assert_eq!(
        files[0].metadata_status.as_deref(),
        Some(METADATA_STATUS_FAILED)
    );
    assert_eq!(
        files[0].metadata_failure_reason.as_deref(),
        Some(METADATA_FAILURE_PROVIDER_ERROR)
    );
    assert_eq!(files[1].file_path, existing_episode.file_path);
    assert_eq!(files[1].title, "Trusted Series");
    assert_eq!(
        files[1].metadata_status.as_deref(),
        Some(METADATA_STATUS_MATCHED)
    );

    let group = super::ScanDiscoveredGroup {
        presentation: super::ScanPresentationGroup {
            item_key: "series:show".to_string(),
            media_type: "series".to_string(),
            title: "Trusted Series".to_string(),
            lookup_title: "Trusted Series".to_string(),
            year: None,
            season_air_year: None,
        },
        files,
        metadata_lookup_hint: None,
        metadata_binding_conflict: false,
    };
    let mut summary = mova_domain::ScanNotificationSummary::default();
    super::record_scan_notification_group(
        &mut summary,
        &group,
        Some("metadata provider unavailable"),
    );
    assert_eq!(summary.matched_files, 1);
    assert_eq!(summary.failed_files, 1);
    assert_eq!(summary.issue_count, 1);
    assert_eq!(summary.issues[0].metadata_status, METADATA_STATUS_FAILED);
}

#[test]
fn no_remote_match_remains_distinct_from_provider_error() {
    let mut no_match = build_discovered_file();
    no_match.metadata_provider = None;
    no_match.metadata_provider_item_id = None;
    super::finalize_file_metadata_status(&mut no_match, true, Some(REMOTE_MEDIA_TYPE_MOVIE));

    assert_eq!(
        no_match.metadata_status.as_deref(),
        Some(METADATA_STATUS_UNMATCHED)
    );
    assert_eq!(
        no_match.metadata_failure_reason.as_deref(),
        Some(METADATA_FAILURE_NO_REMOTE_MATCH)
    );

    let mut provider_error_files = vec![build_discovered_file()];
    let preserved_files = provider_error_files.clone();
    super::restore_group_after_provider_error(
        &mut provider_error_files,
        preserved_files,
        Some(REMOTE_MEDIA_TYPE_MOVIE),
    );

    assert_eq!(
        provider_error_files[0].metadata_status.as_deref(),
        Some(METADATA_STATUS_FAILED)
    );
    assert_eq!(
        provider_error_files[0].metadata_failure_reason.as_deref(),
        Some(METADATA_FAILURE_PROVIDER_ERROR)
    );
}

#[test]
fn build_media_entries_keeps_plain_series_folder_files_as_movies() {
    let mut first_file = build_discovered_file();
    first_file.file_path = PathBuf::from("/media/Arcane/Pilot.mkv");
    first_file.title = "Pilot".to_string();
    first_file.source_title = "Pilot".to_string();
    first_file.season_number = None;
    first_file.episode_number = None;
    first_file.episode_title = None;

    let mut second_file = build_discovered_file();
    second_file.file_path = PathBuf::from("/media/Arcane/Finale.mkv");
    second_file.title = "Finale".to_string();
    second_file.source_title = "Finale".to_string();
    second_file.season_number = None;
    second_file.episode_number = None;
    second_file.episode_title = None;

    let entries = super::build_media_entries(
        &build_library(),
        vec![first_file, second_file],
        true,
        true,
        None,
        false,
    )
    .unwrap();

    assert_eq!(entries.len(), 2);
    assert!(entries.iter().all(|entry| entry.media_type == "movie"));
    assert!(entries.iter().all(|entry| entry.season_number.is_none()));
    assert!(entries.iter().all(|entry| entry.episode_number.is_none()));
    assert!(entries.iter().any(|entry| entry.title == "Finale"));
    assert!(entries.iter().any(|entry| entry.title == "Pilot"));
}

#[test]
fn group_discovered_files_for_scan_keeps_multi_version_movie_folder_as_movie() {
    let mut first_file = build_discovered_file();
    first_file.file_path = PathBuf::from("Movie/Movie.2025.1080p.mkv");
    first_file.title = "Movie".to_string();
    first_file.source_title = "Movie".to_string();
    first_file.year = Some(2025);
    first_file.season_number = None;
    first_file.episode_number = None;
    first_file.episode_title = None;

    let mut second_file = build_discovered_file();
    second_file.file_path = PathBuf::from("Movie/Movie.2025.2160p.mkv");
    second_file.title = "Movie".to_string();
    second_file.source_title = "Movie".to_string();
    second_file.year = Some(2025);
    second_file.season_number = None;
    second_file.episode_number = None;
    second_file.episode_title = None;

    let groups = super::group_discovered_files_for_scan(vec![first_file, second_file]);

    assert_eq!(groups.len(), 2);
    assert!(groups
        .iter()
        .all(|group| group.presentation.media_type == "movie"));
}

#[test]
fn apply_existing_movie_metadata_reuses_stored_remote_fields() {
    let mut file = build_discovered_file();
    file.file_path = PathBuf::from("/media/movies/Arcane.mkv");
    file.title = "Arcane.2021.2160p".to_string();
    file.source_title = "Arcane".to_string();
    file.original_title = None;
    file.overview = None;
    file.poster_path = None;
    file.backdrop_path = None;
    file.country = None;
    file.genres = None;
    file.studio = None;

    super::apply_existing_media_metadata(&mut file, &build_existing_movie_metadata());

    assert_eq!(file.title, "Arcane");
    assert_eq!(file.original_title.as_deref(), Some("Arcane Original"));
    assert_eq!(file.overview.as_deref(), Some("Stored overview"));
    assert_eq!(file.poster_path.as_deref(), Some("/cache/poster.jpg"));
    assert_eq!(file.backdrop_path.as_deref(), Some("/cache/backdrop.jpg"));
    assert_eq!(file.country.as_deref(), Some("United States"));
    assert_eq!(file.genres.as_deref(), Some("Animation, Drama"));
    assert_eq!(file.studio.as_deref(), Some("Fortiche"));
    assert_eq!(file.year, Some(2021));
}

#[test]
fn apply_existing_unmatched_metadata_keeps_fresh_local_title() {
    let mut file = build_discovered_file();
    file.file_path = PathBuf::from(
        "/media/movies/惊变28年2白骨圣殿(2026)/28.Years.Later.The.Bone.Temple.2026.mkv",
    );
    file.title = "28 Years Later The Bone Temple".to_string();
    file.source_title = "28 Years Later The Bone Temple".to_string();
    file.year = Some(2026);

    let mut existing = build_existing_movie_metadata();
    existing.metadata_provider = None;
    existing.metadata_provider_item_id = None;
    existing.metadata_status = METADATA_STATUS_UNMATCHED.to_string();
    existing.metadata_failure_reason = Some(METADATA_FAILURE_NO_REMOTE_MATCH.to_string());
    existing.title = "Years Later The Bone Temple".to_string();
    existing.source_title = "Years Later The Bone Temple".to_string();
    existing.year = Some(2026);

    super::apply_existing_media_metadata(&mut file, &existing);

    assert_eq!(file.title, "28 Years Later The Bone Temple");
    assert_eq!(file.source_title, "28 Years Later The Bone Temple");
    assert_eq!(file.year, Some(2026));
}

#[test]
fn apply_existing_episode_metadata_reuses_series_and_episode_fields() {
    let mut file = build_discovered_file();
    file.title = "Arcane.S01E01".to_string();
    file.source_title = "Arcane.S01E01".to_string();
    file.original_title = None;
    file.sort_title = None;
    file.year = Some(2020);
    file.country = None;
    file.genres = None;
    file.studio = None;
    file.overview = None;
    file.series_poster_path = None;
    file.series_backdrop_path = None;
    file.season_title = None;
    file.season_overview = None;
    file.season_poster_path = None;
    file.season_backdrop_path = None;
    file.poster_path = None;
    file.backdrop_path = None;

    super::apply_existing_media_metadata(&mut file, &build_existing_episode_metadata());

    assert_eq!(file.title, "Arcane");
    assert_eq!(file.original_title.as_deref(), Some("Arcane Original"));
    assert_eq!(file.sort_title.as_deref(), Some("Arcane, The"));
    assert_eq!(file.year, Some(2021));
    assert_eq!(file.overview.as_deref(), Some("Series overview"));
    assert_eq!(
        file.series_poster_path.as_deref(),
        Some("/cache/series-poster.jpg")
    );
    assert_eq!(file.season_title.as_deref(), Some("Season 01"));
    assert_eq!(
        file.episode_title.as_deref(),
        Some("Welcome to the Playground")
    );
    assert_eq!(
        file.poster_path.as_deref(),
        Some("/cache/episode-poster.jpg")
    );
    assert_eq!(
        file.backdrop_path.as_deref(),
        Some("/cache/episode-backdrop.jpg")
    );
}
