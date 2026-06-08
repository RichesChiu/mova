use super::sidecar::{
    find_local_artwork, find_local_artwork_with_scope, read_sidecar_metadata, ArtworkKind,
    ArtworkScope,
};
use std::path::Path;

/// 根据文件名和目录结构判断某个视频路径是否更像剧集文件。
pub fn is_likely_episode_path(path: &Path) -> bool {
    parse_episode_identity(path).is_some()
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
        .map(|ch| {
            if is_filename_word_separator_char(ch) {
                ' '
            } else {
                ch
            }
        })
        .collect::<String>();

    decode_basic_html_entities(&normalized)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EpisodeIdentity {
    pub season_number: i32,
    pub episode_number: i32,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeriesFileMetadata {
    pub display_title: String,
    pub title: String,
    pub year: Option<i32>,
}

pub(crate) fn parse_media_metadata(path: &Path) -> ParsedMediaMetadata {
    let parsed_name = parse_media_name(path);
    let sidecar = read_sidecar_metadata(path);
    let episode_identity = parse_episode_identity(path);
    let is_episode = episode_identity.is_some();
    let file_poster_path =
        find_local_artwork_with_scope(path, ArtworkKind::Poster, ArtworkScope::FileSpecific);
    let file_backdrop_path =
        find_local_artwork_with_scope(path, ArtworkKind::Backdrop, ArtworkScope::FileSpecific);
    let generic_poster_path =
        find_local_artwork_with_scope(path, ArtworkKind::Poster, ArtworkScope::Generic);
    let generic_backdrop_path =
        find_local_artwork_with_scope(path, ArtworkKind::Backdrop, ArtworkScope::Generic);
    let poster_path = if is_episode {
        file_poster_path
    } else {
        find_local_artwork(path, ArtworkKind::Poster).or(sidecar.poster_path)
    };
    let backdrop_path = if is_episode {
        file_backdrop_path
    } else {
        find_local_artwork(path, ArtworkKind::Backdrop).or(sidecar.backdrop_path)
    };
    let series_poster_path = if is_episode {
        generic_poster_path
    } else {
        poster_path.clone()
    };
    let series_backdrop_path = if is_episode {
        generic_backdrop_path
    } else {
        backdrop_path.clone()
    };

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
        series_poster_path,
        series_backdrop_path,
        poster_path,
        backdrop_path,
    }
}

pub(crate) fn parse_media_metadata_without_sidecar(path: &Path) -> ParsedMediaMetadata {
    let parsed_name = parse_media_name(path);
    let episode_identity = parse_episode_identity(path);

    ParsedMediaMetadata {
        title: parsed_name.title.clone(),
        source_title: parsed_name.title,
        original_title: None,
        sort_title: None,
        year: parsed_name.year,
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
        overview: None,
        series_poster_path: None,
        series_backdrop_path: None,
        poster_path: None,
        backdrop_path: None,
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ParsedNameMetadata {
    title: String,
    year: Option<i32>,
}

fn parse_media_name(path: &Path) -> ParsedNameMetadata {
    let normalized = humanize_file_stem(path);
    let has_leading_collection_index = has_leading_collection_index(path);
    let parsed_name = parse_title_year_from_humanized_name(&normalized);
    let mut title = parsed_name.title.clone();
    let year = parsed_name.year;

    if has_leading_collection_index {
        title = strip_leading_collection_index(&title);
    }

    ParsedNameMetadata {
        title: if title.is_empty() { normalized } else { title },
        year,
    }
}

pub fn infer_series_file_metadata(path: &Path) -> Option<SeriesFileMetadata> {
    let normalized = humanize_file_stem(path);
    let tokens = normalized.split_whitespace().collect::<Vec<_>>();
    let (episode_token_index, episode_token) = tokens
        .iter()
        .enumerate()
        .find_map(|(index, token)| parse_episode_token_marker(token).map(|token| (index, token)))?;
    let mut title_end = episode_token_index;
    let mut title_tokens = tokens[..title_end]
        .iter()
        .map(|token| (*token).to_string())
        .collect::<Vec<_>>();

    if let Some(prefix) = episode_token.title_prefix {
        title_tokens.push(prefix);
        title_end = title_tokens.len();
    }

    while title_end > 0 && is_separator_token(&title_tokens[title_end - 1]) {
        title_end -= 1;
    }

    if title_end == 0 {
        return None;
    }

    let display_title = title_tokens[..title_end].join(" ");
    let parsed_name = parse_title_year_from_humanized_name(&display_title);

    if parsed_name.title.trim().is_empty()
        || is_generic_library_folder(&parsed_name.title)
        || is_collection_folder_title(&parsed_name.title)
    {
        return None;
    }

    Some(SeriesFileMetadata {
        display_title,
        title: parsed_name.title,
        year: parsed_name
            .year
            .or_else(|| parse_year_after_episode_token(&tokens, episode_token_index + 1)),
    })
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
    let (_, title_start, season_number, episode_number) =
        tokens.iter().enumerate().find_map(|(index, token)| {
            parse_episode_token_marker(token).and_then(|episode_token| {
                let has_title_prefix = episode_token
                    .title_prefix
                    .as_deref()
                    .is_some_and(|value| !value.trim().is_empty());
                let has_title_before = tokens[..index]
                    .iter()
                    .any(|value| !is_separator_token(value));
                (has_title_prefix || has_title_before).then_some((
                    index,
                    index + 1,
                    episode_token.season_number,
                    episode_token.episode_number,
                ))
            })
        })?;

    let mut title_end = tokens.len();
    for (index, token) in tokens.iter().enumerate().skip(title_start) {
        if parse_year_token(token).is_some() || is_release_token(token) {
            title_end = index;
            break;
        }
    }

    let mut normalized_title_start = title_start;
    while normalized_title_start < title_end && is_separator_token(tokens[normalized_title_start]) {
        normalized_title_start += 1;
    }

    while title_end > normalized_title_start && is_separator_token(tokens[title_end - 1]) {
        title_end -= 1;
    }

    let episode_title = (normalized_title_start < title_end)
        .then(|| tokens[normalized_title_start..title_end].join(" "))
        .filter(|value| !value.is_empty())
        .filter(|value| !is_generic_episode_title(value, episode_number));

    Some(ParsedEpisodeIdentity {
        season_number,
        episode_number,
        episode_title,
    })
}

pub(crate) fn episode_identity_for_path(path: &Path) -> Option<EpisodeIdentity> {
    parse_episode_identity(path).map(|identity| EpisodeIdentity {
        season_number: identity.season_number,
        episode_number: identity.episode_number,
    })
}

fn is_separator_token(token: &str) -> bool {
    !token.is_empty() && token.chars().all(is_separator_token_char)
}

fn is_filename_word_separator_char(ch: char) -> bool {
    matches!(ch, '.' | '_' | '-' | '–' | '—')
}

fn is_separator_token_char(ch: char) -> bool {
    matches!(
        ch,
        '-' | '|' | ':' | '：' | '·' | '•' | '~' | '–' | '—' | '/' | '\\'
    )
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedEpisodeToken {
    title_prefix: Option<String>,
    season_number: i32,
    episode_number: i32,
}

fn parse_episode_token_marker(token: &str) -> Option<ParsedEpisodeToken> {
    if let Some((season_number, episode_number)) =
        parse_series_token(token).or_else(|| parse_x_episode_token(token))
    {
        return Some(ParsedEpisodeToken {
            title_prefix: None,
            season_number,
            episode_number,
        });
    }

    parse_embedded_episode_token(token)
}

fn parse_embedded_episode_token(token: &str) -> Option<ParsedEpisodeToken> {
    for (index, _) in token.char_indices().skip(1) {
        let prefix = &token[..index];
        if !prefix.chars().any(|ch| ch.is_alphanumeric()) {
            continue;
        }

        let suffix = &token[index..];
        if let Some((season_number, episode_number)) =
            parse_series_token(suffix).or_else(|| parse_x_episode_token(suffix))
        {
            return Some(ParsedEpisodeToken {
                title_prefix: Some(prefix.to_string()),
                season_number,
                episode_number,
            });
        }
    }

    None
}

fn parse_year_after_episode_token<T: AsRef<str>>(tokens: &[T], start_index: usize) -> Option<i32> {
    tokens
        .iter()
        .skip(start_index)
        .find_map(|token| parse_year_token(token.as_ref()))
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

fn is_generic_episode_title(value: &str, episode_number: i32) -> bool {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let lower = normalized.to_ascii_lowercase();

    lower == format!("episode {episode_number}")
        || lower == format!("ep {episode_number}")
        || normalized == format!("第 {episode_number} 集")
        || normalized == format!("第{episode_number}集")
}

fn decode_basic_html_entities(value: &str) -> String {
    value
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&quot;", "\"")
        .replace("&amp;", "&")
}

fn is_generic_library_folder(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "movie"
            | "movies"
            | "film"
            | "films"
            | "media"
            | "video"
            | "videos"
            | "series"
            | "shows"
            | "tv"
            | "tv shows"
    ) || matches!(value.trim(), "电影" | "剧集" | "电视剧" | "动画" | "动漫")
}

fn is_collection_folder_title(value: &str) -> bool {
    let normalized = value.trim().to_ascii_lowercase();

    normalized.contains("collection")
        || normalized.contains("box set")
        || normalized.contains("boxset")
        || normalized.contains("anthology")
        || normalized.contains("trilogy")
        || normalized.contains("tetralogy")
        || normalized.contains("saga")
        || matches!(
            value.trim(),
            value if value.contains("合集") || value.contains("全集") || value.contains("系列")
        )
}

fn has_leading_collection_index(path: &Path) -> bool {
    let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
        return false;
    };

    let digit_count = stem.chars().take_while(|ch| ch.is_ascii_digit()).count();
    if digit_count == 0 || digit_count > 3 {
        return false;
    }

    let mut chars = stem.chars().skip(digit_count);
    let Some(separator) = chars.next() else {
        return false;
    };

    matches!(separator, '.' | '_' | '-' | ' ' | '、')
        && chars.next().is_some_and(|ch| !ch.is_ascii_digit())
}

fn strip_leading_collection_index(title: &str) -> String {
    let tokens = title.split_whitespace().collect::<Vec<_>>();

    if tokens.len() >= 3
        && tokens
            .first()
            .is_some_and(|token| parse_short_number_token(token).is_some())
    {
        return tokens[1..].join(" ");
    }

    title.to_string()
}

fn parse_title_year_from_humanized_name(value: &str) -> ParsedNameMetadata {
    let mut tokens = value
        .split_whitespace()
        .map(std::string::ToString::to_string)
        .collect::<Vec<_>>();
    let mut title_start = 0;
    let mut title_end = tokens.len();
    let mut year = None;

    for index in 0..tokens.len() {
        if let Some(parsed_year) = parse_year_token(tokens[index].as_str()) {
            year = Some(parsed_year);
            title_end = index;
            break;
        }

        if let Some((prefix, parsed_year)) = split_trailing_year_suffix(tokens[index].as_str()) {
            year = Some(parsed_year);
            tokens[index] = prefix;
            title_end = index + 1;
            break;
        }

        if let Some(episode_token) = parse_episode_token_marker(tokens[index].as_str()) {
            if let Some(prefix) = episode_token.title_prefix {
                tokens[index] = prefix;
                title_end = index + 1;
            } else {
                title_end = index;
            }
            year = year.or_else(|| parse_year_after_episode_token(&tokens, index + 1));
            break;
        }

        if is_release_token(tokens[index].as_str()) {
            title_end = index;
            break;
        }
    }

    while title_start < title_end && is_separator_token(tokens[title_start].as_str()) {
        title_start += 1;
    }

    while title_end > title_start && is_separator_token(tokens[title_end - 1].as_str()) {
        title_end -= 1;
    }

    let title = tokens[title_start..title_end].join(" ");

    ParsedNameMetadata {
        title: if title.is_empty() {
            value.to_string()
        } else {
            title
        },
        year,
    }
}

fn split_trailing_year_suffix(token: &str) -> Option<(String, i32)> {
    let trimmed = trim_wrapping_punctuation(token);
    let characters = trimmed.chars().collect::<Vec<_>>();

    if characters.len() <= 4 {
        return None;
    }

    let suffix = characters[characters.len() - 4..]
        .iter()
        .collect::<String>();
    let year = parse_year_token(&suffix)?;
    let prefix = characters[..characters.len() - 4]
        .iter()
        .collect::<String>();

    if prefix.is_empty() || prefix.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }

    Some((prefix, year))
}

fn is_short_number_token(token: &str) -> bool {
    !token.is_empty() && token.len() <= 3 && token.chars().all(|ch| ch.is_ascii_digit())
}

fn parse_short_number_token(token: &str) -> Option<i32> {
    let trimmed = trim_wrapping_punctuation(token);
    is_short_number_token(trimmed)
        .then(|| trimmed.parse::<i32>().ok())
        .flatten()
}

fn trim_wrapping_punctuation(token: &str) -> &str {
    token.trim_matches(|ch| {
        matches!(
            ch,
            '(' | ')'
                | '['
                | ']'
                | '{'
                | '}'
                | '<'
                | '>'
                | '（'
                | '）'
                | '【'
                | '】'
                | '《'
                | '》'
                | '"'
                | '\''
        )
    })
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
