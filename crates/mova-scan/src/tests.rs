use super::{
    discover::{
        discover_media_file_inventory_with_progress_and_cancel, discover_media_files,
        discover_media_files_with_progress_and_cancel,
        discover_media_files_with_progress_item_and_cancel, discover_media_paths,
        inspect_media_file, inspect_media_file_inventory_with_cancel,
        inspect_media_file_inventory_within_root_with_cancel_and_subtitle_index_and_nfo_policy,
        inspect_media_file_sidecar_only,
    },
    discovered_media_file_inventory_scan_hash, has_meaningful_file_title,
    infer_movie_container_identity, infer_series_container_identity, infer_series_file_metadata,
    infer_series_sidecar_metadata, infer_series_sidecar_metadata_within_root,
    is_likely_episode_path,
    parse::{humanize_file_stem, parse_media_metadata, ParsedMediaMetadata},
    probe::{parse_ffprobe_output, MediaProbe},
    sidecar::{
        observe_media_nfo, observe_media_nfo_for_kind, observe_media_nfo_for_kind_within_root,
        observe_nfo_file_within_root, observe_series_nfo_within_root, parse_nfo_metadata,
        LocalNfoErrorCode, LocalNfoImageKind, LocalNfoKind, LocalNfoObservation,
        LocalNfoRatingKind, MediaNfoKind,
    },
    SubtitleDirectoryIndex,
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
fn humanize_file_stem_keeps_embedded_series_tokens() {
    let path = Path::new("The.BeautyS01E01.2026.2160p.mkv");

    assert_eq!(humanize_file_stem(path), "The BeautyS01E01 2026 2160p");
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
            episode_overview: None,
            overview: None,
            series_poster_path: None,
            series_backdrop_path: None,
            poster_path: None,
            backdrop_path: None,
            local_nfo: None,
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
            episode_overview: None,
            overview: None,
            series_poster_path: None,
            series_backdrop_path: None,
            poster_path: None,
            backdrop_path: None,
            local_nfo: None,
        }
    );
}

#[test]
fn parse_media_metadata_decodes_basic_html_entities_in_file_names() {
    let path = Path::new("A.Writer&#39;s.Odyssey.2025.mkv");

    assert_eq!(
        parse_media_metadata(path),
        ParsedMediaMetadata {
            title: "A Writer's Odyssey".to_string(),
            source_title: "A Writer's Odyssey".to_string(),
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
            episode_overview: None,
            overview: None,
            series_poster_path: None,
            series_backdrop_path: None,
            poster_path: None,
            backdrop_path: None,
            local_nfo: None,
        }
    );
}

#[test]
fn parse_media_metadata_keeps_noisy_movie_file_title_without_folder_guessing() {
    let path = Path::new(
        "刺杀小说家/刺杀小说家 (2025) [4K蓝光原盘珍藏版]/A Writer&#39;s Odyssey 2 (2025) - 2160p WEB-DL HDR HQ H265 DTS.mkv",
    );

    assert_eq!(
        parse_media_metadata(path),
        ParsedMediaMetadata {
            title: "A Writer's Odyssey 2".to_string(),
            source_title: "A Writer's Odyssey 2".to_string(),
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
            episode_overview: None,
            overview: None,
            series_poster_path: None,
            series_backdrop_path: None,
            poster_path: None,
            backdrop_path: None,
            local_nfo: None,
        }
    );
}

#[test]
fn parse_media_metadata_extracts_movie_title_and_year_before_release_suffix() {
    let path = Path::new(
        "/media/movies/过家家/Unexpected Family (2026) - 2160p WEB-DL DV HQ H265 DTS 5.1.mkv",
    );

    assert_eq!(
        parse_media_metadata(path),
        ParsedMediaMetadata {
            title: "Unexpected Family".to_string(),
            source_title: "Unexpected Family".to_string(),
            original_title: None,
            sort_title: None,
            year: Some(2026),
            season_number: None,
            season_title: None,
            season_overview: None,
            season_poster_path: None,
            season_backdrop_path: None,
            episode_number: None,
            episode_title: None,
            episode_overview: None,
            overview: None,
            series_poster_path: None,
            series_backdrop_path: None,
            poster_path: None,
            backdrop_path: None,
            local_nfo: None,
        }
    );
}

#[test]
fn parse_media_metadata_does_not_collapse_collection_folder_into_one_movie_title() {
    let path = Path::new(
        "哈利波特系列八部合集/1.Harry.Potter.and.the.Sorcerer's.Stone.2001.UHD.BluRay.2160p.10bit.DoVi.4Audio.DTS-X.MA.7.1.x265-beAst.mkv",
    );

    assert_eq!(
        parse_media_metadata(path),
        ParsedMediaMetadata {
            title: "Harry Potter and the Sorcerer's Stone".to_string(),
            source_title: "Harry Potter and the Sorcerer's Stone".to_string(),
            original_title: None,
            sort_title: None,
            year: Some(2001),
            season_number: None,
            season_title: None,
            season_overview: None,
            season_poster_path: None,
            season_backdrop_path: None,
            episode_number: None,
            episode_title: None,
            episode_overview: None,
            overview: None,
            series_poster_path: None,
            series_backdrop_path: None,
            poster_path: None,
            backdrop_path: None,
            local_nfo: None,
        }
    );
}

#[test]
fn parse_media_metadata_keeps_numeric_movie_title_outside_collection_folder() {
    let metadata = parse_media_metadata(Path::new(
        "惊变28年2白骨圣殿(2026)/28.Years.Later.The.Bone.Temple.2026.2160p.mkv",
    ));

    assert_eq!(metadata.title, "28 Years Later The Bone Temple");
    assert_eq!(metadata.source_title, "28 Years Later The Bone Temple");
    assert_eq!(metadata.year, Some(2026));
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
            episode_overview: None,
            overview: None,
            series_poster_path: None,
            series_backdrop_path: None,
            poster_path: None,
            backdrop_path: None,
            local_nfo: None,
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
            episode_overview: None,
            overview: None,
            series_poster_path: None,
            series_backdrop_path: None,
            poster_path: None,
            backdrop_path: None,
            local_nfo: None,
        }
    );
}

#[test]
fn parse_media_metadata_extracts_dotted_series_file_title() {
    let path = Path::new("Taxi.Driver.S01E01.mkv");

    assert_eq!(
        parse_media_metadata(path),
        ParsedMediaMetadata {
            title: "Taxi Driver".to_string(),
            source_title: "Taxi Driver".to_string(),
            original_title: None,
            sort_title: None,
            year: None,
            season_number: Some(1),
            season_title: None,
            season_overview: None,
            season_poster_path: None,
            season_backdrop_path: None,
            episode_number: Some(1),
            episode_title: None,
            episode_overview: None,
            overview: None,
            series_poster_path: None,
            series_backdrop_path: None,
            poster_path: None,
            backdrop_path: None,
            local_nfo: None,
        }
    );
}

#[test]
fn infer_series_file_metadata_extracts_dotted_series_file_title() {
    let path = Path::new("Taxi.Driver.S01E01.mkv");
    let metadata = infer_series_file_metadata(path).expect("series file metadata should parse");

    assert_eq!(metadata.display_title, "Taxi Driver");
    assert_eq!(metadata.title, "Taxi Driver");
    assert_eq!(metadata.year, None);
}

#[test]
fn parse_media_metadata_extracts_space_separated_series_title_before_episode_marker() {
    let path = Path::new(
        "流氓读书会 (2025)/第 1 季 - 1080p WEB-DL AVC AAC/Study Group S01E01 - 第 1 集 - 1080p WEB-DL AVC AAC.mp4",
    );

    assert_eq!(
        parse_media_metadata(path),
        ParsedMediaMetadata {
            title: "Study Group".to_string(),
            source_title: "Study Group".to_string(),
            original_title: None,
            sort_title: None,
            year: None,
            season_number: Some(1),
            season_title: None,
            season_overview: None,
            season_poster_path: None,
            season_backdrop_path: None,
            episode_number: Some(1),
            episode_title: None,
            episode_overview: None,
            overview: None,
            series_poster_path: None,
            series_backdrop_path: None,
            poster_path: None,
            backdrop_path: None,
            local_nfo: None,
        }
    );
}

#[test]
fn infer_series_file_metadata_extracts_space_separated_series_title_before_episode_marker() {
    let path = Path::new(
        "流氓读书会 (2025)/第 1 季 - 1080p WEB-DL AVC AAC/Study Group S01E01 - 第 1 集 - 1080p WEB-DL AVC AAC.mp4",
    );
    let metadata = infer_series_file_metadata(path).expect("series file metadata should parse");

    assert_eq!(metadata.display_title, "Study Group");
    assert_eq!(metadata.title, "Study Group");
    assert_eq!(metadata.year, None);
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
            episode_overview: None,
            overview: None,
            series_poster_path: None,
            series_backdrop_path: None,
            poster_path: None,
            backdrop_path: None,
            local_nfo: None,
        }
    );
}

#[test]
fn parse_media_metadata_extracts_coordinates_from_episode_only_file_names() {
    for (path, season_number, episode_number) in [
        ("S01E02.mkv", 1, 2),
        ("1x03.mp4", 1, 3),
        ("Season 01/S02E04.mkv", 2, 4),
    ] {
        let metadata = parse_media_metadata(Path::new(path));

        assert_eq!(metadata.season_number, Some(season_number), "path: {path}");
        assert_eq!(
            metadata.episode_number,
            Some(episode_number),
            "path: {path}"
        );
        assert_eq!(metadata.episode_title, None, "path: {path}");
    }
}

#[test]
fn parse_media_metadata_does_not_treat_technical_suffixes_as_episode_titles() {
    for path in [
        "S01E01.WEB-DL.DV.DDP5.1.Atmos.2Audio.mkv",
        "1x02.2160p.HDR10.TrueHD.mkv",
        "Show.S01E03.4K.DoVi.mkv",
        "S01E05.NF.WEB-DL.AV1.mkv",
        "S01E06.iT.WEB-DL.HEVC.mkv",
    ] {
        let metadata = parse_media_metadata(Path::new(path));

        assert!(metadata.season_number.is_some(), "path: {path}");
        assert!(metadata.episode_number.is_some(), "path: {path}");
        assert_eq!(metadata.episode_title, None, "path: {path}");
    }

    let titled = parse_media_metadata(Path::new("S01E04.Pilot.WEB-DL.mkv"));
    assert_eq!(titled.episode_title.as_deref(), Some("Pilot"));
}

