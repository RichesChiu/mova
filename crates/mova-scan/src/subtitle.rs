use crate::{
    parse::{episode_identity_for_path, extension_lowercase, humanize_file_stem, EpisodeIdentity},
    probe::EmbeddedSubtitleStream,
    DiscoveredSubtitleTrack,
};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
struct ParsedSubtitleSidecar {
    base_stem: String,
    episode_identity: Option<EpisodeIdentity>,
    language: Option<String>,
    label: Option<String>,
    is_default: bool,
    is_forced: bool,
    is_hearing_impaired: bool,
}

#[derive(Debug, Clone)]
struct IndexedSubtitleSidecar {
    path: PathBuf,
    subtitle_format: String,
    parsed: ParsedSubtitleSidecar,
}

#[derive(Debug, Clone, Default)]
struct IndexedSubtitleDirectory {
    subtitles: Vec<IndexedSubtitleSidecar>,
    episode_identity_counts: HashMap<(i32, i32), usize>,
}

/// Scan-local directory index for external subtitles.
///
/// A directory is read at most once while the index is built. Every video in
/// that directory then reuses the parsed subtitle candidates and episode
/// ambiguity counts instead of issuing two additional `read_dir` calls.
#[derive(Debug, Clone, Default)]
pub struct SubtitleDirectoryIndex {
    directories: HashMap<PathBuf, IndexedSubtitleDirectory>,
}

impl SubtitleDirectoryIndex {
    pub fn build<'a>(video_paths: impl IntoIterator<Item = &'a Path>) -> Self {
        let directories = video_paths
            .into_iter()
            .filter_map(|path| path.parent().map(Path::to_path_buf))
            .collect::<HashSet<_>>();
        let mut index = Self::default();

        for directory in directories {
            index
                .directories
                .insert(directory.clone(), index_subtitle_directory(&directory));
        }

        index
    }

    pub fn directory_count(&self) -> usize {
        self.directories.len()
    }
}

pub(crate) fn discover_subtitle_tracks(
    video_path: &Path,
    embedded_streams: &[EmbeddedSubtitleStream],
) -> Vec<DiscoveredSubtitleTrack> {
    let index = SubtitleDirectoryIndex::build([video_path]);
    discover_subtitle_tracks_with_index(video_path, embedded_streams, &index)
}

pub(crate) fn discover_subtitle_tracks_with_index(
    video_path: &Path,
    embedded_streams: &[EmbeddedSubtitleStream],
    index: &SubtitleDirectoryIndex,
) -> Vec<DiscoveredSubtitleTrack> {
    let mut tracks = embedded_streams
        .iter()
        .map(|stream| DiscoveredSubtitleTrack {
            source_kind: "embedded".to_string(),
            file_path: None,
            stream_index: Some(stream.stream_index),
            language: stream.language.clone(),
            subtitle_format: stream.subtitle_format.clone(),
            label: stream.label.clone(),
            is_default: stream.is_default,
            is_forced: stream.is_forced,
            is_hearing_impaired: stream.is_hearing_impaired,
        })
        .collect::<Vec<_>>();

    tracks.extend(discover_external_subtitle_tracks(video_path, index));
    tracks
}

fn discover_external_subtitle_tracks(
    video_path: &Path,
    index: &SubtitleDirectoryIndex,
) -> Vec<DiscoveredSubtitleTrack> {
    let Some(parent) = video_path.parent() else {
        return Vec::new();
    };
    let Some(directory) = index.directories.get(parent) else {
        return Vec::new();
    };

    let video_base_stem = normalize_subtitle_comparison_stem(video_path);
    let video_episode_identity = episode_identity_for_path(video_path);
    directory
        .subtitles
        .iter()
        .filter_map(|subtitle| {
            if !subtitle_matches_video(
                &video_base_stem,
                video_episode_identity,
                &directory.episode_identity_counts,
                &subtitle.parsed,
            ) {
                return None;
            }

            Some(DiscoveredSubtitleTrack {
                source_kind: "external".to_string(),
                file_path: Some(subtitle.path.clone()),
                stream_index: None,
                language: subtitle.parsed.language.clone(),
                subtitle_format: subtitle.subtitle_format.clone(),
                label: subtitle.parsed.label.clone(),
                is_default: subtitle.parsed.is_default,
                is_forced: subtitle.parsed.is_forced,
                is_hearing_impaired: subtitle.parsed.is_hearing_impaired,
            })
        })
        .collect()
}

