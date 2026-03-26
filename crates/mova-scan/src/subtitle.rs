use crate::{
    parse::{episode_identity_for_path, extension_lowercase, humanize_file_stem, EpisodeIdentity},
    probe::EmbeddedSubtitleStream,
    DiscoveredSubtitleTrack,
};
use std::{
    collections::HashMap,
    fs,
    path::Path,
};

#[derive(Debug, Clone)]
struct ParsedSubtitleSidecar {
    base_stem: String,
    episode_identity: Option<EpisodeIdentity>,
    language: Option<String>,
    label: Option<String>,
    is_default: bool,
    is_forced: bool,
}

pub(crate) fn discover_subtitle_tracks(
    video_path: &Path,
    embedded_streams: &[EmbeddedSubtitleStream],
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
        })
        .collect::<Vec<_>>();

    tracks.extend(discover_external_subtitle_tracks(video_path));
    tracks
}

fn discover_external_subtitle_tracks(video_path: &Path) -> Vec<DiscoveredSubtitleTrack> {
    let Some(parent) = video_path.parent() else {
        return Vec::new();
    };

    let Ok(entries) = fs::read_dir(parent) else {
        return Vec::new();
    };

    let video_base_stem = normalize_subtitle_comparison_stem(video_path);
    let video_episode_identity = episode_identity_for_path(video_path);
    let identity_counts = collect_video_episode_identity_counts(parent);

    entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter(|path| is_supported_subtitle(path))
        .filter_map(|path| {
            let parsed = parse_subtitle_sidecar(&path);
            let subtitle_format = extension_lowercase(&path).unwrap_or_else(|| "srt".to_string());
            if !subtitle_matches_video(
                &video_base_stem,
                video_episode_identity,
                &identity_counts,
                &parsed,
            ) {
                return None;
            }

            Some(DiscoveredSubtitleTrack {
                source_kind: "external".to_string(),
                file_path: Some(path),
                stream_index: None,
                language: parsed.language,
                subtitle_format,
                label: parsed.label,
                is_default: parsed.is_default,
                is_forced: parsed.is_forced,
            })
        })
        .collect()
}

fn collect_video_episode_identity_counts(directory: &Path) -> HashMap<(i32, i32), usize> {
    let Ok(entries) = fs::read_dir(directory) else {
        return HashMap::new();
    };

    let mut counts = HashMap::new();
    for path in entries.filter_map(|entry| entry.ok()).map(|entry| entry.path()) {
        if !path.is_file() || !is_supported_video(&path) {
            continue;
        }

        if let Some(identity) = episode_identity_for_path(&path) {
            *counts
                .entry((identity.season_number, identity.episode_number))
                .or_insert(0) += 1;
        }
    }

    counts
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
        .get(&(video_episode_identity.season_number, video_episode_identity.episode_number))
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

        if matches!(lowered.as_str(), "sdh" | "cc" | "sub" | "subs" | "subtitle" | "subtitles") {
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
    matches!(extension_lowercase(path).as_deref(), Some("srt" | "ass" | "ssa" | "vtt"))
}

fn is_supported_video(path: &Path) -> bool {
    matches!(
        extension_lowercase(path).as_deref(),
        Some("mp4" | "mkv" | "avi" | "mov" | "m4v" | "wmv" | "flv" | "webm" | "mpg" | "mpeg")
    )
}

#[cfg(test)]
mod tests {
    use super::discover_subtitle_tracks;
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
            }],
        );

        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].stream_index, Some(3));
    }
}
