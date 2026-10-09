use super::discover::is_supported_video;
#[cfg(test)]
use super::sidecar::read_sidecar_metadata;
use super::sidecar::{
    find_local_artwork, find_local_artwork_with_scope, find_local_episode_thumbnail,
    find_local_season_artwork, find_local_series_artwork, is_movie_nfo_file,
    read_series_sidecar_metadata, read_series_sidecar_metadata_within_root, ArtworkKind,
    ArtworkScope, LocalNfoKind, LocalNfoMetadata,
};
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

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

#[derive(Debug, PartialEq)]
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
    pub episode_overview: Option<String>,
    pub overview: Option<String>,
    pub series_poster_path: Option<String>,
    pub series_backdrop_path: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub local_nfo: Option<LocalNfoMetadata>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeriesFileMetadata {
    pub display_title: String,
    pub title: String,
    pub season_number: i32,
    /// 只有第一季文件中的年份才能直接表示系列首播年。
    pub year: Option<i32>,
    /// 后续季文件中的年份只表示该季播出年，不能覆盖系列首播年。
    pub season_air_year: Option<i32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SeriesSidecarMetadata {
    pub title: Option<String>,
    pub year: Option<i32>,
    pub local_nfo: LocalNfoMetadata,
}

/// 从媒体文件所在容器目录解析出的本地作品身份。
///
/// `display_title` 保留目录中的年份展示文本，但会移除显式 TMDB 标记；
/// `title` 是用于查询的无年份标题。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaContainerIdentity {
    pub container_path: PathBuf,
    pub display_title: String,
    pub title: String,
    pub year: Option<i32>,
    pub tmdb_id: Option<String>,
}

#[cfg(test)]
pub(crate) fn parse_media_metadata(path: &Path) -> ParsedMediaMetadata {
    parse_media_metadata_with_sidecar(path, read_sidecar_metadata(path))
}

