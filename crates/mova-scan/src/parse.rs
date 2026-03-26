use super::sidecar::{find_local_artwork, read_sidecar_metadata, ArtworkKind};
use std::path::Path;

/// 根据文件名和目录结构判断某个视频路径是否更像剧集文件。
pub fn is_likely_episode_path(path: &Path) -> bool {
    let normalized_stem = humanize_file_stem(path);
    if normalized_stem
        .split_whitespace()
        .any(|token| is_episode_token(token))
    {
        return true;
    }

    path.ancestors()
        .skip(1)
        .filter_map(|ancestor| ancestor.file_name().and_then(|value| value.to_str()))
        .any(is_likely_season_component)
}

pub(crate) fn extension_lowercase(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
}

pub(crate) fn humanize_file_stem(path: &Path) -> String {
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("unknown");

    let normalized = stem
        .chars()
        .map(|ch| match ch {
            '.' | '_' => ' ',
            other => other,
        })
        .collect::<String>();

    normalized.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ParsedMediaMetadata {
    pub title: String,
    pub source_title: String,
    pub original_title: Option<String>,
    pub sort_title: Option<String>,
    pub year: Option<i32>,
    pub season_number: Option<i32>,
    pub season_title: Option<String>,
    pub season_overview: Option<String>,
    pub season_poster_path: Option<String>,
    pub season_backdrop_path: Option<String>,
    pub episode_number: Option<i32>,
    pub episode_title: Option<String>,
    pub overview: Option<String>,
    pub series_poster_path: Option<String>,
    pub series_backdrop_path: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
}

pub(crate) fn parse_media_metadata(path: &Path) -> ParsedMediaMetadata {
    let parsed_name = parse_media_name(path);
    let sidecar = read_sidecar_metadata(path);
    let episode_identity = parse_episode_identity(path);
    let poster_path = find_local_artwork(path, ArtworkKind::Poster).or(sidecar.poster_path);
    let backdrop_path = find_local_artwork(path, ArtworkKind::Backdrop).or(sidecar.backdrop_path);

    ParsedMediaMetadata {
        title: sidecar
            .title
            .clone()
            .unwrap_or_else(|| parsed_name.title.clone()),
        source_title: parsed_name.title,
        original_title: sidecar.original_title,
        sort_title: sidecar.sort_title,
        year: sidecar.year.or(parsed_name.year),
        season_number: episode_identity
            .as_ref()
            .map(|identity| identity.season_number),
        season_title: None,
        season_overview: None,
        season_poster_path: None,
        season_backdrop_path: None,
        episode_number: episode_identity
            .as_ref()
            .map(|identity| identity.episode_number),
        episode_title: episode_identity.and_then(|identity| identity.episode_title),
        overview: sidecar.overview,
        series_poster_path: poster_path.clone(),
        series_backdrop_path: backdrop_path.clone(),
        poster_path,
        backdrop_path,
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ParsedNameMetadata {
    title: String,
    year: Option<i32>,
}

fn parse_media_name(path: &Path) -> ParsedNameMetadata {
    let normalized = humanize_file_stem(path);
    let tokens = normalized.split_whitespace().collect::<Vec<_>>();

    let mut title_start = 0;
    let mut title_end = tokens.len();
    let mut year = None;

    for (index, token) in tokens.iter().enumerate() {
        if let Some(parsed_year) = parse_year_token(token) {
            year = Some(parsed_year);
            title_end = index;
            break;
        }

        if is_episode_token(token) || is_release_token(token) {
            title_end = index;
            break;
        }
    }

    while title_start < title_end && is_separator_token(tokens[title_start]) {
        title_start += 1;
    }

    while title_end > title_start && is_separator_token(tokens[title_end - 1]) {
        title_end -= 1;
    }

    let title = tokens[title_start..title_end].join(" ");
    let fallback_title = normalized.clone();

    ParsedNameMetadata {
        title: if title.is_empty() {
            fallback_title
        } else {
            title
        },
        year,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedEpisodeIdentity {
    season_number: i32,
    episode_number: i32,
    episode_title: Option<String>,
}

fn parse_episode_identity(path: &Path) -> Option<ParsedEpisodeIdentity> {
    let normalized = humanize_file_stem(path);
    let tokens = normalized.split_whitespace().collect::<Vec<_>>();
    let (token_index, season_number, episode_number) =
        tokens.iter().enumerate().find_map(|(index, token)| {
            parse_episode_token(token).map(|(season, episode)| (index, season, episode))
        })?;

    let mut title_end = tokens.len();
    for (index, token) in tokens.iter().enumerate().skip(token_index + 1) {
        if parse_year_token(token).is_some() || is_release_token(token) {
            title_end = index;
            break;
        }
    }

    let mut title_start = token_index + 1;
    while title_start < title_end && is_separator_token(tokens[title_start]) {
        title_start += 1;
    }

    while title_end > title_start && is_separator_token(tokens[title_end - 1]) {
        title_end -= 1;
    }

    let episode_title = (title_start < title_end)
        .then(|| tokens[title_start..title_end].join(" "))
        .filter(|value| !value.is_empty());

    Some(ParsedEpisodeIdentity {
        season_number,
        episode_number,
        episode_title,
    })
}

fn is_separator_token(token: &str) -> bool {
    !token.is_empty()
        && token.chars().all(|ch| {
            matches!(
                ch,
                '-' | '|' | ':' | '：' | '·' | '•' | '~' | '–' | '—' | '/' | '\\'
            )
        })
}

pub(crate) fn parse_year_token(token: &str) -> Option<i32> {
    let token = token.trim_matches(|ch| {
        matches!(
            ch,
            '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | '（' | '）' | '【' | '】' | '《' | '》'
        )
    });

    if token.len() != 4 || !token.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }

    let year = token.parse::<i32>().ok()?;
    (1900..=2100).contains(&year).then_some(year)
}

fn is_episode_token(token: &str) -> bool {
    parse_episode_token(token).is_some()
}

fn parse_episode_token(token: &str) -> Option<(i32, i32)> {
    parse_series_token(token).or_else(|| parse_x_episode_token(token))
}

fn parse_series_token(token: &str) -> Option<(i32, i32)> {
    let bytes = token.as_bytes();
    if bytes.len() < 4 || bytes[0] != b'S' && bytes[0] != b's' {
        return None;
    }

    let e_position = bytes.iter().position(|byte| *byte == b'E' || *byte == b'e');

    let Some(e_position) = e_position else {
        return None;
    };

    if !(e_position > 1
        && e_position < bytes.len() - 1
        && bytes[1..e_position].iter().all(u8::is_ascii_digit)
        && bytes[e_position + 1..].iter().all(u8::is_ascii_digit))
    {
        return None;
    }

    let season_number = std::str::from_utf8(&bytes[1..e_position])
        .ok()?
        .parse::<i32>()
        .ok()?;
    let episode_number = std::str::from_utf8(&bytes[e_position + 1..])
        .ok()?
        .parse::<i32>()
        .ok()?;

    Some((season_number, episode_number))
}

fn parse_x_episode_token(token: &str) -> Option<(i32, i32)> {
    let lower = token.to_ascii_lowercase();
    let Some(separator_index) = lower.find('x') else {
        return None;
    };

    if !(separator_index > 0
        && separator_index < lower.len() - 1
        && lower[..separator_index]
            .chars()
            .all(|ch| ch.is_ascii_digit())
        && lower[separator_index + 1..]
            .chars()
            .all(|ch| ch.is_ascii_digit()))
    {
        return None;
    }

    let season_number = lower[..separator_index].parse::<i32>().ok()?;
    let episode_number = lower[separator_index + 1..].parse::<i32>().ok()?;

    Some((season_number, episode_number))
}

fn is_likely_season_component(component: &str) -> bool {
    let normalized = component
        .chars()
        .map(|ch| match ch {
            '.' | '_' | '-' => ' ',
            other => other,
        })
        .collect::<String>()
        .to_ascii_lowercase();
    let tokens = normalized.split_whitespace().collect::<Vec<_>>();

    matches!(tokens.as_slice(), ["season", number] if is_short_number_token(number))
        || matches!(tokens.as_slice(), [token] if token.starts_with('s') && is_short_number_token(&token[1..]))
}

fn is_short_number_token(token: &str) -> bool {
    !token.is_empty() && token.len() <= 3 && token.chars().all(|ch| ch.is_ascii_digit())
}

fn is_release_token(token: &str) -> bool {
    let token = token.to_ascii_lowercase();

    matches!(
        token.as_str(),
        "2160p"
            | "1080p"
            | "720p"
            | "480p"
            | "x264"
            | "x265"
            | "h264"
            | "h265"
            | "hevc"
            | "bluray"
            | "bdrip"
            | "webrip"
            | "webdl"
            | "web-dl"
            | "hdrip"
            | "dvdrip"
            | "remux"
            | "aac"
            | "dts"
            | "10bit"
            | "8bit"
    )
}