#[test]
fn parse_media_metadata_extracts_embedded_series_token_suffix() {
    let path = Path::new("美丽毒素/S01/The.BeautyS01E01.2026.2160p.WEB-DL.mkv");

    assert_eq!(
        parse_media_metadata(path),
        ParsedMediaMetadata {
            title: "The Beauty".to_string(),
            source_title: "The Beauty".to_string(),
            original_title: None,
            sort_title: None,
            year: Some(2026),
            season_number: Some(1),
            season_title: None,
            season_overview: None,
            season_poster_path: None,
            season_backdrop_path: None,
            episode_number: Some(1),
            episode_title: None,
            episode_overview: None,
            overview: None,
            series_poster_path: None,
            series_backdrop_path: None,
            poster_path: None,
            backdrop_path: None,
            local_nfo: None,
        }
    );
}

#[test]
fn parse_media_metadata_strips_dotted_year_before_series_token() {
    let path = Path::new(
        "/media/overseas_tv/都是她的错.2025/Season 01/All.Her.Fault.2025.S01E01.2160p.PCOK.WEB-DL.DDP5.1.H.265-KRATOS.mkv",
    );

    let metadata = parse_media_metadata(path);

    assert_eq!(metadata.title, "All Her Fault");
    assert_eq!(metadata.source_title, "All Her Fault");
    assert_eq!(metadata.year, Some(2025));
    assert_eq!(metadata.season_number, Some(1));
    assert_eq!(metadata.episode_number, Some(1));
}

#[test]
fn parse_media_metadata_keeps_episode_number_only_file_as_local_file() {
    let path = Path::new("Arcane/Season 01/01 Some Mysteries Are Better Left Unsolved.mkv");

    assert_eq!(
        parse_media_metadata(path),
        ParsedMediaMetadata {
            title: "Some Mysteries Are Better Left Unsolved".to_string(),
            source_title: "Some Mysteries Are Better Left Unsolved".to_string(),
            original_title: None,
            sort_title: None,
            year: None,
            season_number: None,
            season_title: None,
            season_overview: None,
            season_poster_path: None,
            season_backdrop_path: None,
            episode_number: None,
            episode_title: None,
            episode_overview: None,
            overview: None,
            series_poster_path: None,
            series_backdrop_path: None,
            poster_path: None,
            backdrop_path: None,
            local_nfo: None,
        }
    );
}

#[test]
fn parse_media_metadata_keeps_ep_prefixed_file_without_series_token_as_local_file() {
    let path = Path::new("Arcane/Season 01/EP02 Some Mysteries Are Better Left Unsolved.mkv");

    assert_eq!(
        parse_media_metadata(path),
        ParsedMediaMetadata {
            title: "EP02 Some Mysteries Are Better Left Unsolved".to_string(),
            source_title: "EP02 Some Mysteries Are Better Left Unsolved".to_string(),
            original_title: None,
            sort_title: None,
            year: None,
            season_number: None,
            season_title: None,
            season_overview: None,
            season_poster_path: None,
            season_backdrop_path: None,
            episode_number: None,
            episode_title: None,
            episode_overview: None,
            overview: None,
            series_poster_path: None,
            series_backdrop_path: None,
            poster_path: None,
            backdrop_path: None,
            local_nfo: None,
        }
    );
}

#[test]
fn parse_media_metadata_keeps_chinese_episode_file_without_series_token_as_local_file() {
    let path = Path::new("三体/Season 1/第03集 黑暗森林.mkv");

    assert_eq!(
        parse_media_metadata(path),
        ParsedMediaMetadata {
            title: "第03集 黑暗森林".to_string(),
            source_title: "第03集 黑暗森林".to_string(),
            original_title: None,
            sort_title: None,
            year: None,
            season_number: None,
            season_title: None,
            season_overview: None,
            season_poster_path: None,
            season_backdrop_path: None,
            episode_number: None,
            episode_title: None,
            episode_overview: None,
            overview: None,
            series_poster_path: None,
            series_backdrop_path: None,
            poster_path: None,
            backdrop_path: None,
            local_nfo: None,
        }
    );
}

#[test]
fn parse_media_metadata_does_not_extract_year_from_series_folder_name() {
    let path = Path::new("神雕侠侣1993/Season 01/神雕侠侣.S01E01.mp4");

    assert_eq!(
        parse_media_metadata(path),
        ParsedMediaMetadata {
            title: "神雕侠侣".to_string(),
            source_title: "神雕侠侣".to_string(),
            original_title: None,
            sort_title: None,
            year: None,
            season_number: Some(1),
            season_title: None,
            season_overview: None,
            season_poster_path: None,
            season_backdrop_path: None,
            episode_number: Some(1),
            episode_title: None,
            episode_overview: None,
            overview: None,
            series_poster_path: None,
            series_backdrop_path: None,
            poster_path: None,
            backdrop_path: None,
            local_nfo: None,
        }
    );
}

#[test]
fn parse_media_metadata_uses_file_title_for_named_season_folders() {
    let path = Path::new("布里杰顿家族 (2020)/布里杰顿家族 - S01/布里杰顿家族 - S01E01.mkv");

    assert_eq!(
        parse_media_metadata(path),
        ParsedMediaMetadata {
            title: "布里杰顿家族".to_string(),
            source_title: "布里杰顿家族".to_string(),
            original_title: None,
            sort_title: None,
            year: None,
            season_number: Some(1),
            season_title: None,
            season_overview: None,
            season_poster_path: None,
            season_backdrop_path: None,
            episode_number: Some(1),
            episode_title: None,
            episode_overview: None,
            overview: None,
            series_poster_path: None,
            series_backdrop_path: None,
            poster_path: None,
            backdrop_path: None,
            local_nfo: None,
        }
    );
}

#[test]
fn parse_media_metadata_accepts_common_separators_before_episode_token() {
    let cases = [
        "我是电视剧.S01E01.mkv",
        "我是电视剧 - S01E01.mkv",
        "我是电视剧_S01E01.mkv",
        "我是电视剧-S01E01.mkv",
        "我是电视剧—S01E01.mkv",
    ];

    for path in cases {
        let metadata = parse_media_metadata(Path::new(path));

        assert_eq!(metadata.title, "我是电视剧", "path: {path}");
        assert_eq!(metadata.source_title, "我是电视剧", "path: {path}");
        assert_eq!(metadata.season_number, Some(1), "path: {path}");
        assert_eq!(metadata.episode_number, Some(1), "path: {path}");
        assert_eq!(metadata.episode_title, None, "path: {path}");
    }
}

#[test]
fn infer_series_file_metadata_extracts_title_before_sxxexx_token() {
    let metadata =
        infer_series_file_metadata(Path::new("任何目录/随便什么文件夹/我是电视剧.S01E01.mkv"))
            .expect("series file metadata should be inferred");

    assert_eq!(metadata.display_title, "我是电视剧");
    assert_eq!(metadata.title, "我是电视剧");
    assert_eq!(metadata.season_number, 1);
    assert_eq!(metadata.year, None);
    assert_eq!(metadata.season_air_year, None);
}

#[test]
fn infer_series_file_metadata_accepts_common_episode_separators() {
    for path in [
        "任何目录/随便什么文件夹/我是电视剧 - S01E01.mkv",
        "任何目录/随便什么文件夹/我是电视剧_S01E01.mkv",
        "任何目录/随便什么文件夹/我是电视剧-S01E01.mkv",
    ] {
        let metadata =
            infer_series_file_metadata(Path::new(path)).expect("series file metadata should exist");

        assert_eq!(metadata.display_title, "我是电视剧", "path: {path}");
        assert_eq!(metadata.title, "我是电视剧", "path: {path}");
        assert_eq!(metadata.season_number, 1, "path: {path}");
        assert_eq!(metadata.year, None, "path: {path}");
        assert_eq!(metadata.season_air_year, None, "path: {path}");
    }
}

#[test]
fn infer_series_file_metadata_extracts_year_before_sxxexx_token() {
    let metadata = infer_series_file_metadata(Path::new("任何目录/Alls Fair (2025) - S01E01.mkv"))
        .expect("series file metadata should be inferred");

    assert_eq!(metadata.display_title, "Alls Fair (2025)");
    assert_eq!(metadata.title, "Alls Fair");
    assert_eq!(metadata.season_number, 1);
    assert_eq!(metadata.year, Some(2025));
    assert_eq!(metadata.season_air_year, None);
}

#[test]
fn infer_series_file_metadata_extracts_dotted_year_before_sxxexx_token() {
    let metadata = infer_series_file_metadata(Path::new(
        "/media/overseas_tv/都是她的错.2025/Season 01/All.Her.Fault.2025.S01E01.2160p.PCOK.WEB-DL.DDP5.1.H.265-KRATOS.mkv",
    ))
    .expect("series file metadata should be inferred");

    assert_eq!(metadata.display_title, "All Her Fault 2025");
    assert_eq!(metadata.title, "All Her Fault");
    assert_eq!(metadata.season_number, 1);
    assert_eq!(metadata.year, Some(2025));
    assert_eq!(metadata.season_air_year, None);
}

#[test]
fn infer_series_file_metadata_extracts_embedded_sxxexx_suffix() {
    let metadata = infer_series_file_metadata(Path::new(
        "任何目录/随便什么文件夹/The.BeautyS01E01.2026.mkv",
    ))
    .expect("embedded SxxExx suffix should still expose a series title");

    assert_eq!(metadata.display_title, "The Beauty");
    assert_eq!(metadata.title, "The Beauty");
    assert_eq!(metadata.season_number, 1);
    assert_eq!(metadata.year, Some(2026));
    assert_eq!(metadata.season_air_year, None);
}

#[test]
fn infer_series_file_metadata_keeps_later_season_year_separate() {
    let metadata = infer_series_file_metadata(Path::new(
        "/media/overseas_tv/Fallout/S02/Fallout.S02E01.2025.2160p.mkv",
    ))
    .expect("later-season metadata should be inferred");

    assert_eq!(metadata.title, "Fallout");
    assert_eq!(metadata.season_number, 2);
    assert_eq!(metadata.year, None);
    assert_eq!(metadata.season_air_year, Some(2025));
}

#[test]
fn infer_series_sidecar_metadata_has_priority_fields_without_using_directories() {
    let root = unique_temp_path("series-sidecar-identity");
    let video_path = root
        .join("错误目录 2030")
        .join("Fallback.Title.S02E01.2025.mkv");

    let result = {
        fs::create_dir_all(video_path.parent().unwrap()).unwrap();
        fs::write(&video_path, b"video").unwrap();
        fs::write(
            video_path.parent().unwrap().join("tvshow.nfo"),
            r#"<tvshow>
                <title>Authoritative Show</title>
                <year>2021</year>
                <genre>Drama</genre>
                <uniqueid type="tmdb" default="true">31415</uniqueid>
            </tvshow>"#,
        )
        .unwrap();

        infer_series_sidecar_metadata(&video_path)
    };

    let _ = fs::remove_dir_all(&root);

    let metadata = result.expect("sidecar identity should be inferred");
    assert_eq!(metadata.title.as_deref(), Some("Authoritative Show"));
    assert_eq!(metadata.year, Some(2021));
    assert_eq!(metadata.local_nfo.kind, LocalNfoKind::TvShow);
    assert_eq!(metadata.local_nfo.genres, vec!["Drama"]);
    assert_eq!(metadata.local_nfo.unique_ids[0].provider, "tmdb");
    assert!(metadata.local_nfo.source_path.ends_with("tvshow.nfo"));
}