pub(crate) fn parse_media_metadata_with_sidecar(
    path: &Path,
    sidecar: Option<LocalNfoMetadata>,
) -> ParsedMediaMetadata {
    let parsed_name = parse_media_name(path);
    let episode_identity = parse_episode_identity(path);
    let is_episode = episode_identity.is_some()
        || sidecar
            .as_ref()
            .is_some_and(|metadata| metadata.kind == LocalNfoKind::Episode);
    let parsed_season_number = episode_identity
        .as_ref()
        .map(|identity| identity.season_number);
    let file_poster_path =
        find_local_artwork_with_scope(path, ArtworkKind::Poster, ArtworkScope::FileSpecific);
    let file_backdrop_path =
        find_local_artwork_with_scope(path, ArtworkKind::Backdrop, ArtworkScope::FileSpecific);
    let nfo_poster_path = sidecar.as_ref().and_then(|metadata| {
        if is_episode {
            metadata
                .artwork
                .thumbnails
                .first()
                .or_else(|| metadata.artwork.posters.first())
                .cloned()
        } else {
            metadata.artwork.posters.first().cloned()
        }
    });
    let nfo_backdrop_path = sidecar
        .as_ref()
        .and_then(|metadata| metadata.artwork.backdrops.first().cloned());
    let poster_path = if is_episode {
        nfo_poster_path
            .or_else(|| find_local_episode_thumbnail(path))
            .or(file_poster_path)
    } else {
        nfo_poster_path.or_else(|| find_local_artwork(path, ArtworkKind::Poster))
    };
    let backdrop_path = if is_episode {
        nfo_backdrop_path.or(file_backdrop_path)
    } else {
        nfo_backdrop_path.or_else(|| find_local_artwork(path, ArtworkKind::Backdrop))
    };
    let series_poster_path = if is_episode {
        find_local_series_artwork(path, ArtworkKind::Poster)
    } else {
        poster_path.clone()
    };
    let series_backdrop_path = if is_episode {
        find_local_series_artwork(path, ArtworkKind::Backdrop)
    } else {
        backdrop_path.clone()
    };
    let season_poster_path = parsed_season_number
        .and_then(|season| find_local_season_artwork(path, season, ArtworkKind::Poster));
    let season_backdrop_path = parsed_season_number
        .and_then(|season| find_local_season_artwork(path, season, ArtworkKind::Backdrop));

    let nfo_overview = sidecar.as_ref().and_then(|metadata| {
        metadata
            .overview
            .clone()
            .or_else(|| metadata.outline.clone())
    });
    let parsed_episode_number = episode_identity
        .as_ref()
        .map(|identity| identity.episode_number);
    let parsed_episode_title = episode_identity
        .as_ref()
        .and_then(|identity| identity.episode_title.clone());

    ParsedMediaMetadata {
        title: if is_episode {
            parsed_name.title.clone()
        } else {
            sidecar
                .as_ref()
                .and_then(|metadata| metadata.title.clone())
                .unwrap_or_else(|| parsed_name.title.clone())
        },
        source_title: parsed_name.title,
        original_title: sidecar
            .as_ref()
            .and_then(|metadata| metadata.original_title.clone()),
        sort_title: sidecar
            .as_ref()
            .and_then(|metadata| metadata.sort_title.clone()),
        year: sidecar
            .as_ref()
            .and_then(|metadata| metadata.year)
            .or(parsed_name.year),
        season_number: parsed_season_number
            .or_else(|| sidecar.as_ref().and_then(|metadata| metadata.season_number)),
        season_title: None,
        season_overview: None,
        season_poster_path,
        season_backdrop_path,
        episode_number: parsed_episode_number.or_else(|| {
            sidecar
                .as_ref()
                .and_then(|metadata| metadata.episode_number)
        }),
        episode_title: if is_episode {
            sidecar
                .as_ref()
                .and_then(|metadata| metadata.title.clone())
                .or(parsed_episode_title)
        } else {
            None
        },
        episode_overview: is_episode.then_some(nfo_overview.clone()).flatten(),
        overview: (!is_episode).then_some(nfo_overview).flatten(),
        series_poster_path,
        series_backdrop_path,
        poster_path,
        backdrop_path,
        local_nfo: sidecar,
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
        episode_overview: None,
        overview: None,
        series_poster_path: None,
        series_backdrop_path: None,
        poster_path: None,
        backdrop_path: None,
        local_nfo: None,
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ParsedNameMetadata {
    title: String,
    year: Option<i32>,
    has_meaningful_title: bool,
}

fn parse_media_name(path: &Path) -> ParsedNameMetadata {
    let normalized = humanized_stem_for_identity(path);
    let has_leading_sequence_index = has_leading_sequence_index(path);
    let parsed_name = parse_title_year_from_humanized_name(&normalized);
    let mut title = parsed_name.title.clone();
    let year = parsed_name.year;

    if has_leading_sequence_index {
        title = strip_leading_sequence_index(&title);
    }

    ParsedNameMetadata {
        title: if title.is_empty() { normalized } else { title },
        year,
        has_meaningful_title: parsed_name.has_meaningful_title,
    }
}

pub fn infer_series_file_metadata(path: &Path) -> Option<SeriesFileMetadata> {
    let normalized = humanized_stem_for_identity(path);
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

    let parsed_year = parsed_name
        .year
        .or_else(|| parse_year_after_episode_token(&tokens, episode_token_index + 1));
    let is_first_season = episode_token.season_number == 1;

    Some(SeriesFileMetadata {
        display_title,
        title: parsed_name.title,
        season_number: episode_token.season_number,
        year: is_first_season.then_some(parsed_year).flatten(),
        season_air_year: (!is_first_season).then_some(parsed_year).flatten(),
    })
}

/// 判断文件名在年份、季集标记和技术标签之前是否包含可用的作品标题。
///
/// 该判断只检查文件名，不读取或猜测任何父目录。
pub fn has_meaningful_file_title(path: &Path) -> bool {
    let parsed_name = parse_media_name(path);
    parsed_name.has_meaningful_title
        && is_valid_container_title(&parsed_name.title)
        && !is_season_directory_name(&parsed_name.title)
        && !is_technical_directory_name(&parsed_name.title)
}

/// 从媒体库根目录内的受限父目录链解析纯季集文件的容器身份。
///
/// 只跳过明确的季目录和技术版本目录。遇到第一个非结构目录后立即停止；
/// 该目录无效时不会继续向更高层寻找，媒体库根目录本身也不会成为候选。
pub fn infer_series_container_identity(
    path: &Path,
    root_path: &Path,
) -> Option<MediaContainerIdentity> {
    parse_episode_identity(path)?;
    let mut candidate_path = relative_media_parent(path, root_path)?;

    loop {
        let directory = candidate_path.file_name()?.to_str()?;

        if is_season_directory_name(directory) || is_technical_directory_name(directory) {
            candidate_path = candidate_path.parent()?;
            if candidate_path.as_os_str().is_empty() {
                return None;
            }
            continue;
        }

        return parse_container_identity(directory, candidate_path);
    }
}

/// 从媒体库根目录内的电影直接父目录解析容器身份。
///
/// 本函数不向上跳层。调用方应结合 [`has_meaningful_file_title`]，只在文件名
/// 没有可用作品标题时把该结果提升为主身份。
pub fn infer_movie_container_identity(
    path: &Path,
    root_path: &Path,
) -> Option<MediaContainerIdentity> {
    if parse_episode_identity(path).is_some() {
        return None;
    }

    let relative_parent = relative_media_parent(path, root_path)?;
    let directory = relative_parent.file_name()?.to_str()?;

    if is_season_directory_name(directory) || is_technical_directory_name(directory) {
        return None;
    }

    parse_container_identity(directory, relative_parent)
}

/// 读取剧集路径最近的 `tvshow.nfo` 身份字段。目录名称不会参与该结果。
pub fn infer_series_sidecar_metadata(path: &Path) -> Option<SeriesSidecarMetadata> {
    let metadata = read_series_sidecar_metadata(path);
    series_sidecar_metadata_from_parsed(metadata)
}

/// 在媒体库根目录边界内读取最近的 `tvshow.nfo` 身份字段。
pub fn infer_series_sidecar_metadata_within_root(
    path: &Path,
    root_path: &Path,
) -> Option<SeriesSidecarMetadata> {
    let metadata = read_series_sidecar_metadata_within_root(path, root_path);
    series_sidecar_metadata_from_parsed(metadata)
}

fn series_sidecar_metadata_from_parsed(
    metadata: Option<LocalNfoMetadata>,
) -> Option<SeriesSidecarMetadata> {
    let metadata = metadata?;
    let title = metadata
        .title
        .clone()
        .filter(|value| !value.trim().is_empty());
    let year = metadata.year;

    Some(SeriesSidecarMetadata {
        title,
        year,
        local_nfo: metadata,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedEpisodeIdentity {
    season_number: i32,
    episode_number: i32,
    episode_title: Option<String>,
}

fn parse_episode_identity(path: &Path) -> Option<ParsedEpisodeIdentity> {
    let normalized = humanized_stem_for_identity(path);
    let tokens = normalized.split_whitespace().collect::<Vec<_>>();
    let (_, title_start, season_number, episode_number) =
        tokens.iter().enumerate().find_map(|(index, token)| {
            parse_episode_token_marker(token).map(|episode_token| {
                (
                    index,
                    index + 1,
                    episode_token.season_number,
                    episode_token.episode_number,
                )
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

/// Humanized file stem used for every identity decision.
///
/// A filename with only an episode marker (`EP01`, `E01`, `第01集`) is an
/// episode only when its directory proves a series; the marker is then
/// rewritten to the canonical `S01E01` form so title, year, episode-title and
/// season handling stay identical to files that carry a full marker.
fn humanized_stem_for_identity(path: &Path) -> String {
    let normalized = humanize_file_stem(path);
    let tokens = normalized
        .split_whitespace()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let Some((marker, season_number)) = proven_episode_only_marker(path, &tokens) else {
        return normalized;
    };

    let canonical = format!("S{season_number:02}E{:02}", marker.episode_number);
    let replacement = match marker.title_prefix {
        Some(prefix) => format!("{prefix}{canonical}"),
        None => canonical,
    };
    tokens[..marker.index]
        .iter()
        .cloned()
        .chain(std::iter::once(replacement))
        .chain(tokens[marker.index + marker.consumed..].iter().cloned())
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EpisodeOnlyMarker {
    index: usize,
    consumed: usize,
    title_prefix: Option<String>,
    episode_number: i32,
}

/// Directory-independent facts of one file that carries an episode-only marker.
#[derive(Debug, Clone, PartialEq, Eq)]
struct EpisodeOnlyCandidate {
    title_key: String,
    year: Option<i32>,
    episode_number: i32,
}

/// Finds `EP01`, `EP 01`, `Ep.01`, `E01`, `第01集` or `第 1 集`. Files that
/// already carry a full season/episode marker never reach this parser.
fn parse_episode_only_marker(tokens: &[String]) -> Option<EpisodeOnlyMarker> {
    for (index, token) in tokens.iter().enumerate() {
        let lower = token.to_ascii_lowercase();
        let marker = |consumed, title_prefix, episode_number| EpisodeOnlyMarker {
            index,
            consumed,
            title_prefix,
            episode_number,
        };

        if let Some(number) = lower.strip_prefix("ep").and_then(parse_episode_only_number) {
            return Some(marker(1, None, number));
        }
        if lower == "ep" {
            if let Some(number) = tokens
                .get(index + 1)
                .and_then(|next| parse_episode_only_number(next))
            {
                return Some(marker(2, None, number));
            }
        }
        if let Some(number) = lower.strip_prefix('e').and_then(parse_episode_only_number) {
            return Some(marker(1, None, number));
        }
        if token == "第" && tokens.get(index + 2).is_some_and(|last| last == "集") {
            if let Some(number) = tokens
                .get(index + 1)
                .and_then(|next| parse_episode_only_number(next))
            {
                return Some(marker(3, None, number));
            }
        }
        if let Some((title_prefix, number)) = parse_chinese_episode_only_token(token) {
            return Some(marker(1, title_prefix, number));
        }
    }

    None
}

fn parse_episode_only_number(value: &str) -> Option<i32> {
    (!value.is_empty() && value.len() <= 4 && value.chars().all(|ch| ch.is_ascii_digit()))
        .then(|| value.parse::<i32>().ok())
        .flatten()
}

/// `第01集` with an optional title glued in front, as in `山海情第01集`.
fn parse_chinese_episode_only_token(token: &str) -> Option<(Option<String>, i32)> {
    let body = token.strip_suffix('集')?;
    let marker_start = body.rfind('第')?;
    let number = parse_episode_only_number(&body[marker_start + '第'.len_utf8()..])?;
    let prefix = &body[..marker_start];
    let title_prefix = prefix
        .chars()
        .any(char::is_alphanumeric)
        .then(|| prefix.to_string());
    Some((title_prefix, number))
}

fn episode_only_candidate(tokens: &[String]) -> Option<(EpisodeOnlyMarker, EpisodeOnlyCandidate)> {
    if tokens
        .iter()
        .any(|token| parse_episode_token_marker(token).is_some())
    {
        return None;
    }
    let marker = parse_episode_only_marker(tokens)?;

    let mut prefix_tokens = tokens[..marker.index].to_vec();
    if let Some(prefix) = &marker.title_prefix {
        prefix_tokens.push(prefix.clone());
    }
    while prefix_tokens
        .last()
        .is_some_and(|token| is_separator_token(token))
    {
        prefix_tokens.pop();
    }
    let prefix_text = prefix_tokens.join(" ");
    let (title_key, prefix_year) = if prefix_text.trim().is_empty() {
        (String::new(), None)
    } else {
        let parsed = parse_title_year_from_humanized_name(&prefix_text);
        (parsed.title.to_lowercase(), parsed.year)
    };
    let year = prefix_year
        .or_else(|| parse_year_after_episode_token(tokens, marker.index + marker.consumed));
    let episode_number = marker.episode_number;

    Some((
        marker,
        EpisodeOnlyCandidate {
            title_key,
            year,
            episode_number,
        },
    ))
}

/// Returns the marker and season when the file's directory proves a series:
/// at least two distinct episode numbers share the file's title, their years
/// do not conflict, and the file has no movie NFO of its own. A lone `EP4`
/// movie or an `EP4`/`EP5`/`EP6` collection with different years stays a movie.
fn proven_episode_only_marker(path: &Path, tokens: &[String]) -> Option<(EpisodeOnlyMarker, i32)> {
    let (marker, candidate) = episode_only_candidate(tokens)?;
    let directory = path.parent()?;
    let siblings = directory_episode_only_candidates(directory);
    let group = siblings
        .iter()
        .filter(|sibling| sibling.title_key == candidate.title_key)
        .collect::<Vec<_>>();
    let episode_numbers = group
        .iter()
        .map(|sibling| sibling.episode_number)
        .collect::<BTreeSet<_>>();
    let years = group
        .iter()
        .filter_map(|sibling| sibling.year)
        .collect::<BTreeSet<_>>();
    if episode_numbers.len() < 2
        || years.len() > 1
        || !episode_numbers.contains(&candidate.episode_number)
        || has_own_movie_nfo(path)
    {
        return None;
    }

    let season_number = directory
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(season_number_from_folder_title)
        .unwrap_or(1);
    Some((marker, season_number))
}

fn has_own_movie_nfo(path: &Path) -> bool {
    is_movie_nfo_file(&path.with_extension("nfo"))
}

type EpisodeOnlyDirectoryCache = HashMap<PathBuf, (SystemTime, Arc<Vec<EpisodeOnlyCandidate>>)>;

const EPISODE_ONLY_DIRECTORY_CACHE_LIMIT: usize = 4096;

/// Episode-only candidates of one directory, cached per directory mtime.
///
/// Every identity call for a file with an episode-only marker needs its
/// siblings; the cache keeps that to one directory read per directory state.
/// Adding, removing or renaming a file changes the directory mtime and
/// invalidates the entry.
fn directory_episode_only_candidates(directory: &Path) -> Arc<Vec<EpisodeOnlyCandidate>> {
    static CACHE: OnceLock<Mutex<EpisodeOnlyDirectoryCache>> = OnceLock::new();

    let modified = fs::metadata(directory)
        .and_then(|metadata| metadata.modified())
        .ok();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(modified) = modified {
        if let Some((cached_modified, candidates)) = cache
            .lock()
            .ok()
            .and_then(|entries| entries.get(directory).cloned())
        {
            if cached_modified == modified {
                return candidates;
            }
        }
    }

    let candidates = Arc::new(read_directory_episode_only_candidates(directory));
    if let (Some(modified), Ok(mut entries)) = (modified, cache.lock()) {
        if entries.len() >= EPISODE_ONLY_DIRECTORY_CACHE_LIMIT {
            entries.clear();
        }
        entries.insert(directory.to_path_buf(), (modified, Arc::clone(&candidates)));
    }
    candidates
}

fn read_directory_episode_only_candidates(directory: &Path) -> Vec<EpisodeOnlyCandidate> {
    let Ok(entries) = fs::read_dir(directory) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| is_supported_video(path) && path.is_file())
        .filter_map(|path| {
            let tokens = humanize_file_stem(&path)
                .split_whitespace()
                .map(str::to_string)
                .collect::<Vec<_>>();
            episode_only_candidate(&tokens).map(|(_, candidate)| candidate)
        })
        .collect()
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

    let e_position = bytes
        .iter()
        .position(|byte| *byte == b'E' || *byte == b'e')?;

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
    let separator_index = lower.find('x')?;

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

fn relative_media_parent<'a>(path: &'a Path, root_path: &Path) -> Option<&'a Path> {
    let relative_path = path.strip_prefix(root_path).ok()?;
    if relative_path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return None;
    }
    let parent = relative_path.parent()?;

    (!parent.as_os_str().is_empty()).then_some(parent)
}

fn parse_container_identity(value: &str, container_path: &Path) -> Option<MediaContainerIdentity> {
    let (title_without_identity, tmdb_id) = strip_trailing_tmdb_identity(value);
    let display_title = humanize_directory_title(&title_without_identity)?;
    let parsed_name = parse_title_year_from_humanized_name(&display_title);

    if !parsed_name.has_meaningful_title
        || !is_valid_container_title(&parsed_name.title)
        || is_season_directory_name(&parsed_name.title)
        || is_technical_directory_name(&parsed_name.title)
    {
        return None;
    }

    Some(MediaContainerIdentity {
        container_path: container_path.to_path_buf(),
        display_title,
        title: parsed_name.title,
        year: parsed_name.year,
        tmdb_id,
    })
}

fn strip_trailing_tmdb_identity(value: &str) -> (String, Option<String>) {
    let trimmed = value.trim();
    let Some((opening, closing)) = trimmed
        .ends_with('}')
        .then_some(('{', '}'))
        .or_else(|| trimmed.ends_with(']').then_some(('[', ']')))
    else {
        return (trimmed.to_string(), None);
    };
    let Some(opening_index) = trimmed.rfind(opening) else {
        return (trimmed.to_string(), None);
    };
    let token = &trimmed[opening_index + opening.len_utf8()..trimmed.len() - closing.len_utf8()];
    let Some(tmdb_id) = parse_tmdb_identity_token(token) else {
        return (trimmed.to_string(), None);
    };
    let title = trimmed[..opening_index]
        .trim_end_matches(|ch: char| {
            ch.is_whitespace() || matches!(ch, '.' | '_' | '-' | '–' | '—')
        })
        .to_string();

    (title, Some(tmdb_id))
}

fn parse_tmdb_identity_token(value: &str) -> Option<String> {
    let normalized = value.trim().to_ascii_lowercase();
    let id = normalized
        .strip_prefix("tmdb-")
        .or_else(|| normalized.strip_prefix("tmdbid-"))?;

    let id = id.parse::<i64>().ok()?;
    (id > 0).then(|| id.to_string())
}

fn humanize_directory_title(value: &str) -> Option<String> {
    let normalized = value
        .chars()
        .map(|ch| {
            if is_filename_word_separator_char(ch) {
                ' '
            } else {
                ch
            }
        })
        .collect::<String>();
    let title = decode_basic_html_entities(&normalized)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    (!title.is_empty()).then_some(title)
}

fn is_valid_container_title(value: &str) -> bool {
    let trimmed = value.trim();
    let normalized = trimmed.to_ascii_lowercase();

    !trimmed.is_empty()
        && trimmed.chars().any(char::is_alphanumeric)
        && !trimmed
            .chars()
            .filter(|ch| !ch.is_whitespace())
            .all(|ch| ch.is_ascii_digit())
        && !is_generic_library_folder(trimmed)
        && !is_collection_folder_title(trimmed)
        && !matches!(
            normalized.as_str(),
            "mainland"
                | "new folder"
                | "untitled folder"
                | "uncategorized"
                | "temporary"
                | "temp"
                | "tmp"
        )
        && !matches!(
            trimmed,
            "国产剧" | "海外剧" | "新建文件夹" | "未分类" | "临时文件" | "临时目录"
        )
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

fn is_season_folder_title(value: &str) -> bool {
    season_number_from_folder_title(value).is_some()
}

/// Season number of a strict season folder name: `Season 01`, `S01`, `第1季`.
fn season_number_from_folder_title(value: &str) -> Option<i32> {
    let normalized = value
        .chars()
        .map(|ch| {
            if is_filename_word_separator_char(ch) {
                ' '
            } else {
                ch
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    let compact = normalized.replace(' ', "");

    normalized
        .strip_prefix("season ")
        .and_then(parse_short_number_token)
        .or_else(|| compact.strip_prefix('s').and_then(parse_short_number_token))
        .or_else(|| {
            compact
                .strip_prefix('第')
                .and_then(|value| value.strip_suffix('季'))
                .and_then(parse_short_number_token)
        })
}

fn is_season_directory_name(value: &str) -> bool {
    let normalized = value
        .trim()
        .replace(['.', '_', '-', '—', '–'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let normalized_lower = normalized.to_ascii_lowercase();
    let tokens = normalized_lower.split_whitespace().collect::<Vec<_>>();

    if tokens
        .windows(2)
        .any(|pair| pair[0] == "season" && parse_short_number_token(pair[1]).is_some())
    {
        return true;
    }

    if normalized.contains('季') && normalized.chars().any(|ch| ch.is_ascii_digit()) {
        return true;
    }

    tokens.iter().any(|token| {
        token.strip_prefix('s').is_some_and(|suffix| {
            !suffix.is_empty() && suffix.len() <= 3 && suffix.chars().all(|ch| ch.is_ascii_digit())
        })
    })
}

fn is_technical_directory_name(value: &str) -> bool {
    let normalized = value
        .trim()
        .replace(['.', '_', '-', '—', '–'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    let tokens = normalized.split_whitespace().collect::<Vec<_>>();

    !tokens.is_empty()
        && tokens
            .iter()
            .all(|token| is_technical_directory_token(token))
}

fn is_technical_directory_token(token: &str) -> bool {
    matches!(
        token,
        "4k" | "8k"
            | "2160p"
            | "1440p"
            | "1080p"
            | "720p"
            | "480p"
            | "uhd"
            | "web"
            | "dl"
            | "rip"
            | "webrip"
            | "webdl"
            | "blu"
            | "ray"
            | "bluray"
            | "bdrip"
            | "remux"
            | "dv"
            | "dovi"
            | "dolby"
            | "vision"
            | "hdr"
            | "hdr10"
            | "hdr10+"
            | "sdr"
            | "avc"
            | "av1"
            | "hevc"
            | "x264"
            | "x265"
            | "h264"
            | "h265"
            | "10bit"
            | "8bit"
    )
}

fn has_leading_sequence_index(path: &Path) -> bool {
    let is_explicit_sequence_path = path
        .parent()
        .and_then(Path::file_name)
        .and_then(|value| value.to_str())
        .is_some_and(|value| is_collection_folder_title(value) || is_season_folder_title(value));
    if !is_explicit_sequence_path {
        return false;
    }

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

fn strip_leading_sequence_index(title: &str) -> String {
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
        has_meaningful_title: title_start < title_end,
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
    let prefix = trim_wrapping_punctuation(&prefix)
        .trim_matches(is_separator_token_char)
        .trim()
        .to_string();

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
    if token == "iT" {
        return true;
    }
    let token = token.to_ascii_lowercase();

    matches!(
        token.as_str(),
        "8k" | "4k"
            | "2160p"
            | "1440p"
            | "1080p"
            | "720p"
            | "480p"
            | "web"
            | "webdl"
            | "web-dl"
            | "webrip"
            | "x264"
            | "x265"
            | "h264"
            | "h265"
            | "hevc"
            | "avc"
            | "av1"
            | "bluray"
            | "bdrip"
            | "hdrip"
            | "dvdrip"
            | "remux"
            | "hdr"
            | "hdr10"
            | "hdr10+"
            | "dv"
            | "dovi"
            | "sdr"
            | "nf"
            | "amzn"
            | "dsnp"
            | "hmax"
            | "atvp"
            | "pcok"
            | "aac"
            | "dts"
            | "atmos"
            | "truehd"
            | "eac3"
            | "ac3"
            | "10bit"
            | "8bit"
    ) || token
        .strip_prefix("ddp")
        .is_some_and(|suffix| suffix.is_empty() || suffix.chars().all(|ch| ch.is_ascii_digit()))
        || token.strip_suffix("audio").is_some_and(|prefix| {
            !prefix.is_empty() && prefix.chars().all(|ch| ch.is_ascii_digit())
        })
}
