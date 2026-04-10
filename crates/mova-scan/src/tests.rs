use super::{
    discover::{
        discover_media_files, discover_media_files_with_progress_and_cancel,
        discover_media_files_with_progress_item_and_cancel, discover_media_paths,
        inspect_media_file,
    },
    is_likely_episode_path,
    parse::{humanize_file_stem, parse_media_metadata, ParsedMediaMetadata},
    probe::{parse_ffprobe_output, MediaProbe},
    sidecar::{parse_nfo_metadata, ParsedSidecarMetadata},
};
use std::{cell::Cell, env, fs, io::ErrorKind, path::Path, path::PathBuf};
use uuid::Uuid;

fn unique_temp_path(kind: &str) -> PathBuf {
    env::temp_dir().join(format!("mova-scan-{kind}-{}", Uuid::new_v4()))
}

#[test]
fn humanize_file_stem_replaces_common_separators() {
    let path = Path::new("The.Matrix_1999.1080p.mkv");

    assert_eq!(humanize_file_stem(path), "The Matrix 1999 1080p");
}

#[test]
fn parse_media_metadata_extracts_movie_title_and_year() {
    let path = Path::new("The.Matrix.1999.1080p.mkv");

    assert_eq!(
        parse_media_metadata(path),
        ParsedMediaMetadata {
            title: "The Matrix".to_string(),
            source_title: "The Matrix".to_string(),
            original_title: None,
            sort_title: None,
            year: Some(1999),
            season_number: None,
            season_title: None,
            season_overview: None,
            season_poster_path: None,
            season_backdrop_path: None,
            episode_number: None,
            episode_title: None,
            overview: None,
            series_poster_path: None,
            series_backdrop_path: None,
            poster_path: None,
            backdrop_path: None,
        }
    );
}

#[test]
fn parse_media_metadata_extracts_parenthesized_year() {
    let path = Path::new("创：战神 (2025).mp4");

    assert_eq!(
        parse_media_metadata(path),
        ParsedMediaMetadata {
            title: "创：战神".to_string(),
            source_title: "创：战神".to_string(),
            original_title: None,
            sort_title: None,
            year: Some(2025),
            season_number: None,
            season_title: None,
            season_overview: None,
            season_poster_path: None,
            season_backdrop_path: None,
            episode_number: None,
            episode_title: None,
            overview: None,
            series_poster_path: None,
            series_backdrop_path: None,
            poster_path: None,
            backdrop_path: None,
        }
    );
}

#[test]
fn parse_media_metadata_trims_trailing_separator_before_year() {
    let path = Path::new("新驯龙高手 - 2025.mp4");

    assert_eq!(
        parse_media_metadata(path),
        ParsedMediaMetadata {
            title: "新驯龙高手".to_string(),
            source_title: "新驯龙高手".to_string(),
            original_title: None,
            sort_title: None,
            year: Some(2025),
            season_number: None,
            season_title: None,
            season_overview: None,
            season_poster_path: None,
            season_backdrop_path: None,
            episode_number: None,
            episode_title: None,
            overview: None,
            series_poster_path: None,
            series_backdrop_path: None,
            poster_path: None,
            backdrop_path: None,
        }
    );
}

#[test]
fn parse_media_metadata_stops_before_series_token() {
    let path = Path::new("Planet.Earth.S01E02.1080p.mkv");

    assert_eq!(
        parse_media_metadata(path),
        ParsedMediaMetadata {
            title: "Planet Earth".to_string(),
            source_title: "Planet Earth".to_string(),
            original_title: None,
            sort_title: None,
            year: None,
            season_number: Some(1),
            season_title: None,
            season_overview: None,
            season_poster_path: None,
            season_backdrop_path: None,
            episode_number: Some(2),
            episode_title: None,
            overview: None,
            series_poster_path: None,
            series_backdrop_path: None,
            poster_path: None,
            backdrop_path: None,
        }
    );
}

#[test]
fn parse_media_metadata_extracts_episode_numbers_and_title() {
    let path = Path::new("Arcane.S01E02.Some.Mysteries.Are.Better.Left.Unsolved.mkv");

    assert_eq!(
        parse_media_metadata(path),
        ParsedMediaMetadata {
            title: "Arcane".to_string(),
            source_title: "Arcane".to_string(),
            original_title: None,
            sort_title: None,
            year: None,
            season_number: Some(1),
            season_title: None,
            season_overview: None,
            season_poster_path: None,
            season_backdrop_path: None,
            episode_number: Some(2),
            episode_title: Some("Some Mysteries Are Better Left Unsolved".to_string()),
            overview: None,
            series_poster_path: None,
            series_backdrop_path: None,
            poster_path: None,
            backdrop_path: None,
        }
    );
}