fn index_subtitle_directory(directory: &Path) -> IndexedSubtitleDirectory {
    let Ok(entries) = fs::read_dir(directory) else {
        return IndexedSubtitleDirectory::default();
    };

    let mut indexed = IndexedSubtitleDirectory::default();
    for path in entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
    {
        if !path.is_file() {
            continue;
        }

        if is_supported_video(&path) {
            if path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("strm"))
                && crate::read_http_strm_reference(&path).is_err()
            {
                continue;
            }
            if let Some(identity) = episode_identity_for_path(&path) {
                *indexed
                    .episode_identity_counts
                    .entry((identity.season_number, identity.episode_number))
                    .or_insert(0) += 1;
            }
            continue;
        }

        if is_supported_subtitle(&path) {
            indexed.subtitles.push(IndexedSubtitleSidecar {
                subtitle_format: extension_lowercase(&path).unwrap_or_else(|| "srt".to_string()),
                parsed: parse_subtitle_sidecar(&path),
                path,
            });
        }
    }

    indexed
        .subtitles
        .sort_by(|left, right| left.path.cmp(&right.path));
    indexed
}

fn subtitle_matches_video(
    video_base_stem: &str,
    video_episode_identity: Option<EpisodeIdentity>,
    identity_counts: &HashMap<(i32, i32), usize>,
    subtitle: &ParsedSubtitleSidecar,
) -> bool {
    if subtitle.base_stem == video_base_stem {
        return true;
    }

    let Some(video_episode_identity) = video_episode_identity else {
        return false;
    };
    let Some(subtitle_episode_identity) = subtitle.episode_identity else {
        return false;
    };

    if subtitle_episode_identity != video_episode_identity {
        return false;
    }

    identity_counts
        .get(&(
            video_episode_identity.season_number,
            video_episode_identity.episode_number,
        ))
        .copied()
        .unwrap_or(0)
        <= 1
}

fn parse_subtitle_sidecar(path: &Path) -> ParsedSubtitleSidecar {
    let normalized_stem = humanize_file_stem(path);
    let raw_tokens = normalized_stem
        .split_whitespace()
        .map(|token| token.to_string())
        .collect::<Vec<_>>();
    let episode_identity = episode_identity_for_path(path);
    let mut tokens = raw_tokens.clone();
    let mut label_tokens = Vec::new();
    let mut language = None;
    let mut is_default = false;
    let mut is_forced = false;
    let mut is_hearing_impaired = false;

    // 外挂字幕常把语言、forced/default 等标记放在文件名结尾；从尾部剥离能更稳地保留真正的资源标题。
    while let Some(token) = tokens.last().cloned() {
        let lowered = token.to_ascii_lowercase();
        if let Some(normalized_language) = normalize_language_suffix(&lowered) {
            language = Some(normalized_language);
            tokens.pop();
            continue;
        }

        if matches!(lowered.as_str(), "default" | "defaults") {
            is_default = true;
            tokens.pop();
            continue;
        }

        if matches!(lowered.as_str(), "forced" | "foreign") {
            is_forced = true;
            tokens.pop();
            continue;
        }

        if matches!(
            lowered.as_str(),
            "sdh" | "cc" | "hi" | "hearing" | "hearing-impaired" | "hearing_impaired"
        ) {
            is_hearing_impaired = true;
            label_tokens.push(token);
            tokens.pop();
            continue;
        }

        if matches!(lowered.as_str(), "sub" | "subs" | "subtitle" | "subtitles") {
            label_tokens.push(token);
            tokens.pop();
            continue;
        }

        break;
    }

    let label = (!label_tokens.is_empty())
        .then(|| label_tokens.into_iter().rev().collect::<Vec<_>>().join(" "));

    ParsedSubtitleSidecar {
        base_stem: tokens.join(" ").to_ascii_lowercase(),
        episode_identity,
        language,
        label,
        is_default,
        is_forced,
        is_hearing_impaired,
    }
}