#[test]
fn infer_series_sidecar_metadata_within_root_never_reads_parent_library_nfo() {
    let outer = unique_temp_path("series-sidecar-root-boundary");
    let library_root = outer.join("library");
    let series_root = library_root.join("千香 (2026)");
    let video_path = series_root.join("Season 01").join("S01E01.mkv");

    fs::create_dir_all(video_path.parent().unwrap()).unwrap();
    fs::write(
        outer.join("tvshow.nfo"),
        "<tvshow><title>Outside Root</title><year>1999</year></tvshow>",
    )
    .unwrap();

    assert_eq!(
        infer_series_sidecar_metadata_within_root(&video_path, &library_root),
        None
    );

    fs::write(
        series_root.join("tvshow.nfo"),
        "<tvshow><title>Inside Root</title><year>2026</year></tvshow>",
    )
    .unwrap();
    let metadata = infer_series_sidecar_metadata_within_root(&video_path, &library_root)
        .expect("inside-root tvshow.nfo should resolve");

    let _ = fs::remove_dir_all(&outer);
    assert_eq!(metadata.title.as_deref(), Some("Inside Root"));
    assert_eq!(metadata.year, Some(2026));
}

#[test]
fn media_nfo_observation_does_not_fall_back_after_invalid_specific_candidate() {
    for (file_name, invalid_root, valid_generic_root) in [
        (
            "Example.Movie.2026.mkv",
            "<movie><title>broken",
            "<movie><title>Generic movie</title></movie>",
        ),
        (
            "Example.Show.S01E01.mkv",
            "<episodedetails><title>broken",
            "<episodedetails><title>Generic episode</title></episodedetails>",
        ),
    ] {
        let root = unique_temp_path("nfo-specific-invalid");
        let video_path = root.join(file_name);
        let specific_path = video_path.with_extension("nfo");
        fs::create_dir_all(&root).unwrap();
        fs::write(&specific_path, invalid_root).unwrap();
        fs::write(root.join("movie.nfo"), valid_generic_root).unwrap();

        let observation = observe_media_nfo(&video_path);

        let _ = fs::remove_dir_all(&root);
        assert_eq!(
            observation,
            LocalNfoObservation::Invalid {
                candidate_path: specific_path,
                error_code: LocalNfoErrorCode::MalformedXml,
            }
        );
    }
}

#[test]
fn absent_media_nfo_reports_candidates_in_lookup_order() {
    let root = unique_temp_path("nfo-absent-candidates");
    let video_path = root.join("Example.Movie.2026.mkv");

    assert_eq!(
        observe_media_nfo(&video_path),
        LocalNfoObservation::Absent {
            candidate_paths: vec![video_path.with_extension("nfo"), root.join("movie.nfo"),],
        }
    );
}

#[test]
fn episode_nfo_observation_never_uses_generic_movie_nfo() {
    let root = unique_temp_path("episode-nfo-no-generic-fallback");
    let video_path = root.join("Example.Show.S01E01.mkv");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("movie.nfo"),
        "<episodedetails><title>Must not be selected</title></episodedetails>",
    )
    .unwrap();

    let observation = observe_media_nfo(&video_path);

    let _ = fs::remove_dir_all(&root);
    assert_eq!(
        observation,
        LocalNfoObservation::Absent {
            candidate_paths: vec![video_path.with_extension("nfo")],
        }
    );
}

#[test]
fn explicit_episode_nfo_scope_does_not_depend_on_filename_shape() {
    let root = unique_temp_path("episode-nfo-explicit-kind");
    let video_path = root.join("Unexpected.Name.mkv");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("movie.nfo"),
        "<movie><title>Must not be selected</title></movie>",
    )
    .unwrap();

    assert_eq!(
        observe_media_nfo_for_kind(&video_path, MediaNfoKind::Episode),
        LocalNfoObservation::Absent {
            candidate_paths: vec![video_path.with_extension("nfo")],
        }
    );

    fs::write(
        video_path.with_extension("nfo"),
        "<episodedetails><title>Explicit episode</title></episodedetails>",
    )
    .unwrap();
    let observation = observe_media_nfo_for_kind(&video_path, MediaNfoKind::Episode);

    let _ = fs::remove_dir_all(&root);
    let LocalNfoObservation::Valid(metadata) = observation else {
        panic!("expected explicit episode NFO to be selected");
    };
    assert_eq!(metadata.kind, LocalNfoKind::Episode);
    assert_eq!(metadata.title.as_deref(), Some("Explicit episode"));
}

#[test]
fn non_regular_higher_priority_nfo_is_invalid_instead_of_falling_back() {
    let root = unique_temp_path("nfo-non-regular-candidate");
    let video_path = root.join("Example.Movie.2026.mkv");
    let specific_path = video_path.with_extension("nfo");
    fs::create_dir_all(&specific_path).unwrap();
    fs::write(
        root.join("movie.nfo"),
        "<movie><title>Must not be selected</title></movie>",
    )
    .unwrap();

    let observation = observe_media_nfo(&video_path);

    let _ = fs::remove_dir_all(&root);
    assert_eq!(
        observation,
        LocalNfoObservation::Invalid {
            candidate_path: specific_path,
            error_code: LocalNfoErrorCode::NotRegularFile,
        }
    );
}

#[test]
fn series_nfo_observation_reports_nearest_invalid_candidate_without_fallback() {
    let outer = unique_temp_path("series-nfo-invalid-nearest");
    let library_root = outer.join("library");
    let series_root = library_root.join("Example Show");
    let season_root = series_root.join("Season 01");
    let video_path = season_root.join("Example.Show.S01E01.mkv");
    let nearest_path = season_root.join("tvshow.nfo");
    fs::create_dir_all(&season_root).unwrap();
    fs::write(&nearest_path, "<tvshow><title>broken").unwrap();
    fs::write(
        series_root.join("tvshow.nfo"),
        "<tvshow><title>Valid but lower priority</title></tvshow>",
    )
    .unwrap();
    fs::write(
        outer.join("tvshow.nfo"),
        "<tvshow><title>Outside root</title></tvshow>",
    )
    .unwrap();

    let observation = observe_series_nfo_within_root(&video_path, &library_root);

    let _ = fs::remove_dir_all(&outer);
    assert_eq!(
        observation,
        LocalNfoObservation::Invalid {
            candidate_path: nearest_path,
            error_code: LocalNfoErrorCode::MalformedXml,
        }
    );
}

#[test]
fn series_nfo_absence_stays_within_library_root() {
    let outer = unique_temp_path("series-nfo-absent-boundary");
    let library_root = outer.join("library");
    let video_path = library_root
        .join("Example Show")
        .join("Season 01")
        .join("Example.Show.S01E01.mkv");
    fs::create_dir_all(video_path.parent().unwrap()).unwrap();

    let observation = observe_series_nfo_within_root(&video_path, &library_root);

    let _ = fs::remove_dir_all(&outer);
    let LocalNfoObservation::Absent { candidate_paths } = observation else {
        panic!("expected absent series NFO observation");
    };
    assert_eq!(
        candidate_paths.last(),
        Some(&library_root.join("tvshow.nfo"))
    );
    assert!(candidate_paths
        .iter()
        .all(|candidate| candidate.starts_with(&library_root)));
    assert!(!candidate_paths.contains(&outer.join("tvshow.nfo")));
}

#[cfg(unix)]
#[test]
fn root_aware_media_nfo_observation_rejects_symlink_to_outside_library() {
    use std::os::unix::fs::symlink;

    let outer = unique_temp_path("nfo-symlink-outside-library");
    let library_root = outer.join("library");
    let video_path = library_root.join("Example.Movie.2026.mkv");
    let nfo_path = video_path.with_extension("nfo");
    let outside_nfo_path = outer.join("outside.nfo");
    fs::create_dir_all(&library_root).unwrap();
    fs::write(&outside_nfo_path, "<movie><title>Outside</title></movie>").unwrap();
    symlink(&outside_nfo_path, &nfo_path).unwrap();

    let observation =
        observe_media_nfo_for_kind_within_root(&video_path, MediaNfoKind::Movie, &library_root);

    let _ = fs::remove_dir_all(&outer);
    assert_eq!(
        observation,
        LocalNfoObservation::Invalid {
            candidate_path: nfo_path,
            error_code: LocalNfoErrorCode::SymlinkNotAllowed,
        }
    );
}

#[test]
fn persisted_nfo_observation_rejects_a_path_outside_its_library_root() {
    let outer = unique_temp_path("persisted-nfo-outside-library");
    let library_root = outer.join("library");
    let outside_nfo_path = outer.join("outside.nfo");
    fs::create_dir_all(&library_root).unwrap();
    fs::write(&outside_nfo_path, "<movie><title>Outside</title></movie>").unwrap();

    let observation =
        observe_nfo_file_within_root(&outside_nfo_path, LocalNfoKind::Movie, &library_root);

    let _ = fs::remove_dir_all(&outer);
    assert_eq!(
        observation,
        LocalNfoObservation::Invalid {
            candidate_path: outside_nfo_path,
            error_code: LocalNfoErrorCode::OutsideLibraryRoot,
        }
    );
}

#[test]
fn root_aware_series_nfo_observation_searches_at_most_five_ancestors() {
    let outer = unique_temp_path("series-nfo-five-ancestor-limit");
    let library_root = outer.join("library");
    let fifth_candidate_dir = library_root.join("level-5");
    let video_path = fifth_candidate_dir
        .join("level-4")
        .join("level-3")
        .join("level-2")
        .join("level-1")
        .join("Example.Show.S01E01.mkv");
    let fifth_candidate_path = fifth_candidate_dir.join("tvshow.nfo");
    let sixth_candidate_path = library_root.join("tvshow.nfo");
    fs::create_dir_all(video_path.parent().unwrap()).unwrap();
    fs::write(
        &fifth_candidate_path,
        "<tvshow><title>Fifth candidate</title></tvshow>",
    )
    .unwrap();
    let canonical_fifth_candidate_path = fs::canonicalize(&fifth_candidate_path).unwrap();

    let fifth_observation = observe_series_nfo_within_root(&video_path, &library_root);
    assert!(
        matches!(
        &fifth_observation,
        LocalNfoObservation::Valid(metadata)
            if metadata.source_path == canonical_fifth_candidate_path
                && metadata.title.as_deref() == Some("Fifth candidate")
        ),
        "unexpected fifth-candidate observation: {fifth_observation:?}"
    );

    fs::remove_file(&fifth_candidate_path).unwrap();
    fs::write(
        &sixth_candidate_path,
        "<tvshow><title>Sixth candidate</title></tvshow>",
    )
    .unwrap();
    let sixth_observation = observe_series_nfo_within_root(&video_path, &library_root);

    let _ = fs::remove_dir_all(&outer);
    let LocalNfoObservation::Absent { candidate_paths } = sixth_observation else {
        panic!("a tvshow.nfo beyond the five-candidate boundary must not be observed");
    };
    assert_eq!(candidate_paths.len(), 5);
    assert!(!candidate_paths.contains(&sixth_candidate_path));
}