#[test]
fn is_likely_episode_path_detects_sxxexx_file_names() {
    assert!(is_likely_episode_path(Path::new(
        "Arcane.S01E02.Some.Title.mkv"
    )));
    assert!(is_likely_episode_path(Path::new("Severance.1x03.mp4")));
}

#[test]
fn is_likely_episode_path_detects_season_directories() {
    assert!(is_likely_episode_path(Path::new(
        "Arcane/Season 01/episode-file.mkv"
    )));
}

#[test]
fn is_likely_episode_path_keeps_movies_as_non_episode() {
    assert!(!is_likely_episode_path(Path::new(
        "Movies/How.to.Train.Your.Dragon.2025.mp4"
    )));
}

#[test]
fn parse_nfo_metadata_extracts_common_media_fields() {
    let root = unique_temp_path("nfo");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("poster.jpg"), b"poster").unwrap();
    fs::write(root.join("fanart.png"), b"fanart").unwrap();

    let metadata = parse_nfo_metadata(
        r#"
        <movie>
          <title><![CDATA[Spirited Away]]></title>
          <originaltitle>Sen to Chihiro no Kamikakushi</originaltitle>
          <sorttitle>Spirited Away</sorttitle>
          <year>2001</year>
          <plot>Chihiro enters the spirit world.</plot>
          <thumb aspect="poster">poster.jpg</thumb>
          <fanart>
            <thumb>fanart.png</thumb>
          </fanart>
        </movie>
        "#,
        &root,
    );

    assert_eq!(
        metadata,
        ParsedSidecarMetadata {
            title: Some("Spirited Away".to_string()),
            original_title: Some("Sen to Chihiro no Kamikakushi".to_string()),
            sort_title: Some("Spirited Away".to_string()),
            year: Some(2001),
            overview: Some("Chihiro enters the spirit world.".to_string()),
            poster_path: Some(root.join("poster.jpg").to_string_lossy().to_string()),
            backdrop_path: Some(root.join("fanart.png").to_string_lossy().to_string()),
        }
    );

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn parse_ffprobe_output_extracts_media_probe_fields() {
    let probe = parse_ffprobe_output(
        br#"{
            "streams": [
                {
                    "codec_type": "video",
                    "codec_name": "h264",
                    "profile": "High",
                    "level": 41,
                    "avg_frame_rate": "24000/1001",
                    "width": 1920,
                    "height": 1080,
                    "display_aspect_ratio": "16:9",
                    "field_order": "progressive",
                    "bit_rate": "4000000",
                    "pix_fmt": "yuv420p10le",
                    "color_space": "bt2020nc",
                    "color_transfer": "smpte2084",
                    "color_primaries": "bt2020",
                    "refs": 4,
                    "tags": {
                        "title": "Main Video"
                    }
                },
                {
                    "codec_type": "audio",
                    "codec_name": "aac"
                }
            ],
            "format": {
                "duration": "123.4",
                "bit_rate": "4500000"
            }
        }"#,
    )
    .unwrap();

    assert_eq!(
        probe,
        MediaProbe {
            duration_seconds: Some(123),
            video_title: Some("Main Video".to_string()),
            video_codec: Some("h264".to_string()),
            video_profile: Some("High".to_string()),
            video_level: Some("4.1".to_string()),
            audio_codec: Some("aac".to_string()),
            width: Some(1920),
            height: Some(1080),
            bitrate: Some(4_500_000),
            video_bitrate: Some(4_000_000),
            video_frame_rate: Some(23.976),
            video_aspect_ratio: Some("16:9".to_string()),
            video_scan_type: Some("Progressive".to_string()),
            video_color_primaries: Some("bt2020".to_string()),
            video_color_space: Some("bt2020nc".to_string()),
            video_color_transfer: Some("smpte2084".to_string()),
            video_bit_depth: Some(10),
            video_pixel_format: Some("yuv420p10le".to_string()),
            video_reference_frames: Some(4),
            audio_streams: Vec::new(),
            subtitle_streams: Vec::new(),
        }
    );
}