fn normalize_subtitle_comparison_stem(path: &Path) -> String {
    humanize_file_stem(path).to_ascii_lowercase()
}

fn normalize_language_suffix(token: &str) -> Option<String> {
    match token {
        "zh" | "zho" | "chi" | "chs" | "cht" | "sc" | "tc" | "cn" | "gb" => {
            Some("zh-CN".to_string())
        }
        "zh-cn" | "zh-hans" | "zh_sg" | "zh-sg" => Some("zh-CN".to_string()),
        "zh-tw" | "zh-hant" | "zh-hk" | "zh_tw" | "zh_hk" => Some("zh-TW".to_string()),
        "en" | "eng" => Some("en".to_string()),
        "ja" | "jpn" => Some("ja".to_string()),
        "ko" | "kor" => Some("ko".to_string()),
        "fr" | "fra" | "fre" => Some("fr".to_string()),
        "de" | "ger" | "deu" => Some("de".to_string()),
        "es" | "spa" => Some("es".to_string()),
        _ => None,
    }
}

fn is_supported_subtitle(path: &Path) -> bool {
    matches!(
        extension_lowercase(path).as_deref(),
        Some("srt" | "ass" | "ssa" | "vtt")
    )
}

fn is_supported_video(path: &Path) -> bool {
    matches!(
        extension_lowercase(path).as_deref(),
        Some(
            "mp4"
                | "mkv"
                | "avi"
                | "mov"
                | "m4v"
                | "wmv"
                | "flv"
                | "webm"
                | "mpg"
                | "mpeg"
                | "strm"
        )
    )
}

#[cfg(test)]
mod tests {
    use super::{
        discover_subtitle_tracks, discover_subtitle_tracks_with_index, SubtitleDirectoryIndex,
    };
    use crate::probe::EmbeddedSubtitleStream;
    use std::{fs, path::PathBuf};
    use uuid::Uuid;