#[test]
fn infer_series_file_metadata_ignores_episode_only_file_names() {
    assert!(infer_series_file_metadata(Path::new("V世代 (2023)/Season 01/S01E01.mkv")).is_none());
}

#[test]
fn meaningful_file_title_distinguishes_titles_from_coordinates_and_release_tokens() {
    assert!(has_meaningful_file_title(Path::new(
        "Arcane.S01E01.2160p.mkv"
    )));
    assert!(has_meaningful_file_title(Path::new("Dune.2021.2160p.mkv")));
    assert!(!has_meaningful_file_title(Path::new("S01E01.mkv")));
    assert!(!has_meaningful_file_title(Path::new("1x02.WEB-DL.mkv")));
    assert!(!has_meaningful_file_title(Path::new(
        "2026.2160p.iT.WEB-DL.DV.mkv"
    )));
}

#[test]
fn infer_series_container_identity_skips_only_known_structure_directories() {
    let root = Path::new("/media/tv");
    let path = Path::new("/media/tv/国产剧/千香(2026)/Season 01/4K/S01E01.WEB-DL.DV.mkv");
    let identity =
        infer_series_container_identity(path, root).expect("series container should resolve");

    assert_eq!(identity.container_path, PathBuf::from("国产剧/千香(2026)"));
    assert_eq!(identity.display_title, "千香(2026)");
    assert_eq!(identity.title, "千香");
    assert_eq!(identity.year, Some(2026));
    assert_eq!(identity.tmdb_id, None);
}

#[test]
fn infer_series_container_identity_parses_supported_tmdb_suffixes() {
    for (directory, expected_id) in [
        ("千香 (2026) [tmdbid-123456]", "123456"),
        ("千香 (2026) {tmdb-654321}", "654321"),
    ] {
        let path = PathBuf::from("/media/tv")
            .join(directory)
            .join("第 1 季 - 1080p WEB-DL")
            .join("S01E01.mkv");
        let identity = infer_series_container_identity(&path, Path::new("/media/tv"))
            .expect("explicit identity container should resolve");

        assert_eq!(identity.container_path, PathBuf::from(directory));
        assert_eq!(identity.display_title, "千香 (2026)");
        assert_eq!(identity.title, "千香");
        assert_eq!(identity.year, Some(2026));
        assert_eq!(identity.tmdb_id.as_deref(), Some(expected_id));
    }
}

#[test]
fn infer_series_container_identity_ignores_invalid_tmdb_suffixes() {
    for directory in [
        "千香 (2026) [tmdbid-0]",
        "千香 (2026) {tmdb--1}",
        "千香 (2026) {tmdb-not-a-number}",
    ] {
        let path = PathBuf::from("/media/tv")
            .join(directory)
            .join("S01E01.mkv");
        let identity = infer_series_container_identity(&path, Path::new("/media/tv"))
            .expect("the container title remains valid");

        assert_eq!(identity.tmdb_id, None);
    }
}

#[test]
fn infer_series_container_identity_stops_at_first_invalid_business_directory() {
    let path = Path::new("/media/tv/千香/新建文件夹/Season 01/S01E01.mkv");

    assert!(infer_series_container_identity(path, Path::new("/media/tv")).is_none());
}

#[test]
fn infer_series_container_identity_never_uses_or_crosses_library_root() {
    assert!(infer_series_container_identity(
        Path::new("/media/tv/千香/Season 01/S01E01.mkv"),
        Path::new("/media/tv/千香"),
    )
    .is_none());
    assert!(infer_series_container_identity(
        Path::new("/media/other/千香/Season 01/S01E01.mkv"),
        Path::new("/media/tv"),
    )
    .is_none());
}

#[test]
fn infer_movie_container_identity_uses_only_the_direct_parent() {
    let path = Path::new(
        "/media/movies/星球大战曼达洛人与古古(2026)/2026.2160p.iT.WEB-DL.DV.DDP5.1.Atmos.2Audio.mkv",
    );
    let identity = infer_movie_container_identity(path, Path::new("/media/movies"))
        .expect("titleless movie container should resolve");

    assert_eq!(
        identity.container_path,
        PathBuf::from("星球大战曼达洛人与古古(2026)")
    );
    assert_eq!(identity.display_title, "星球大战曼达洛人与古古(2026)");
    assert_eq!(identity.title, "星球大战曼达洛人与古古");
    assert_eq!(identity.year, Some(2026));
    assert_eq!(identity.tmdb_id, None);
    assert!(!has_meaningful_file_title(path));

    assert!(infer_movie_container_identity(
        Path::new("/media/movies/正确标题/4K/2026.2160p.mkv"),
        Path::new("/media/movies"),
    )
    .is_none());
    assert!(infer_movie_container_identity(
        Path::new("/media/movies/2026.2160p.mkv"),
        Path::new("/media/movies"),
    )
    .is_none());
}

#[test]
fn is_likely_episode_path_detects_sxxexx_file_names() {
    assert!(is_likely_episode_path(Path::new(
        "Arcane.S01E02.Some.Title.mkv"
    )));
    assert!(is_likely_episode_path(Path::new("Severance.1x03.mp4")));
    assert!(is_likely_episode_path(Path::new("S01E01.mkv")));
    assert!(is_likely_episode_path(Path::new("1x02.mp4")));
}

#[test]
fn is_likely_episode_path_uses_file_name_episode_signal_not_season_directory_only() {
    assert!(is_likely_episode_path(Path::new(
        "美丽毒素/S01/The.BeautyS01E01.2026.mkv"
    )));
    assert!(!is_likely_episode_path(Path::new(
        "Arcane/Season 01/episode-file.mkv"
    )));
}

