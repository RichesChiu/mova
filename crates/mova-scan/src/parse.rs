use super::sidecar::{find_local_artwork, read_sidecar_metadata, ArtworkKind};
use std::path::{Path, PathBuf};

/// 根据文件名和目录结构判断某个视频路径是否更像剧集文件。
pub fn is_likely_episode_path(path: &Path) -> bool {
    if parse_episode_identity(path).is_some() {
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
pub struct SeriesFolderMetadata {
    pub folder_path: PathBuf,
    pub display_title: String,
    pub title: String,
    pub year: Option<i32>,
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
    let has_leading_collection_index = has_leading_collection_index(path);
    let has_collection_folder = path_has_collection_folder(path);
    let tokens = normalized.split_whitespace().collect::<Vec<_>>();
    let inferred_series_folder_metadata = infer_series_folder_metadata(path);
    let inferred_movie_folder_metadata = infer_movie_folder_metadata(path);
    let inferred_series_title = inferred_series_folder_metadata
        .as_ref()
        .map(|metadata| metadata.title.clone());
    let inferred_series_year = inferred_series_folder_metadata
        .as_ref()
        .and_then(|metadata| metadata.year);
    let parsed_name = parse_title_year_from_humanized_name(&normalized);
    let mut title = parsed_name.title.clone();
    let mut year = parsed_name.year.or(inferred_series_year);
    let fallback_title = inferred_series_title
        .clone()
        .filter(|_| starts_with_episode_only_marker(&tokens))
        .unwrap_or_else(|| normalized.clone());
    let should_prefer_inferred_series_title =
        starts_with_episode_only_marker(&tokens) && inferred_series_title.is_some();
    let should_prefer_movie_folder = !should_prefer_inferred_series_title
        && inferred_movie_folder_metadata
            .as_ref()
            .is_some_and(|metadata| {
                should_prefer_movie_folder_metadata(&normalized, &parsed_name, metadata)
            });

    if should_prefer_movie_folder {
        if let Some(folder_metadata) = inferred_movie_folder_metadata {
            title = folder_metadata.title;
            year = year.or(folder_metadata.year);
        }
    } else if has_leading_collection_index && has_collection_folder {
        title = strip_leading_collection_index(&title);
    }

    ParsedNameMetadata {
        title: if should_prefer_inferred_series_title {
            inferred_series_title.unwrap_or(fallback_title)
        } else if title.is_empty() {
            fallback_title
        } else {
            title
        },
        year,
    }
}

pub fn infer_series_folder_metadata(path: &Path) -> Option<SeriesFolderMetadata> {
    let (folder_path, component_name) = find_series_group_component(path)?;
    let display_title = humanize_component_name(&component_name);
    let parsed_name = parse_title_year_from_humanized_name(&display_title);

    Some(SeriesFolderMetadata {
        folder_path,
        display_title,
        title: parsed_name.title,
        year: parsed_name.year,
    })
}

fn infer_movie_folder_metadata(path: &Path) -> Option<ParsedNameMetadata> {
    path.ancestors()
        .skip(1)
        .take(3)
        .filter_map(|ancestor| ancestor.file_name().and_then(|value| value.to_str()))
        .filter(|component| parse_season_component(component).is_none())
        .filter_map(|component| {
            let humanized = humanize_component_name(component);
            let parsed = parse_title_year_from_humanized_name(&humanized);
            let score = score_movie_folder_candidate(&humanized, &parsed);

            (score > 0).then_some((parsed, score))
        })
        .max_by_key(|(_, score)| *score)
        .map(|(parsed, _)| parsed)
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
    let inferred_season_number = infer_season_number_from_path(path);
    let (_, title_start, season_number, episode_number) =
        if let Some((index, season_number, episode_number)) =
            tokens.iter().enumerate().find_map(|(index, token)| {
                parse_episode_token(token).map(|(season, episode)| (index, season, episode))
            })
        {
            (index, index + 1, season_number, episode_number)
        } else {
            let season_number = inferred_season_number?;
            let (index, title_start, episode_number) = parse_episode_number_only_tokens(&tokens)?;
            (index, title_start, season_number, episode_number)
        };

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
        .filter(|value| !value.is_empty());

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

fn parse_episode_number_only_tokens(tokens: &[&str]) -> Option<(usize, usize, i32)> {
    for (index, token) in tokens.iter().enumerate() {
        if let Some(episode_number) = parse_episode_number_token(token) {
            return Some((index, index + 1, episode_number));
        }

        if is_episode_label_token(token) {
            let next_token = tokens.get(index + 1)?;
            let episode_number = parse_short_number_token(next_token)?;
            return Some((index, index + 2, episode_number));
        }
    }

    let first_content_index = tokens.iter().position(|token| !is_separator_token(token))?;
    let episode_number = parse_short_number_token(tokens[first_content_index])?;
    Some((first_content_index, first_content_index + 1, episode_number))
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

fn parse_episode_number_token(token: &str) -> Option<i32> {
    let trimmed = trim_wrapping_punctuation(token);

    if let Some(value) = parse_chinese_episode_token(trimmed) {
        return Some(value);
    }

    let normalized = trimmed
        .chars()
        .filter(|ch| !matches!(ch, '.' | '_' | '-'))
        .collect::<String>()
        .to_ascii_lowercase();

    ["episode", "ep", "e"].iter().find_map(|prefix| {
        normalized
            .strip_prefix(prefix)
            .and_then(parse_short_number_token)
    })
}

fn parse_chinese_episode_token(token: &str) -> Option<i32> {
    let trimmed = trim_wrapping_punctuation(token);

    for suffix in ['集', '话', '話'] {
        let body = trimmed
            .strip_prefix('第')
            .and_then(|value| value.strip_suffix(suffix))?;
        if let Some(number) = parse_short_number_token(body) {
            return Some(number);
        }
    }

    None
}

fn is_episode_label_token(token: &str) -> bool {
    matches!(
        trim_wrapping_punctuation(token)
            .to_ascii_lowercase()
            .as_str(),
        "episode" | "ep" | "e"
    )
}

fn starts_with_episode_only_marker(tokens: &[&str]) -> bool {
    let Some(first_content_index) = tokens.iter().position(|token| !is_separator_token(token))
    else {
        return false;
    };

    let first_token = tokens[first_content_index];
    if parse_episode_number_token(first_token).is_some() || is_episode_label_token(first_token) {
        return true;
    }

    if parse_short_number_token(first_token).is_none() {
        return false;
    }

    !tokens
        .iter()
        .skip(first_content_index + 1)
        .filter(|token| !is_separator_token(token))
        .any(|token| parse_year_token(token).is_some())
}

fn infer_season_number_from_path(path: &Path) -> Option<i32> {
    path.ancestors()
        .skip(1)
        .filter_map(|ancestor| ancestor.file_name().and_then(|value| value.to_str()))
        .find_map(parse_season_component)
}

fn humanize_component_name(component: &str) -> String {
    let normalized = component
        .chars()
        .map(|ch| match ch {
            '.' | '_' | '-' => ' ',
            other => other,
        })
        .collect::<String>();

    decode_basic_html_entities(&normalized)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn decode_basic_html_entities(value: &str) -> String {
    value
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&quot;", "\"")
        .replace("&amp;", "&")
}

fn should_prefer_movie_folder_metadata(
    normalized_file_name: &str,
    parsed_name: &ParsedNameMetadata,
    folder_metadata: &ParsedNameMetadata,
) -> bool {
    let file_title = parsed_name.title.trim();
    let folder_title = folder_metadata.title.trim();

    if folder_title.is_empty()
        || is_generic_library_folder(folder_title)
        || is_collection_folder_title(folder_title)
    {
        return false;
    }

    contains_encoded_entity(normalized_file_name)
        || (contains_cjk(folder_title) && !contains_cjk(file_title))
        || count_release_tokens(normalized_file_name) >= 2
        || (folder_metadata.year.is_some() && parsed_name.year.is_none())
}

fn score_movie_folder_candidate(component: &str, parsed_name: &ParsedNameMetadata) -> i32 {
    let title = parsed_name.title.trim();
    if title.is_empty() || is_generic_library_folder(title) || is_collection_folder_title(title) {
        return 0;
    }

    let token_count = title.split_whitespace().count();
    let release_token_count = count_release_tokens(component);
    let mut score = 0;

    if parsed_name.year.is_some() {
        score += 5;
    }

    if contains_cjk(title) {
        score += 4;
    }

    if (1..=6).contains(&token_count) {
        score += 2;
    }

    if release_token_count == 0 {
        score += 2;
    } else {
        score -= i32::try_from(release_token_count.min(3)).unwrap_or(3);
    }

    if title.len() <= 32 {
        score += 1;
    }

    score
}

fn contains_encoded_entity(value: &str) -> bool {
    value.contains("&#") || value.contains("&apos;") || value.contains("&quot;")
}

fn contains_cjk(value: &str) -> bool {
    value.chars().any(|ch| {
        ('\u{4E00}'..='\u{9FFF}').contains(&ch)
            || ('\u{3400}'..='\u{4DBF}').contains(&ch)
            || ('\u{3040}'..='\u{30FF}').contains(&ch)
            || ('\u{AC00}'..='\u{D7AF}').contains(&ch)
    })
}

fn count_release_tokens(value: &str) -> usize {
    value
        .split_whitespace()
        .filter(|token| is_release_token(trim_wrapping_punctuation(token)))
        .count()
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

fn path_has_collection_folder(path: &Path) -> bool {
    path.ancestors()
        .skip(1)
        .take(3)
        .filter_map(|ancestor| ancestor.file_name().and_then(|value| value.to_str()))
        .any(is_collection_folder_title)
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

        if is_episode_token(tokens[index].as_str()) || is_release_token(tokens[index].as_str()) {
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

fn find_series_group_component(path: &Path) -> Option<(PathBuf, String)> {
    let components = path
        .ancestors()
        .skip(1)
        .filter_map(|ancestor| {
            ancestor
                .file_name()
                .and_then(|value| value.to_str())
                .map(|name| (ancestor.to_path_buf(), name.to_string()))
        })
        .collect::<Vec<_>>();

    for (index, (_, component)) in components.iter().enumerate() {
        if parse_season_component(component).is_some() {
            return components.get(index + 1).cloned();
        }
    }

    components.first().cloned()
}

fn is_likely_season_component(component: &str) -> bool {
    parse_season_component(component).is_some()
}

fn parse_season_component(component: &str) -> Option<i32> {
    let normalized = component
        .chars()
        .map(|ch| match ch {
            '.' | '_' | '-' => ' ',
            other => other,
        })
        .collect::<String>()
        .to_ascii_lowercase();
    let tokens = normalized.split_whitespace().collect::<Vec<_>>();

    if let ["season", number] = tokens.as_slice() {
        return parse_short_number_token(number);
    }

    if let [token] = tokens.as_slice() {
        return token.strip_prefix('s').and_then(parse_short_number_token);
    }

    None
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