#[test]
fn parse_ffprobe_output_extracts_embedded_audio_tracks() {
    let probe = parse_ffprobe_output(
        br#"{
            "streams": [
                {
                    "index": 1,
                    "codec_type": "audio",
                    "codec_name": "aac",
                    "channels": 2,
                    "channel_layout": "stereo",
                    "bit_rate": "192000",
                    "sample_rate": "48000",
                    "tags": {
                        "language": "zh",
                        "title": "Mandarin"
                    },
                    "disposition": {
                        "default": 1,
                        "forced": 0
                    }
                },
                {
                    "index": 2,
                    "codec_type": "audio",
                    "codec_name": "ac3",
                    "channels": 6,
                    "channel_layout": "5.1(side)",
                    "bit_rate": "768000",
                    "sample_rate": "48000",
                    "tags": {
                        "language": "en",
                        "title": "English"
                    },
                    "disposition": {
                        "default": 0,
                        "forced": 0
                    }
                }
            ],
            "format": {}
        }"#,
    )
    .unwrap();

    assert_eq!(
        probe.audio_streams,
        vec![
            crate::probe::EmbeddedAudioStream {
                stream_index: 1,
                language: Some("zh".to_string()),
                audio_codec: Some("aac".to_string()),
                label: Some("Mandarin".to_string()),
                channel_layout: Some("stereo".to_string()),
                channels: Some(2),
                bitrate: Some(192_000),
                sample_rate: Some(48_000),
                is_default: true,
            },
            crate::probe::EmbeddedAudioStream {
                stream_index: 2,
                language: Some("en".to_string()),
                audio_codec: Some("ac3".to_string()),
                label: Some("English".to_string()),
                channel_layout: Some("5.1(side)".to_string()),
                channels: Some(6),
                bitrate: Some(768_000),
                sample_rate: Some(48_000),
                is_default: false,
            },
        ]
    );
}

#[test]
fn parse_ffprobe_output_extracts_embedded_subtitle_tracks() {
    let probe = parse_ffprobe_output(
        br#"{
            "streams": [
                {
                    "index": 5,
                    "codec_type": "subtitle",
                    "codec_name": "subrip",
                    "tags": {
                        "language": "en",
                        "title": "SDH"
                    },
                    "disposition": {
                        "default": 0,
                        "forced": 1,
                        "hearing_impaired": 1
                    }
                }
            ],
            "format": {}
        }"#,
    )
    .unwrap();

    assert_eq!(
        probe.subtitle_streams,
        vec![crate::probe::EmbeddedSubtitleStream {
            stream_index: 5,
            language: Some("en".to_string()),
            subtitle_format: "srt".to_string(),
            label: Some("SDH".to_string()),
            is_default: false,
            is_forced: true,
            is_hearing_impaired: true,
        }]
    );
}

#[test]
fn parse_ffprobe_output_handles_missing_fields() {
    let probe = parse_ffprobe_output(
        br#"{
            "streams": [],
            "format": {}
        }"#,
    )
    .unwrap();

    assert_eq!(probe, MediaProbe::default());
}

#[test]
fn discover_media_files_only_returns_supported_videos() {
    let root = unique_temp_path("root");
    let nested = root.join("nested");

    let result = (|| {
        fs::create_dir_all(&nested).unwrap();
        fs::write(root.join("movie.mp4"), b"video").unwrap();
        fs::write(nested.join("episode.mkv"), b"video").unwrap();
        fs::write(root.join("notes.txt"), b"ignore").unwrap();

        discover_media_files(&root)
    })();

    let _ = fs::remove_dir_all(&root);

    let files = result.unwrap();
    let discovered_names = files
        .iter()
        .map(|file| {
            file.file_path
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .to_string()
        })
        .collect::<Vec<_>>();

    assert_eq!(files.len(), 2);
    assert!(discovered_names.contains(&"movie.mp4".to_string()));
    assert!(discovered_names.contains(&"episode.mkv".to_string()));
}

#[test]
fn discover_media_paths_only_returns_supported_video_paths() {
    let root = unique_temp_path("paths");
    let nested = root.join("nested");

    let result = (|| {
        fs::create_dir_all(&nested).unwrap();
        fs::write(root.join("movie.mp4"), b"video").unwrap();
        fs::write(nested.join("episode.mkv"), b"video").unwrap();
        fs::write(root.join("movie.nfo"), b"meta").unwrap();
        fs::write(root.join("poster.jpg"), b"art").unwrap();

        discover_media_paths(&root)
    })();

    let _ = fs::remove_dir_all(&root);

    let files = result.unwrap();
    let discovered_names = files
        .iter()
        .map(|path| path.file_name().unwrap().to_str().unwrap().to_string())
        .collect::<Vec<_>>();

    assert_eq!(files.len(), 2);
    assert!(discovered_names.contains(&"movie.mp4".to_string()));
    assert!(discovered_names.contains(&"episode.mkv".to_string()));
}