#[test]
fn is_likely_episode_path_ignores_episode_numbers_inside_season_directories() {
    assert!(!is_likely_episode_path(Path::new(
        "Arcane/Season 01/01.mkv"
    )));
    assert!(!is_likely_episode_path(Path::new(
        "Arcane/Season 01/EP02.mkv"
    )));
    assert!(!is_likely_episode_path(Path::new(
        "三体/Season 1/第03集.mkv"
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
    fs::write(root.join("poster.jpg"), b"\xff\xd8\xffposter").unwrap();
    fs::write(root.join("fanart.png"), b"\x89PNG\r\n\x1a\nfanart").unwrap();

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
    )
    .expect("valid movie NFO should parse");
    let canonical_root = root.canonicalize().unwrap();

    assert_eq!(metadata.kind, LocalNfoKind::Movie);
    assert_eq!(metadata.title.as_deref(), Some("Spirited Away"));
    assert_eq!(
        metadata.original_title.as_deref(),
        Some("Sen to Chihiro no Kamikakushi")
    );
    assert_eq!(metadata.sort_title.as_deref(), Some("Spirited Away"));
    assert_eq!(metadata.year, Some(2001));
    assert_eq!(
        metadata.overview.as_deref(),
        Some("Chihiro enters the spirit world.")
    );
    assert_eq!(
        metadata.artwork.posters,
        vec![canonical_root
            .join("poster.jpg")
            .to_string_lossy()
            .to_string()]
    );
    assert_eq!(
        metadata.artwork.backdrops,
        vec![canonical_root
            .join("fanart.png")
            .to_string_lossy()
            .to_string()]
    );

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn parse_nfo_metadata_extracts_emby_kodi_structured_fields() {
    let root = unique_temp_path("nfo-structured");
    fs::create_dir_all(&root).unwrap();

    let metadata = parse_nfo_metadata(
        r#"
        <movie>
          <title>Example Movie</title>
          <originaltitle>Original Example</originaltitle>
          <sorttitle>Example, The</sorttitle>
          <year>2026</year>
          <plot>Full plot.</plot>
          <outline>Short outline.</outline>
          <tagline>A local-first story.</tagline>
          <status>Released</status>
          <premiered>2026-05-06</premiered>
          <runtime>123.4</runtime>
          <mpaa>PG-13</mpaa>
          <original_language>zh</original_language>
          <genre>Drama</genre>
          <genre>Science Fiction</genre>
          <country>China</country>
          <country>United States</country>
          <studio>Studio One</studio>
          <tag>Featured</tag>
          <director>Director One</director>
          <credits>Writer One</credits>
          <writer>Writer Two</writer>
          <actor>
            <name>Actor One</name>
            <role>Lead</role>
            <order>2</order>
            <thumb>https://image.tmdb.org/t/p/original/actor.jpg</thumb>
          </actor>
          <uniqueid type="themoviedb" default="true">12345</uniqueid>
          <uniqueid type="imdb">tt1234567</uniqueid>
          <tvdbid>9876</tvdbid>
          <ratings>
            <rating name="themoviedb" max="10" default="true">
              <value>8.4</value>
              <votes>12,345</votes>
            </rating>
          </ratings>
          <thumb aspect="poster">https://image.tmdb.org/t/p/original/poster.jpg</thumb>
          <fanart><thumb>https://image.tmdb.org/t/p/original/backdrop.jpg</thumb></fanart>
          <clearlogo>https://image.tmdb.org/t/p/original/logo.png</clearlogo>
          <set>
            <name>Example Collection</name>
            <overview>Collection overview.</overview>
            <uniqueid type="tmdb">77</uniqueid>
          </set>
          <lockdata>true</lockdata>
          <lockedfields>title | plot</lockedfields>
        </movie>
        "#,
        &root,
    )
    .expect("structured movie NFO should parse");

    assert_eq!(metadata.kind, LocalNfoKind::Movie);
    assert_eq!(metadata.title.as_deref(), Some("Example Movie"));
    assert_eq!(metadata.overview.as_deref(), Some("Full plot."));
    assert_eq!(metadata.outline.as_deref(), Some("Short outline."));
    assert_eq!(metadata.tagline.as_deref(), Some("A local-first story."));
    assert_eq!(metadata.status.as_deref(), Some("Released"));
    assert_eq!(metadata.premiered.as_deref(), Some("2026-05-06"));
    assert_eq!(metadata.runtime_minutes, Some(123));
    assert_eq!(metadata.content_rating.as_deref(), Some("PG-13"));
    assert_eq!(metadata.original_language.as_deref(), Some("zh"));
    assert_eq!(metadata.genres, vec!["Drama", "Science Fiction"]);
    assert_eq!(metadata.countries, vec!["China", "United States"]);
    assert_eq!(metadata.studios, vec!["Studio One"]);
    assert_eq!(metadata.tags, vec!["Featured"]);
    assert_eq!(metadata.credits.directors, vec!["Director One"]);
    assert_eq!(metadata.credits.writers, vec!["Writer One", "Writer Two"]);
    assert_eq!(metadata.credits.actors.len(), 1);
    assert_eq!(metadata.credits.actors[0].name, "Actor One");
    assert_eq!(metadata.credits.actors[0].role.as_deref(), Some("Lead"));
    assert_eq!(metadata.credits.actors[0].order, Some(2));
    assert_eq!(metadata.unique_ids.len(), 3);
    assert_eq!(metadata.unique_ids[0].provider, "tmdb");
    assert!(metadata.unique_ids[0].is_default);
    assert_eq!(metadata.ratings.len(), 1);
    assert_eq!(metadata.ratings[0].source, "tmdb");
    assert_eq!(metadata.ratings[0].kind, LocalNfoRatingKind::Audience);
    assert_eq!(metadata.ratings[0].value, 8.4);
    assert_eq!(metadata.ratings[0].votes, Some(12_345));
    assert_eq!(metadata.artwork.posters.len(), 1);
    assert_eq!(metadata.artwork.backdrops.len(), 1);
    assert_eq!(metadata.artwork.logos.len(), 1);
    assert_eq!(
        metadata
            .collection
            .as_ref()
            .and_then(|collection| collection.name.as_deref()),
        Some("Example Collection")
    );
    assert!(metadata.lock_data);
    assert_eq!(metadata.locked_fields, vec!["title", "plot"]);

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn parse_nfo_metadata_keeps_legacy_ids_and_separates_language_and_rating_semantics() {
    let metadata = parse_nfo_metadata(
        r#"
        <movie>
          <customrating>Family profile</customrating>
          <language>zh-CN</language>
          <originallanguage>ja</originallanguage>
          <dateadded>2026-08-01 12:34:56</dateadded>
          <id TMDB="101" TVDB="202" IMDB="tt303">tt303</id>
          <trailer>https://example.invalid/one</trailer>
          <trailer>https://example.invalid/two</trailer>
          <showlink>Example Show</showlink>
          <ratings>
            <rating name="tomatometerallcritics" max="100"><value>91</value></rating>
            <rating name="tomatometerallaudience" max="100"><value>87</value></rating>
            <rating name="metacritic" max="100"><value>72</value></rating>
            <rating name="imdb" max="10"><value>8.1</value></rating>
          </ratings>
          <set tmdbcolid="404"><name>Example Collection</name></set>
        </movie>
        "#,
        Path::new("/media"),
    )
    .expect("legacy Emby-compatible NFO fields should parse");

    assert_eq!(metadata.content_rating, None);
    assert_eq!(metadata.custom_rating.as_deref(), Some("Family profile"));
    assert_eq!(metadata.original_language.as_deref(), Some("ja"));
    assert_eq!(
        metadata.preferred_metadata_language.as_deref(),
        Some("zh-CN")
    );
    assert_eq!(metadata.date_added.as_deref(), Some("2026-08-01 12:34:56"));
    assert_eq!(metadata.trailers.len(), 2);
    assert_eq!(metadata.show_link.as_deref(), Some("Example Show"));
    assert!(metadata
        .unique_ids
        .iter()
        .any(|id| id.provider == "tmdb" && id.value == "101"));
    assert!(metadata
        .unique_ids
        .iter()
        .any(|id| id.provider == "tvdb" && id.value == "202"));
    assert!(metadata
        .unique_ids
        .iter()
        .any(|id| id.provider == "imdb" && id.value == "tt303"));
    assert_eq!(
        metadata
            .ratings
            .iter()
            .find(|rating| rating.source == "tomatometerallcritics")
            .map(|rating| rating.kind),
        Some(LocalNfoRatingKind::Critic)
    );
    assert_eq!(
        metadata
            .ratings
            .iter()
            .find(|rating| rating.source == "tomatometerallaudience")
            .map(|rating| rating.kind),
        Some(LocalNfoRatingKind::Audience)
    );
    assert_eq!(
        metadata
            .ratings
            .iter()
            .find(|rating| rating.source == "metacritic")
            .map(|rating| rating.kind),
        Some(LocalNfoRatingKind::Critic)
    );
    assert_eq!(
        metadata
            .ratings
            .iter()
            .find(|rating| rating.source == "imdb")
            .map(|rating| rating.kind),
        Some(LocalNfoRatingKind::Audience)
    );
    assert!(metadata
        .collection
        .as_ref()
        .expect("collection")
        .unique_ids
        .iter()
        .any(|id| id.provider == "tmdb_collection" && id.value == "404"));
}

#[test]
fn parse_tvshow_nfo_scopes_counts_season_metadata_and_artwork() {
    let root = unique_temp_path("nfo-tvshow-seasons");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("series.jpg"), b"\xff\xd8\xffseries").unwrap();
    fs::write(root.join("season-2.jpg"), b"\xff\xd8\xffseason").unwrap();
    fs::write(
        root.join("season-2-landscape.jpg"),
        b"\xff\xd8\xfflandscape",
    )
    .unwrap();

    let metadata = parse_nfo_metadata(
        r#"
        <tvshow>
          <title>Example Show</title>
          <season>3</season>
          <episode>24</episode>
          <episodeguide>{"tmdb":"123", "tvdb":456}</episodeguide>
          <namedseason number="2">The Second Chapter</namedseason>
          <seasonplot number="2">Second-season overview.</seasonplot>
          <thumb aspect="poster">series.jpg</thumb>
          <thumb aspect="poster" type="season" season="2">season-2.jpg</thumb>
          <thumb aspect="landscape" type="season" season="2">season-2-landscape.jpg</thumb>
        </tvshow>
        "#,
        &root,
    )
    .expect("TV show NFO should parse");

    assert_eq!(metadata.season_count, Some(3));
    assert_eq!(metadata.episode_count, Some(24));
    assert_eq!(metadata.season_number, None);
    assert_eq!(metadata.episode_number, None);
    assert_eq!(metadata.artwork.posters.len(), 1);
    assert_eq!(metadata.artwork.images[0].kind, LocalNfoImageKind::Poster);
    assert!(metadata
        .episode_guide_ids
        .iter()
        .any(|id| id.provider == "tmdb" && id.value == "123"));
    assert!(metadata
        .episode_guide_ids
        .iter()
        .any(|id| id.provider == "tvdb" && id.value == "456"));

    let season = metadata
        .named_seasons
        .iter()
        .find(|season| season.season_number == 2)
        .expect("season metadata");
    assert_eq!(season.title.as_deref(), Some("The Second Chapter"));
    assert_eq!(season.overview.as_deref(), Some("Second-season overview."));
    assert_eq!(season.artwork.posters.len(), 1);
    assert_eq!(season.artwork.thumbnails.len(), 1);
    assert_eq!(season.artwork.images[1].kind, LocalNfoImageKind::Landscape);

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn parse_episode_nfo_keeps_special_display_order_without_multi_episode_projection() {
    let metadata = parse_nfo_metadata(
        r#"
        <episodedetails>
          <season>0</season>
          <episode>4</episode>
          <displayepisode>8</displayepisode>
          <displayseason>2</displayseason>
          <airsafter_season>1</airsafter_season>
          <episodenumberend>5</episodenumberend>
        </episodedetails>
        "#,
        Path::new("/media"),
    )
    .expect("episode NFO should parse");

    assert_eq!(metadata.season_number, Some(0));
    assert_eq!(metadata.episode_number, Some(4));
    assert_eq!(metadata.display_episode_number, Some(8));
    assert_eq!(metadata.display_season_number, Some(2));
    assert_eq!(metadata.display_after_season_number, Some(1));
    assert!(serde_json::to_value(&metadata)
        .expect("normalized payload")
        .get("episode_number_end")
        .is_none());
}

#[test]
fn nfo_root_specific_fields_do_not_leak_between_media_kinds() {
    let movie = parse_nfo_metadata(
        r#"
        <movie>
          <season>4</season>
          <episode>20</episode>
          <displayepisode>8</displayepisode>
          <airs_dayofweek>Monday</airs_dayofweek>
          <episodeguide>{"tmdb":"123"}</episodeguide>
          <namedseason number="2">Must not project</namedseason>
          <showlink>Allowed movie link</showlink>
          <showtitle>Must not project</showtitle>
        </movie>
        "#,
        Path::new("/media"),
    )
    .expect("movie NFO should parse");

    assert_eq!(movie.season_count, None);
    assert_eq!(movie.episode_count, None);
    assert_eq!(movie.display_episode_number, None);
    assert!(movie.air_days.is_empty());
    assert!(movie.episode_guide_ids.is_empty());
    assert!(movie.named_seasons.is_empty());
    assert_eq!(movie.show_link.as_deref(), Some("Allowed movie link"));
    assert_eq!(movie.show_title, None);

    let episode = parse_nfo_metadata(
        r#"
        <episodedetails>
          <showlink>Must not project</showlink>
          <airs_time>20:00</airs_time>
          <episodeguide>{"tmdb":"123"}</episodeguide>
          <namedseason number="2">Must not project</namedseason>
          <showtitle>Allowed episode show title</showtitle>
        </episodedetails>
        "#,
        Path::new("/media"),
    )
    .expect("episode NFO should parse");

    assert_eq!(episode.show_link, None);
    assert_eq!(episode.air_time, None);
    assert!(episode.episode_guide_ids.is_empty());
    assert!(episode.named_seasons.is_empty());
    assert_eq!(
        episode.show_title.as_deref(),
        Some("Allowed episode show title")
    );
}

#[test]
fn parse_nfo_metadata_distinguishes_audience_and_critic_ratings() {
    let metadata = parse_nfo_metadata(
        r#"
        <movie>
          <rating>7.8</rating>
          <votes>1,234</votes>
          <criticrating>91</criticrating>
          <ratings>
            <rating name="tmdb" max="10"><value>8.2</value></rating>
          </ratings>
        </movie>
        "#,
        Path::new("/media"),
    )
    .expect("rating NFO should parse");

    assert!(metadata.ratings.iter().any(|rating| {
        rating.source == "default"
            && rating.kind == LocalNfoRatingKind::Audience
            && rating.value == 7.8
    }));
    assert!(metadata.ratings.iter().any(|rating| {
        rating.source == "default"
            && rating.kind == LocalNfoRatingKind::Critic
            && rating.value == 91.0
            && rating.scale == 100.0
    }));
    assert!(metadata.ratings.iter().any(|rating| {
        rating.source == "tmdb"
            && rating.kind == LocalNfoRatingKind::Audience
            && rating.value == 8.2
    }));
}

#[test]
fn parse_nfo_metadata_ignores_non_finite_and_out_of_range_ratings() {
    let metadata = parse_nfo_metadata(
        r#"
        <movie>
          <title>Valid title survives invalid ratings</title>
          <rating>NaN</rating>
          <communityrating>-1</communityrating>
          <criticrating>101</criticrating>
          <ratings>
            <rating name="infinite" max="10"><value>inf</value></rating>
            <rating name="negative" max="10"><value>-0.1</value></rating>
            <rating name="overflow" max="10"><value>10.1</value></rating>
            <rating name="invalid-scale" max="NaN"><value>8</value></rating>
            <rating name="valid" max="5"><value>4.5</value></rating>
          </ratings>
        </movie>
        "#,
        Path::new("/media"),
    )
    .expect("invalid ratings must not invalidate the remaining NFO metadata");

    assert_eq!(
        metadata.title.as_deref(),
        Some("Valid title survives invalid ratings")
    );
    assert_eq!(metadata.ratings.len(), 1);
    assert_eq!(metadata.ratings[0].source, "valid");
    assert_eq!(metadata.ratings[0].value, 4.5);
    assert_eq!(metadata.ratings[0].scale, 5.0);
    assert!(serde_json::to_value(&metadata).is_ok());
}

#[test]
fn normalized_nfo_payload_accepts_future_additive_defaults_without_losing_identity() {
    let metadata = serde_json::from_value::<super::LocalNfoMetadata>(serde_json::json!({
        "kind": "movie",
        "title": "Stored v1 title"
    }))
    .expect("missing additive fields in an older normalized payload should use safe defaults");

    assert_eq!(metadata.kind, LocalNfoKind::Movie);
    assert_eq!(metadata.title.as_deref(), Some("Stored v1 title"));
    assert!(metadata.ratings.is_empty());
    assert!(metadata.credits.actors.is_empty());
    assert!(
        serde_json::from_value::<super::LocalNfoMetadata>(serde_json::json!({
            "title": "Identity must remain required"
        }))
        .is_err()
    );
}

#[test]
fn parse_nfo_metadata_rejects_dtd_and_unsupported_roots() {
    let root = unique_temp_path("nfo-security");
    fs::create_dir_all(&root).unwrap();

    assert!(parse_nfo_metadata(
        r#"<!DOCTYPE movie [<!ENTITY xxe SYSTEM "file:///etc/passwd">]>
        <movie><title>&xxe;</title></movie>"#,
        &root,
    )
    .is_none());
    assert!(parse_nfo_metadata("<album><title>Not Media</title></album>", &root).is_none());

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn discovery_routes_episode_nfo_fields_and_records_source_path() {
    let root = unique_temp_path("episode-nfo-discovery");
    let video_path = root.join("Example.Show.S01E02.mkv");
    let nfo_path = video_path.with_extension("nfo");
    fs::create_dir_all(&root).unwrap();
    fs::write(&video_path, b"not a real video").unwrap();
    fs::write(
        &nfo_path,
        r#"
        <episodedetails>
          <title>The Local Episode</title>
          <plot>Episode-only plot.</plot>
          <aired>2026-08-01</aired>
          <season>1</season>
          <episode>2</episode>
          <uniqueid type="tmdb">4455</uniqueid>
        </episodedetails>
        "#,
    )
    .unwrap();

    let files = discover_media_files(&root).expect("discovery should tolerate ffprobe failure");
    let file = files.first().expect("episode should be discovered");
    assert_eq!(file.title, "Example Show");
    assert_eq!(file.episode_title.as_deref(), Some("The Local Episode"));
    assert_eq!(file.episode_overview.as_deref(), Some("Episode-only plot."));
    assert_eq!(file.overview, None);
    assert_eq!(file.season_number, Some(1));
    assert_eq!(file.episode_number, Some(2));
    let local_nfo = file
        .local_nfo
        .as_ref()
        .expect("episode NFO should be retained");
    assert_eq!(local_nfo.kind, LocalNfoKind::Episode);
    assert_eq!(local_nfo.source_path, nfo_path.canonicalize().unwrap());
    assert_eq!(local_nfo.aired.as_deref(), Some("2026-08-01"));

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn episode_file_coordinates_take_priority_over_conflicting_nfo_coordinates() {
    let root = unique_temp_path("episode-nfo-coordinate-priority");
    let video_path = root.join("Example.Show.S02E03.mkv");
    fs::create_dir_all(&root).unwrap();
    fs::write(&video_path, b"video").unwrap();
    fs::write(
        video_path.with_extension("nfo"),
        r#"<episodedetails>
            <title>Episode title</title>
            <season>9</season>
            <episode>10</episode>
        </episodedetails>"#,
    )
    .unwrap();

    let metadata = parse_media_metadata(&video_path);
    assert_eq!(metadata.season_number, Some(2));
    assert_eq!(metadata.episode_number, Some(3));
    assert_eq!(metadata.episode_title.as_deref(), Some("Episode title"));

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn nfo_root_type_must_match_movie_episode_and_series_use() {
    let root = unique_temp_path("nfo-root-use");
    let movie_path = root.join("Example.Movie.2026.mkv");
    let episode_path = root.join("Show.S01E01.mkv");
    let series_path = root
        .join("Series")
        .join("Season 01")
        .join("Show.S01E01.mkv");
    fs::create_dir_all(series_path.parent().unwrap()).unwrap();
    fs::write(&movie_path, b"movie").unwrap();
    fs::write(&episode_path, b"episode").unwrap();
    fs::write(&series_path, b"series episode").unwrap();
    fs::write(
        movie_path.with_extension("nfo"),
        "<episodedetails><title>Wrong episode root</title></episodedetails>",
    )
    .unwrap();
    fs::write(
        episode_path.with_extension("nfo"),
        "<movie><title>Wrong movie root</title></movie>",
    )
    .unwrap();
    fs::write(
        root.join("Series").join("tvshow.nfo"),
        "<movie><title>Wrong series root</title></movie>",
    )
    .unwrap();

    assert!(parse_media_metadata(&movie_path).local_nfo.is_none());
    assert!(parse_media_metadata(&episode_path).local_nfo.is_none());
    assert!(infer_series_sidecar_metadata(&series_path).is_none());

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn parse_nfo_metadata_rejects_local_artwork_outside_the_nfo_directory() {
    let root = unique_temp_path("nfo-artwork-boundary");
    let media_dir = root.join("media");
    let outside_image = root.join("outside.jpg");
    fs::create_dir_all(&media_dir).unwrap();
    fs::write(&outside_image, b"\xff\xd8\xffoutside").unwrap();

    let metadata = parse_nfo_metadata(
        &format!(
            r#"
            <movie>
              <thumb>../outside.jpg</thumb>
              <fanart><thumb>{}</thumb></fanart>
            </movie>
            "#,
            outside_image.display()
        ),
        &media_dir,
    )
    .expect("valid NFO should still parse when artwork is rejected");

    assert!(metadata.artwork.posters.is_empty());
    assert!(metadata.artwork.backdrops.is_empty());

    let _ = fs::remove_dir_all(&root);
}

#[cfg(unix)]
#[test]
fn parse_nfo_metadata_rejects_symlinks_that_escape_the_nfo_directory() {
    use std::os::unix::fs::symlink;

    let root = unique_temp_path("nfo-artwork-symlink");
    let media_dir = root.join("media");
    let outside_image = root.join("outside.jpg");
    let linked_image = media_dir.join("poster.jpg");
    fs::create_dir_all(&media_dir).unwrap();
    fs::write(&outside_image, b"\xff\xd8\xffoutside").unwrap();
    symlink(&outside_image, &linked_image).unwrap();

    let metadata = parse_nfo_metadata("<movie><thumb>poster.jpg</thumb></movie>", &media_dir)
        .expect("valid NFO should still parse when artwork is rejected");

    assert!(metadata.artwork.posters.is_empty());

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn parse_nfo_metadata_rejects_non_image_payloads() {
    let root = unique_temp_path("nfo-artwork-content");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("poster.jpg"), b"not an image").unwrap();

    let metadata = parse_nfo_metadata("<movie><thumb>poster.jpg</thumb></movie>", &root)
        .expect("valid NFO should still parse when artwork is rejected");

    assert!(metadata.artwork.posters.is_empty());

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn parse_nfo_metadata_ignores_empty_artwork_references() {
    let root = unique_temp_path("nfo-empty-artwork");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("poster.jpg"), b"").unwrap();
    fs::write(root.join("fanart.png"), b"").unwrap();

    let metadata = parse_nfo_metadata(
        r#"
        <movie>
          <title>Empty Artwork</title>
          <thumb aspect="poster">poster.jpg</thumb>
          <fanart>
            <thumb>fanart.png</thumb>
          </fanart>
        </movie>
        "#,
        &root,
    )
    .expect("valid NFO should still parse when artwork is rejected");

    assert!(metadata.artwork.posters.is_empty());
    assert!(metadata.artwork.backdrops.is_empty());

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
            error: None,
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
            technical_tags: vec!["1080p".to_string(), "HDR10".to_string()],
            audio_streams: Vec::new(),
            subtitle_streams: Vec::new(),
        }
    );
}

#[test]
fn parse_ffprobe_output_extracts_technical_tags() {
    let probe = parse_ffprobe_output(
        br#"{
            "streams": [
                {
                    "codec_type": "video",
                    "codec_name": "hevc",
                    "codec_tag_string": "dvh1",
                    "width": 3840,
                    "height": 1600,
                    "color_transfer": "smpte2084",
                    "color_primaries": "bt2020",
                    "side_data_list": [
                        { "side_data_type": "DOVI configuration record" },
                        { "side_data_type": "HDR Dynamic Metadata SMPTE2094-40 (HDR10+)" }
                    ]
                },
                {
                    "codec_type": "audio",
                    "codec_name": "truehd",
                    "profile": "Dolby TrueHD with Dolby Atmos",
                    "tags": {
                        "title": "English Atmos"
                    }
                },
                {
                    "codec_type": "audio",
                    "codec_name": "dts",
                    "profile": "DTS-HD MA"
                }
            ],
            "format": {}
        }"#,
    )
    .unwrap();

    assert_eq!(
        probe.technical_tags,
        vec![
            "4K".to_string(),
            "Dolby Vision".to_string(),
            "Atmos".to_string(),
            "DTS-HD".to_string()
        ]
    );
}

#[test]
fn parse_ffprobe_output_extracts_resolution_technical_tags() {
    let probe = parse_ffprobe_output(
        br#"{
            "streams": [
                {
                    "codec_type": "video",
                    "codec_name": "h264",
                    "width": 1920,
                    "height": 800
                }
            ],
            "format": {}
        }"#,
    )
    .unwrap();

    assert_eq!(probe.technical_tags, vec!["1080p".to_string()]);
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
fn inspect_media_inventory_can_be_cancelled_before_ffprobe_starts() {
    let result = inspect_media_file_inventory_with_cancel(
        super::DiscoveredMediaFileInventory {
            file_path: PathBuf::from("/media/missing-file.mkv"),
            file_size: 2048,
            file_modified_at_ms: None,
            sidecar_fingerprint: String::new(),
        },
        || true,
    );

    assert_eq!(
        result.expect_err("inspection should be cancelled").kind(),
        ErrorKind::Interrupted
    );
}

#[test]
fn inventory_scan_hash_changes_when_directory_sidecars_change() {
    let root = unique_temp_path("sidecar-fingerprint");
    let video_path = root.join("Movie.2026.mkv");
    fs::create_dir_all(&root).unwrap();
    fs::write(&video_path, b"same video bytes").unwrap();
    fs::write(root.join("movie.nfo"), b"<movie><title>A</title></movie>").unwrap();

    let first = discover_media_file_inventory_with_progress_and_cancel(&root, |_| {}, || false)
        .unwrap()
        .pop()
        .unwrap();
    let first_hash = discovered_media_file_inventory_scan_hash(&first);

    fs::write(
        root.join("movie.nfo"),
        b"<movie><title>A changed title</title></movie>",
    )
    .unwrap();
    let second = discover_media_file_inventory_with_progress_and_cancel(&root, |_| {}, || false)
        .unwrap()
        .pop()
        .unwrap();
    let second_hash = discovered_media_file_inventory_scan_hash(&second);

    let _ = fs::remove_dir_all(root);
    assert_eq!(first.file_size, second.file_size);
    assert_eq!(first.file_modified_at_ms, second.file_modified_at_ms);
    assert_ne!(first.sidecar_fingerprint, second.sidecar_fingerprint);
    assert_ne!(first_hash, second_hash);
}

#[test]
fn inventory_sidecar_fingerprint_only_tracks_tvshow_nfo_within_library_root() {
    let outer = unique_temp_path("root-bound-series-fingerprint");
    let library_root = outer.join("library");
    let series_root = library_root.join("Show");
    let video_path = series_root.join("Season 01").join("S01E01.mkv");
    fs::create_dir_all(video_path.parent().unwrap()).unwrap();
    fs::write(&video_path, b"same video").unwrap();
    fs::write(
        outer.join("tvshow.nfo"),
        b"<tvshow><title>Outside A</title></tvshow>",
    )
    .unwrap();

    let first =
        discover_media_file_inventory_with_progress_and_cancel(&library_root, |_| {}, || false)
            .unwrap()
            .pop()
            .unwrap();

    fs::write(
        outer.join("tvshow.nfo"),
        b"<tvshow><title>Outside title changed and grew</title></tvshow>",
    )
    .unwrap();
    let outside_changed =
        discover_media_file_inventory_with_progress_and_cancel(&library_root, |_| {}, || false)
            .unwrap()
            .pop()
            .unwrap();

    fs::write(
        series_root.join("tvshow.nfo"),
        b"<tvshow><title>Inside</title></tvshow>",
    )
    .unwrap();
    let inside_added =
        discover_media_file_inventory_with_progress_and_cancel(&library_root, |_| {}, || false)
            .unwrap()
            .pop()
            .unwrap();

    let _ = fs::remove_dir_all(&outer);
    assert_eq!(
        first.sidecar_fingerprint,
        outside_changed.sidecar_fingerprint
    );
    assert_ne!(
        outside_changed.sidecar_fingerprint,
        inside_added.sidecar_fingerprint
    );
}

#[test]
fn inventory_scan_hash_tracks_episode_version_count_changes() {
    let root = unique_temp_path("episode-version-fingerprint");
    let first_video_path = root.join("Show.1080p.S01E01.mkv");
    let second_video_path = root.join("Show.2160p.S01E01.mkv");
    fs::create_dir_all(&root).unwrap();
    fs::write(&first_video_path, b"first video").unwrap();

    let initial = discover_media_file_inventory_with_progress_and_cancel(&root, |_| {}, || false)
        .unwrap()
        .into_iter()
        .find(|file| file.file_path == first_video_path)
        .unwrap();
    let initial_hash = discovered_media_file_inventory_scan_hash(&initial);

    fs::write(&second_video_path, b"second video").unwrap();
    let ambiguous = discover_media_file_inventory_with_progress_and_cancel(&root, |_| {}, || false)
        .unwrap()
        .into_iter()
        .find(|file| file.file_path == first_video_path)
        .unwrap();
    let ambiguous_hash = discovered_media_file_inventory_scan_hash(&ambiguous);

    fs::remove_file(&second_video_path).unwrap();
    let restored = discover_media_file_inventory_with_progress_and_cancel(&root, |_| {}, || false)
        .unwrap()
        .into_iter()
        .find(|file| file.file_path == first_video_path)
        .unwrap();
    let restored_hash = discovered_media_file_inventory_scan_hash(&restored);

    let _ = fs::remove_dir_all(root);
    assert_ne!(initial.sidecar_fingerprint, ambiguous.sidecar_fingerprint);
    assert_ne!(initial_hash, ambiguous_hash);
    assert_eq!(initial.sidecar_fingerprint, restored.sidecar_fingerprint);
    assert_eq!(initial_hash, restored_hash);
}

#[test]
fn discover_media_files_only_returns_supported_videos() {
    let root = unique_temp_path("root");
    let nested = root.join("nested");

    let result = {
        fs::create_dir_all(&nested).unwrap();
        fs::write(root.join("movie.mp4"), b"video").unwrap();
        fs::write(nested.join("episode.mkv"), b"video").unwrap();
        fs::write(root.join("notes.txt"), b"ignore").unwrap();

        discover_media_files(&root)
    };

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

    let result = {
        fs::create_dir_all(&nested).unwrap();
        fs::write(root.join("movie.mp4"), b"video").unwrap();
        fs::write(nested.join("episode.mkv"), b"video").unwrap();
        fs::write(root.join("movie.nfo"), b"meta").unwrap();
        fs::write(root.join("poster.jpg"), b"art").unwrap();

        discover_media_paths(&root)
    };

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

#[cfg(unix)]
#[test]
fn discover_media_paths_rejects_file_and_directory_symlinks_outside_the_library() {
    use std::os::unix::fs::symlink;

    let root = unique_temp_path("outside-symlink-root");
    let outside = unique_temp_path("outside-symlink-target");
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&outside).unwrap();
    fs::write(root.join("inside.mp4"), b"video").unwrap();
    fs::write(outside.join("outside.mkv"), b"video").unwrap();
    symlink(outside.join("outside.mkv"), root.join("linked-file.mkv")).unwrap();
    symlink(&outside, root.join("linked-directory")).unwrap();

    let files = discover_media_paths(&root).unwrap();
    let inventory =
        discover_media_file_inventory_with_progress_and_cancel(&root, |_| {}, || false).unwrap();

    assert_eq!(files, vec![root.join("inside.mp4")]);
    assert_eq!(inventory.len(), 1);
    assert_eq!(inventory[0].file_path, root.join("inside.mp4"));
    let _ = fs::remove_dir_all(root);
    let _ = fs::remove_dir_all(outside);
}

#[cfg(unix)]
#[test]
fn discover_media_paths_keeps_file_symlinks_that_resolve_inside_the_library() {
    use std::os::unix::fs::symlink;

    let root = unique_temp_path("inside-symlink-root");
    fs::create_dir_all(root.join("targets")).unwrap();
    let target = root.join("targets").join("movie.mkv");
    let linked_file = root.join("movie-link.mkv");
    fs::write(&target, b"video").unwrap();
    symlink(&target, &linked_file).unwrap();

    let files = discover_media_paths(&root).unwrap();

    assert!(files.contains(&target));
    assert!(files.contains(&linked_file));
    let _ = fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
fn discover_media_paths_stops_internal_directory_symlink_cycles() {
    use std::os::unix::fs::symlink;

    let root = unique_temp_path("directory-symlink-cycle");
    let nested = root.join("nested");
    fs::create_dir_all(&nested).unwrap();
    fs::write(nested.join("episode.mkv"), b"video").unwrap();
    symlink(&root, nested.join("back-to-root")).unwrap();

    let files = discover_media_paths(&root).unwrap();

    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0].canonicalize().unwrap(),
        nested.join("episode.mkv").canonicalize().unwrap()
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn discover_media_inventory_returns_error_for_missing_root() {
    let root = unique_temp_path("missing-root");

    let result = discover_media_file_inventory_with_progress_and_cancel(&root, |_| {}, || false);

    assert!(matches!(
        result,
        Err(error) if error.kind() == ErrorKind::NotFound
    ));
}

#[test]
fn discover_media_files_reads_sidecar_metadata_and_artwork() {
    let root = unique_temp_path("sidecar");
    let movie_dir = root.join("spirited-away");
    let video_path = movie_dir.join("Spirited.Away.2001.mkv");

    let result = {
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
    };

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

    let result = {
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
    };

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
fn full_inspection_can_exclude_generic_movie_nfo_without_excluding_file_specific_nfo() {
    let root = unique_temp_path("generic-movie-nfo-policy");
    let video_path = root.join("Actual.Movie.2025.mkv");
    fs::create_dir_all(&root).unwrap();
    fs::write(&video_path, b"video").unwrap();
    fs::write(
        root.join("movie.nfo"),
        "<movie><title>Shared Wrong Title</title></movie>",
    )
    .unwrap();

    let inventory = discover_media_file_inventory_with_progress_and_cancel(&root, |_| {}, || false)
        .unwrap()
        .pop()
        .unwrap();
    let subtitle_index = SubtitleDirectoryIndex::build([video_path.as_path()]);
    let without_generic =
        inspect_media_file_inventory_within_root_with_cancel_and_subtitle_index_and_nfo_policy(
            inventory.clone(),
            &root,
            &subtitle_index,
            false,
            || false,
        )
        .unwrap();
    assert_eq!(without_generic.title, "Actual Movie");
    assert!(without_generic.local_nfo.is_none());

    fs::write(
        video_path.with_extension("nfo"),
        "<movie><title>File Specific Title</title></movie>",
    )
    .unwrap();
    let with_specific =
        inspect_media_file_inventory_within_root_with_cancel_and_subtitle_index_and_nfo_policy(
            inventory,
            &root,
            &subtitle_index,
            false,
            || false,
        )
        .unwrap();
    let _ = fs::remove_dir_all(&root);

    assert_eq!(with_specific.title, "File Specific Title");
    assert!(with_specific.local_nfo.is_some());
}

#[test]
fn inspect_media_file_sidecar_only_reads_local_metadata_without_probing_streams() {
    let root = unique_temp_path("inspect-sidecar-only");
    let video_path = root.join("Not.A.Real.Video.2026.mkv");
    fs::create_dir_all(&root).unwrap();
    fs::write(&video_path, b"not a playable media stream").unwrap();
    fs::write(
        video_path.with_extension("nfo"),
        r#"<movie><title>Local title</title><plot>Local plot</plot></movie>"#,
    )
    .unwrap();
    fs::write(root.join("poster.jpg"), b"\xff\xd8\xffposter").unwrap();

    let file = inspect_media_file_sidecar_only(&video_path)
        .expect("sidecar-only inspection must not require a valid media stream");

    let _ = fs::remove_dir_all(&root);
    assert_eq!(file.title, "Local title");
    assert_eq!(file.overview.as_deref(), Some("Local plot"));
    assert!(file.local_nfo.is_some());
    assert_eq!(file.probe_error, None);
    assert_eq!(file.duration_seconds, None);
    assert_eq!(file.container.as_deref(), Some("mkv"));
    assert!(file.audio_tracks.is_empty());
    assert!(file.subtitle_tracks.is_empty());
}

#[test]
fn parse_media_metadata_ignores_empty_local_artwork_files() {
    let root = unique_temp_path("empty-artwork");
    let video_path = root.join("Spirited.Away.2001.mkv");

    let result = {
        fs::create_dir_all(&root).unwrap();
        fs::write(&video_path, b"video").unwrap();
        fs::write(root.join("poster.jpg"), b"").unwrap();
        fs::write(root.join("fanart.jpg"), b"").unwrap();

        parse_media_metadata(&video_path)
    };

    let _ = fs::remove_dir_all(&root);

    assert_eq!(result.poster_path, None);
    assert_eq!(result.backdrop_path, None);
}

#[test]
fn parse_episode_metadata_keeps_file_artwork_separate_from_series_artwork() {
    let root = unique_temp_path("episode-artwork-scope");
    let video_path = root.join("Arcane.S01E01.mkv");

    let result = {
        fs::create_dir_all(&root).unwrap();
        fs::write(&video_path, b"video").unwrap();
        fs::write(root.join("Arcane.S01E01-poster.jpg"), b"episode poster").unwrap();
        fs::write(root.join("Arcane.S01E01-fanart.jpg"), b"episode backdrop").unwrap();
        fs::write(root.join("poster.jpg"), b"series poster").unwrap();
        fs::write(root.join("fanart.jpg"), b"series backdrop").unwrap();

        parse_media_metadata(&video_path)
    };

    let _ = fs::remove_dir_all(&root);

    assert_eq!(
        result.poster_path.as_deref(),
        Some(
            root.join("Arcane.S01E01-poster.jpg")
                .to_string_lossy()
                .as_ref()
        )
    );
    assert_eq!(
        result.backdrop_path.as_deref(),
        Some(
            root.join("Arcane.S01E01-fanart.jpg")
                .to_string_lossy()
                .as_ref()
        )
    );
    assert_eq!(
        result.series_poster_path.as_deref(),
        Some(root.join("poster.jpg").to_string_lossy().as_ref())
    );
    assert_eq!(
        result.series_backdrop_path.as_deref(),
        Some(root.join("fanart.jpg").to_string_lossy().as_ref())
    );
}

#[test]
fn parse_episode_metadata_does_not_use_generic_artwork_as_episode_artwork() {
    let root = unique_temp_path("episode-generic-artwork");
    let video_path = root.join("Arcane.S01E01.mkv");

    let result = {
        fs::create_dir_all(&root).unwrap();
        fs::write(&video_path, b"video").unwrap();
        fs::write(root.join("poster.jpg"), b"series poster").unwrap();
        fs::write(root.join("fanart.jpg"), b"series backdrop").unwrap();

        parse_media_metadata(&video_path)
    };

    let _ = fs::remove_dir_all(&root);

    assert_eq!(result.poster_path, None);
    assert_eq!(result.backdrop_path, None);
    assert_eq!(
        result.series_poster_path.as_deref(),
        Some(root.join("poster.jpg").to_string_lossy().as_ref())
    );
    assert_eq!(
        result.series_backdrop_path.as_deref(),
        Some(root.join("fanart.jpg").to_string_lossy().as_ref())
    );
}

#[test]
fn parse_episode_metadata_supports_exact_thumb_names_without_fuzzy_matching() {
    let root = unique_temp_path("episode-thumb-names");
    let exact_video = root.join("Arcane.S01E01.mkv");
    let spaced_video = root.join("Arcane.S01E02.mkv");
    fs::create_dir_all(&root).unwrap();
    fs::write(&exact_video, b"video").unwrap();
    fs::write(&spaced_video, b"video").unwrap();
    fs::write(root.join("Arcane.S01E01-thumb.jpg"), b"episode one").unwrap();
    fs::write(root.join("Arcane.S01E02 - thumb.jpg"), b"episode two").unwrap();
    fs::write(root.join("Arcane.S01-thumb.jpg"), b"must not match").unwrap();

    let exact = parse_media_metadata(&exact_video);
    let spaced = parse_media_metadata(&spaced_video);
    let _ = fs::remove_dir_all(&root);

    assert_eq!(
        exact.poster_path.as_deref(),
        Some(
            root.join("Arcane.S01E01-thumb.jpg")
                .to_string_lossy()
                .as_ref()
        )
    );
    assert_eq!(
        spaced.poster_path.as_deref(),
        Some(
            root.join("Arcane.S01E02 - thumb.jpg")
                .to_string_lossy()
                .as_ref()
        )
    );
}

#[test]
fn parse_episode_metadata_prefers_an_explicit_nfo_thumb() {
    let root = unique_temp_path("episode-nfo-thumb-priority");
    let video_path = root.join("Arcane.S01E01.mkv");
    fs::create_dir_all(&root).unwrap();
    fs::write(&video_path, b"video").unwrap();
    fs::write(
        video_path.with_extension("nfo"),
        "<episodedetails><title>Episode One</title><thumb>nfo-thumb.jpg</thumb></episodedetails>",
    )
    .unwrap();
    fs::write(root.join("nfo-thumb.jpg"), b"\xff\xd8\xffnfo").unwrap();
    fs::write(
        root.join("Arcane.S01E01-thumb.jpg"),
        b"\xff\xd8\xffautomatic",
    )
    .unwrap();

    let parsed = parse_media_metadata(&video_path);
    let expected_nfo_thumb = root.join("nfo-thumb.jpg").canonicalize().unwrap();
    let _ = fs::remove_dir_all(&root);

    assert_eq!(
        parsed.poster_path.as_deref(),
        Some(expected_nfo_thumb.to_string_lossy().as_ref())
    );
}

#[test]
fn parse_episode_metadata_resolves_only_numbered_artwork_in_a_flat_series_directory() {
    let root = unique_temp_path("flat-season-artwork");
    let first_season = root.join("S01E01.mkv");
    let second_season = root.join("S02E01.mkv");
    fs::create_dir_all(&root).unwrap();
    fs::write(&first_season, b"video").unwrap();
    fs::write(&second_season, b"video").unwrap();
    fs::write(root.join("season01-poster.jpg"), b"season one").unwrap();
    fs::write(root.join("season-poster.jpg"), b"ambiguous").unwrap();

    let first = parse_media_metadata(&first_season);
    let second = parse_media_metadata(&second_season);
    let _ = fs::remove_dir_all(&root);

    assert_eq!(
        first.season_poster_path.as_deref(),
        Some(root.join("season01-poster.jpg").to_string_lossy().as_ref())
    );
    assert_eq!(second.season_poster_path, None);
}

#[test]
fn parse_episode_metadata_scopes_generic_artwork_to_an_explicit_season_directory() {
    let root = unique_temp_path("explicit-season-artwork");
    let series_root = root.join("Arcane");
    let season_root = series_root.join("Season 01");
    let video_path = season_root.join("S01E01.mkv");
    fs::create_dir_all(&season_root).unwrap();
    fs::write(&video_path, b"video").unwrap();
    fs::write(series_root.join("poster.jpg"), b"series poster").unwrap();
    fs::write(season_root.join("poster.jpg"), b"season poster").unwrap();

    let parsed = parse_media_metadata(&video_path);
    let _ = fs::remove_dir_all(&root);

    assert_eq!(parsed.series_poster_path, None);
    assert_eq!(
        parsed.season_poster_path.as_deref(),
        Some(season_root.join("poster.jpg").to_string_lossy().as_ref())
    );
}

#[test]
fn parse_episode_metadata_rejects_generic_artwork_from_a_conflicting_season_directory() {
    let root = unique_temp_path("conflicting-season-artwork");
    let season_root = root.join("Season 02");
    let video_path = season_root.join("S01E01.mkv");
    fs::create_dir_all(&season_root).unwrap();
    fs::write(&video_path, b"video").unwrap();
    fs::write(season_root.join("season01-poster.jpg"), b"wrong season").unwrap();
    fs::write(season_root.join("season-poster.jpg"), b"wrong season").unwrap();

    let parsed = parse_media_metadata(&video_path);
    let _ = fs::remove_dir_all(&root);

    assert_eq!(parsed.season_poster_path, None);
    assert_eq!(parsed.series_poster_path, None);
}

#[test]
fn parse_episode_metadata_does_not_use_sidecar_artwork_as_episode_artwork() {
    let root = unique_temp_path("episode-sidecar-artwork");
    let video_path = root.join("Arcane.S01E01.mkv");

    let result = {
        fs::create_dir_all(&root).unwrap();
        fs::write(&video_path, b"video").unwrap();
        fs::write(root.join("sidecar-poster.jpg"), b"sidecar poster").unwrap();
        fs::write(root.join("sidecar-fanart.jpg"), b"sidecar backdrop").unwrap();
        fs::write(
            video_path.with_extension("nfo"),
            r#"
            <episodedetails>
              <title>Welcome to the Playground</title>
              <thumb aspect="poster">sidecar-poster.jpg</thumb>
              <fanart>
                <thumb>sidecar-fanart.jpg</thumb>
              </fanart>
            </episodedetails>
            "#,
        )
        .unwrap();

        parse_media_metadata(&video_path)
    };

    let _ = fs::remove_dir_all(&root);

    assert_eq!(result.poster_path, None);
    assert_eq!(result.backdrop_path, None);
    assert_eq!(result.series_poster_path, None);
    assert_eq!(result.series_backdrop_path, None);
}

#[test]
fn discover_media_files_with_progress_and_cancel_stops_when_requested() {
    let root = unique_temp_path("cancel");

    let result = {
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
    };

    let _ = fs::remove_dir_all(&root);

    assert!(matches!(
        result,
        Err(error) if error.kind() == ErrorKind::Interrupted
    ));
}

#[test]
fn discover_media_files_with_progress_item_and_cancel_emits_discovered_files() {
    let root = unique_temp_path("progress-items");

    let (result, discovered_titles) = {
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
    };

    let _ = fs::remove_dir_all(&root);

    let files = result.unwrap();
    assert_eq!(files.len(), 2);
    assert_eq!(discovered_titles.len(), 2);
    assert!(discovered_titles
        .iter()
        .any(|title| title.to_ascii_lowercase().contains("movie")));
    assert!(discovered_titles.iter().any(|title| title.contains("Show")));
}