    fn temp_dir() -> PathBuf {
        let path = std::env::temp_dir().join(format!("mova-subtitle-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn discover_subtitle_tracks_matches_same_episode_token_in_same_directory() {
        let root = temp_dir();
        let video_path = root.join("show.S01E01.mkv");
        let subtitle_path = root.join("xxxxx.S01E01.zh.srt");
        fs::write(&video_path, b"video").unwrap();
        fs::write(&subtitle_path, b"1\n00:00:00,000 --> 00:00:01,000\nhello").unwrap();

        let tracks = discover_subtitle_tracks(&video_path, &[]);
        let external = tracks
            .iter()
            .find(|track| track.source_kind == "external")
            .unwrap();

        assert_eq!(external.file_path.as_ref(), Some(&subtitle_path));
        assert_eq!(external.language.as_deref(), Some("zh-CN"));
    }

    #[test]
    fn discover_subtitle_tracks_avoids_ambiguous_episode_matches() {
        let root = temp_dir();
        let video_path = root.join("show.1080p.S01E01.mkv");
        fs::write(&video_path, b"video").unwrap();
        fs::write(root.join("show.4k.S01E01.mkv"), b"video").unwrap();
        fs::write(
            root.join("random.S01E01.en.srt"),
            b"1\n00:00:00,000 --> 00:00:01,000\nhello",
        )
        .unwrap();

        let tracks = discover_subtitle_tracks(&video_path, &[]);
        assert!(tracks.iter().all(|track| track.source_kind != "external"));
    }

    #[test]
    fn discover_subtitle_tracks_reacts_to_episode_version_addition_and_removal() {
        let root = temp_dir();
        let video_path = root.join("show.1080p.S01E01.mkv");
        let second_version_path = root.join("show.4k.S01E01.mkv");
        let subtitle_path = root.join("random.S01E01.en.srt");
        fs::write(&video_path, b"video").unwrap();
        fs::write(&subtitle_path, b"1\n00:00:00,000 --> 00:00:01,000\nhello").unwrap();

        let initial_tracks = discover_subtitle_tracks(&video_path, &[]);
        assert!(initial_tracks
            .iter()
            .any(|track| track.file_path.as_ref() == Some(&subtitle_path)));

        fs::write(&second_version_path, b"second video").unwrap();
        let ambiguous_tracks = discover_subtitle_tracks(&video_path, &[]);
        assert!(ambiguous_tracks
            .iter()
            .all(|track| track.file_path.as_ref() != Some(&subtitle_path)));

        fs::remove_file(second_version_path).unwrap();
        let restored_tracks = discover_subtitle_tracks(&video_path, &[]);
        let _ = fs::remove_dir_all(root);
        assert!(restored_tracks
            .iter()
            .any(|track| track.file_path.as_ref() == Some(&subtitle_path)));
    }

    #[test]
    fn discover_subtitle_tracks_marks_hearing_impaired_sidecars() {
        let root = temp_dir();
        let video_path = root.join("movie.mkv");
        let subtitle_path = root.join("movie.en.sdh.srt");
        fs::write(&video_path, b"video").unwrap();
        fs::write(&subtitle_path, b"1\n00:00:00,000 --> 00:00:01,000\nhello").unwrap();

        let tracks = discover_subtitle_tracks(&video_path, &[]);
        let external = tracks
            .iter()
            .find(|track| track.source_kind == "external")
            .unwrap();

        assert_eq!(external.file_path.as_ref(), Some(&subtitle_path));
        assert_eq!(external.label.as_deref(), Some("sdh"));
        assert!(external.is_hearing_impaired);
    }

    #[test]
    fn discover_subtitle_tracks_keeps_embedded_streams() {
        let root = temp_dir();
        let video_path = root.join("movie.mp4");
        fs::write(&video_path, b"video").unwrap();

        let tracks = discover_subtitle_tracks(
            &video_path,
            &[EmbeddedSubtitleStream {
                stream_index: 3,
                language: Some("en".to_string()),
                subtitle_format: "mov_text".to_string(),
                label: Some("English".to_string()),
                is_default: true,
                is_forced: false,
                is_hearing_impaired: false,
            }],
        );

        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].stream_index, Some(3));
    }

    #[test]
    fn directory_index_is_reused_for_multiple_videos() {
        let root = temp_dir();
        let first_video = root.join("show.S01E01.mkv");
        let second_video = root.join("show.S01E02.mkv");
        let first_subtitle = root.join("show.S01E01.zh.srt");
        let second_subtitle = root.join("show.S01E02.en.srt");
        fs::write(&first_video, b"video").unwrap();
        fs::write(&second_video, b"video").unwrap();
        fs::write(&first_subtitle, b"first").unwrap();
        fs::write(&second_subtitle, b"second").unwrap();

        let index = SubtitleDirectoryIndex::build([first_video.as_path(), second_video.as_path()]);
        assert_eq!(index.directory_count(), 1);
        let first_tracks = discover_subtitle_tracks_with_index(&first_video, &[], &index);
        let second_tracks = discover_subtitle_tracks_with_index(&second_video, &[], &index);

        assert_eq!(first_tracks.len(), 1);
        assert_eq!(first_tracks[0].file_path.as_ref(), Some(&first_subtitle));
        assert_eq!(second_tracks.len(), 1);
        assert_eq!(second_tracks[0].file_path.as_ref(), Some(&second_subtitle));
    }

    #[test]
    fn directory_index_size_depends_on_unique_directories_not_video_count() {
        let root = temp_dir();
        let video_paths = (0..200)
            .map(|episode| {
                root.join(format!("season-{}", episode % 4))
                    .join(format!("show.S01E{episode:03}.mkv"))
            })
            .collect::<Vec<_>>();

        let index = SubtitleDirectoryIndex::build(video_paths.iter().map(PathBuf::as_path));

        assert_eq!(index.directory_count(), 4);
    }
}