#[test]
fn discover_media_files_reads_sidecar_metadata_and_artwork() {
    let root = unique_temp_path("sidecar");
    let movie_dir = root.join("spirited-away");
    let video_path = movie_dir.join("Spirited.Away.2001.mkv");

    let result = (|| {
        fs::create_dir_all(&movie_dir).unwrap();
        fs::write(&video_path, b"video").unwrap();
        fs::write(
            movie_dir.join("movie.nfo"),
            r#"
            <movie>
              <title>Spirited Away</title>
              <originaltitle>Sen to Chihiro no Kamikakushi</originaltitle>
              <plot>A young girl enters the spirit world.</plot>
            </movie>
            "#,
        )
        .unwrap();
        fs::write(movie_dir.join("poster.jpg"), b"poster").unwrap();
        fs::write(movie_dir.join("fanart.jpg"), b"fanart").unwrap();

        discover_media_files(&root)
    })();

    let _ = fs::remove_dir_all(&root);

    let files = result.unwrap();
    assert_eq!(files.len(), 1);

    let file = &files[0];
    assert_eq!(file.title, "Spirited Away");
    assert_eq!(file.source_title, "Spirited Away");
    assert_eq!(
        file.original_title.as_deref(),
        Some("Sen to Chihiro no Kamikakushi")
    );
    assert_eq!(
        file.overview.as_deref(),
        Some("A young girl enters the spirit world.")
    );
    assert_eq!(
        file.poster_path.as_deref(),
        Some(movie_dir.join("poster.jpg").to_string_lossy().as_ref())
    );
    assert_eq!(
        file.backdrop_path.as_deref(),
        Some(movie_dir.join("fanart.jpg").to_string_lossy().as_ref())
    );
}

#[test]
fn inspect_media_file_reads_sidecar_metadata_and_artwork() {
    let root = unique_temp_path("inspect");
    let video_path = root.join("Spirited.Away.2001.mkv");

    let result = (|| {
        fs::create_dir_all(&root).unwrap();
        fs::write(&video_path, b"video").unwrap();
        fs::write(
            root.join("movie.nfo"),
            r#"
            <movie>
              <title>Spirited Away</title>
              <originaltitle>Sen to Chihiro no Kamikakushi</originaltitle>
              <plot>A young girl enters the spirit world.</plot>
            </movie>
            "#,
        )
        .unwrap();
        fs::write(root.join("poster.jpg"), b"poster").unwrap();
        fs::write(root.join("fanart.jpg"), b"fanart").unwrap();

        inspect_media_file(&video_path)
    })();

    let _ = fs::remove_dir_all(&root);

    let file = result.unwrap();
    assert_eq!(file.title, "Spirited Away");
    assert_eq!(file.source_title, "Spirited Away");
    assert_eq!(
        file.original_title.as_deref(),
        Some("Sen to Chihiro no Kamikakushi")
    );
    assert_eq!(
        file.overview.as_deref(),
        Some("A young girl enters the spirit world.")
    );
    assert_eq!(
        file.poster_path.as_deref(),
        Some(root.join("poster.jpg").to_string_lossy().as_ref())
    );
    assert_eq!(
        file.backdrop_path.as_deref(),
        Some(root.join("fanart.jpg").to_string_lossy().as_ref())
    );
}

#[test]
fn discover_media_files_with_progress_and_cancel_stops_when_requested() {
    let root = unique_temp_path("cancel");

    let result = (|| {
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("movie-a.mp4"), b"video").unwrap();
        fs::write(root.join("movie-b.mp4"), b"video").unwrap();

        let cancelled = Cell::new(false);
        discover_media_files_with_progress_and_cancel(
            &root,
            |count| {
                if count >= 1 {
                    cancelled.set(true);
                }
            },
            || cancelled.get(),
        )
    })();

    let _ = fs::remove_dir_all(&root);

    assert!(matches!(
        result,
        Err(error) if error.kind() == ErrorKind::Interrupted
    ));
}

#[test]
fn discover_media_files_with_progress_item_and_cancel_emits_discovered_files() {
    let root = unique_temp_path("progress-items");

    let (result, discovered_titles) = (|| {
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("movie-a.mp4"), b"video").unwrap();
        fs::write(root.join("Show.S01E01.mkv"), b"video").unwrap();

        let discovered_titles = std::cell::RefCell::new(Vec::<String>::new());
        let result = discover_media_files_with_progress_item_and_cancel(
            &root,
            |_| {},
            |file| {
                discovered_titles.borrow_mut().push(file.title.clone());
            },
            || false,
        );

        (result, discovered_titles.into_inner())
    })();

    let _ = fs::remove_dir_all(&root);

    let files = result.unwrap();
    assert_eq!(files.len(), 2);
    assert_eq!(discovered_titles.len(), 2);
    assert!(discovered_titles
        .iter()
        .any(|title| title.to_ascii_lowercase().contains("movie")));
    assert!(discovered_titles.iter().any(|title| title.contains("Show")));
}
