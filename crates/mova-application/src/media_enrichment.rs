use crate::metadata::{
    apply_remote_metadata, MetadataLookup, MetadataLookupCache, MetadataProvider, RemoteMetadata,
    RemoteSeriesEpisodeOutline,
};
use mova_scan::DiscoveredMediaFile;
use reqwest::{header::CONTENT_TYPE, Client, Url};
use std::{
    collections::{hash_map::DefaultHasher, HashMap},
    hash::{Hash, Hasher},
    io::ErrorKind,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FrameCaptureAvailability {
    Unknown,
    Available,
    Unavailable,
}

/// 复用扫描和手动刷新共用的 metadata 补全与图片缓存逻辑。
pub struct MetadataEnrichmentContext {
    artwork_cache_dir: PathBuf,
    metadata_provider: Arc<dyn MetadataProvider>,
    metadata_language: String,
    metadata_cache: MetadataLookupCache,
    series_outline_cache: HashMap<MetadataLookup, Option<RemoteSeriesEpisodeOutline>>,
    artwork_cache: HashMap<String, Option<String>>,
    artwork_client: Client,
    frame_capture_availability: FrameCaptureAvailability,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MetadataEnrichmentStage {
    Metadata,
    Artwork,
    Completed,
}

impl MetadataEnrichmentContext {
    /// 扫描和手动刷新都会复用这个上下文。
    /// 语言在创建时就绑定下来，确保同一个库的所有 TMDB 请求都落在同一语言版本上。
    pub fn new(
        artwork_cache_dir: PathBuf,
        metadata_provider: Arc<dyn MetadataProvider>,
        metadata_language: String,
    ) -> Self {
        Self {
            artwork_cache_dir,
            metadata_provider,
            metadata_language,
            metadata_cache: HashMap::new(),
            series_outline_cache: HashMap::new(),
            artwork_cache: HashMap::new(),
            artwork_client: Client::new(),
            frame_capture_availability: FrameCaptureAvailability::Unknown,
        }
    }

    pub async fn enrich_file(&mut self, lookup_type: &str, file: &mut DiscoveredMediaFile) {
        self.enrich_file_with_progress(lookup_type, file, |_, _| {})
            .await;
    }

    pub async fn enrich_file_with_progress<F>(
        &mut self,
        lookup_type: &str,
        file: &mut DiscoveredMediaFile,
        mut on_progress: F,
    ) where
        F: FnMut(MetadataEnrichmentStage, &DiscoveredMediaFile),
    {
        let lookups = metadata_lookup_candidates(lookup_type, file, &self.metadata_language);
        let primary_lookup = lookups.first().cloned().unwrap_or_else(|| MetadataLookup {
            title: file.source_title.clone(),
            year: file.year,
            library_type: lookup_type.to_string(),
            language: Some(self.metadata_language.clone()),
            provider_item_id: None,
        });

        on_progress(MetadataEnrichmentStage::Metadata, file);
        let mut resolved_remote_metadata = None;
        let mut episode_outline_lookup = primary_lookup.clone();

        if self.metadata_provider.is_enabled() && needs_remote_metadata(file) {
            let mut metadata = None;

            for lookup in &lookups {
                let candidate = self.lookup_remote_metadata_cached(lookup).await;
                if candidate.is_some() {
                    episode_outline_lookup = lookup.clone();
                    metadata = candidate;
                    break;
                }
            }

            if let Some(remote_metadata) = metadata.as_ref() {
                episode_outline_lookup.provider_item_id = remote_metadata.provider_item_id;
            }

            apply_remote_metadata(
                metadata.clone(),
                &mut file.metadata_provider,
                &mut file.metadata_provider_item_id,
                &mut file.title,
                &mut file.original_title,
                &mut file.year,
                &mut file.imdb_rating,
                &mut file.country,
                &mut file.genres,
                &mut file.studio,
                &mut file.overview,
                &mut file.poster_path,
                &mut file.backdrop_path,
            );
            resolved_remote_metadata = metadata;
        }

        on_progress(MetadataEnrichmentStage::Artwork, file);

        if lookup_type.eq_ignore_ascii_case("series") {
            self.enrich_episode_like_artwork(
                &episode_outline_lookup,
                file,
                resolved_remote_metadata.is_some(),
            )
            .await;
        }

        self.cache_file_artwork(file).await;

        on_progress(MetadataEnrichmentStage::Completed, file);
    }

    async fn lookup_remote_metadata_cached(
        &mut self,
        lookup: &MetadataLookup,
    ) -> Option<RemoteMetadata> {
        if let Some(metadata) = self.metadata_cache.get(lookup) {
            return metadata.clone();
        }

        let metadata = match self.metadata_provider.lookup(lookup).await {
            Ok(metadata) => metadata,
            Err(error) => {
                tracing::warn!(
                    title = %lookup.title,
                    year = lookup.year,
                    library_type = %lookup.library_type,
                    error = ?error,
                    "metadata enrichment stage failed to fetch remote metadata, falling back to local data"
                );
                None
            }
        };

        self.metadata_cache.insert(lookup.clone(), metadata.clone());
        metadata
    }

    async fn lookup_series_outline_cached(
        &mut self,
        lookup: &MetadataLookup,
    ) -> Option<RemoteSeriesEpisodeOutline> {
        if let Some(outline) = self.series_outline_cache.get(lookup) {
            return outline.clone();
        }

        let outline = match self
            .metadata_provider
            .lookup_series_episode_outline(lookup)
            .await
        {
            Ok(outline) => outline,
            Err(error) => {
                tracing::warn!(
                    title = %lookup.title,
                    year = lookup.year,
                    library_type = %lookup.library_type,
                    error = ?error,
                    "metadata enrichment stage failed to fetch remote episode outline metadata, falling back to local data"
                );
                None
            }
        };

        self.series_outline_cache
            .insert(lookup.clone(), outline.clone());
        outline
    }

    async fn enrich_episode_like_artwork(
        &mut self,
        lookup: &MetadataLookup,
        file: &mut DiscoveredMediaFile,
        allow_remote_outline: bool,
    ) {
        let Some(season_number) = file.season_number else {
            return;
        };
        let Some(episode_number) = file.episode_number else {
            return;
        };

        if file.series_poster_path.is_none() {
            file.series_poster_path = file.poster_path.clone();
        }
        if file.series_backdrop_path.is_none() {
            file.series_backdrop_path = file.backdrop_path.clone();
        }

        if allow_remote_outline && self.metadata_provider.is_enabled() {
            if let Some(outline) = self.lookup_series_outline_cached(lookup).await {
                if let Some(remote_season) = outline
                    .seasons
                    .iter()
                    .find(|season| season.season_number == season_number)
                {
                    if file.season_title.is_none() {
                        file.season_title = remote_season.title.clone();
                    }
                    if file.season_overview.is_none() {
                        file.season_overview = remote_season.overview.clone();
                    }
                    if file.season_poster_path.is_none() {
                        file.season_poster_path = remote_season.poster_path.clone();
                    }
                    if file.season_backdrop_path.is_none() {
                        file.season_backdrop_path = remote_season.backdrop_path.clone();
                    }

                    if let Some(remote_episode) = remote_season
                        .episodes
                        .iter()
                        .find(|episode| episode.episode_number == episode_number)
                    {
                        if file.episode_title.is_none() {
                            file.episode_title = remote_episode.title.clone();
                        }
                        if file.overview.is_none() {
                            file.overview = remote_episode.overview.clone();
                        }
                        if remote_episode.poster_path.is_some()
                            && should_replace_episode_artwork(
                                file.poster_path.as_deref(),
                                is_generic_poster_artwork_path,
                            )
                        {
                            file.poster_path = remote_episode.poster_path.clone();
                        }
                        if remote_episode.backdrop_path.is_some()
                            && should_replace_episode_artwork(
                                file.backdrop_path.as_deref(),
                                is_generic_backdrop_artwork_path,
                            )
                        {
                            file.backdrop_path = remote_episode.backdrop_path.clone();
                        }
                    }
                }
            }
        }

        if file.poster_path.is_none() || file.backdrop_path.is_none() {
            if let Some(local_still_path) = self.capture_first_frame_for_file(file).await {
                if file.poster_path.is_none() {
                    file.poster_path = Some(local_still_path.clone());
                }
                if file.backdrop_path.is_none() {
                    file.backdrop_path = Some(local_still_path.clone());
                }
            }
        }

        if file.season_poster_path.is_none() {
            file.season_poster_path = file.poster_path.clone();
        }
        if file.season_backdrop_path.is_none() {
            file.season_backdrop_path = file.backdrop_path.clone();
        }
    }

    async fn capture_first_frame_for_file(&mut self, file: &DiscoveredMediaFile) -> Option<String> {
        if matches!(
            self.frame_capture_availability,
            FrameCaptureAvailability::Unavailable
        ) {
            return None;
        }

        let input_path = file.file_path.clone();
        let output_path = build_generated_episode_still_cache_path(&self.artwork_cache_dir, file);
        if tokio::fs::metadata(&output_path)
            .await
            .map(|metadata| metadata.is_file())
            .unwrap_or(false)
        {
            return Some(output_path.to_string_lossy().to_string());
        }

        if let Some(parent) = output_path.parent() {
            if let Err(error) = tokio::fs::create_dir_all(parent).await {
                tracing::warn!(
                    file_path = %input_path.display(),
                    cache_path = %output_path.display(),
                    error = ?error,
                    "failed to create generated artwork directory"
                );
                return None;
            }
        }

        let join_input = input_path.clone();
        let join_output = output_path.clone();
        let result = tokio::task::spawn_blocking(move || {
            run_ffmpeg_frame_capture(&join_input, &join_output)
        })
        .await;

        match result {
            Ok(Ok(())) => {
                self.frame_capture_availability = FrameCaptureAvailability::Available;
                Some(output_path.to_string_lossy().to_string())
            }
            Ok(Err(error)) if error.kind() == ErrorKind::NotFound => {
                self.frame_capture_availability = FrameCaptureAvailability::Unavailable;
                tracing::warn!(
                    error = %error,
                    "ffmpeg is not available; first-frame artwork fallback disabled"
                );
                None
            }
            Ok(Err(error)) => {
                self.frame_capture_availability = FrameCaptureAvailability::Available;
                tracing::warn!(
                    file_path = %input_path.display(),
                    cache_path = %output_path.display(),
                    error = %error,
                    "failed to capture first frame for episode artwork"
                );
                None
            }
            Err(error) => {
                tracing::warn!(
                    file_path = %input_path.display(),
                    cache_path = %output_path.display(),
                    error = ?error,
                    "first-frame artwork worker failed to join"
                );
                None
            }
        }
    }
    async fn cache_file_artwork(&mut self, file: &mut DiscoveredMediaFile) {
        if let Some(series_poster_path) = file.series_poster_path.clone() {
            if let Some(cached_path) = self
                .cache_remote_artwork(&series_poster_path, "poster")
                .await
            {
                file.series_poster_path = Some(cached_path);
            }
        }

        if let Some(series_backdrop_path) = file.series_backdrop_path.clone() {
            if let Some(cached_path) = self
                .cache_remote_artwork(&series_backdrop_path, "backdrop")
                .await
            {
                file.series_backdrop_path = Some(cached_path);
            }
        }

        if let Some(season_poster_path) = file.season_poster_path.clone() {
            if let Some(cached_path) = self
                .cache_remote_artwork(&season_poster_path, "poster")
                .await
            {
                file.season_poster_path = Some(cached_path);
            }
        }

        if let Some(season_backdrop_path) = file.season_backdrop_path.clone() {
            if let Some(cached_path) = self
                .cache_remote_artwork(&season_backdrop_path, "backdrop")
                .await
            {
                file.season_backdrop_path = Some(cached_path);
            }
        }

        if let Some(poster_path) = file.poster_path.clone() {
            if let Some(cached_path) = self.cache_remote_artwork(&poster_path, "poster").await {
                file.poster_path = Some(cached_path);
            }
        }

        if let Some(backdrop_path) = file.backdrop_path.clone() {
            if let Some(cached_path) = self.cache_remote_artwork(&backdrop_path, "backdrop").await {
                file.backdrop_path = Some(cached_path);
            }
        }
    }

    async fn cache_remote_artwork(&mut self, source_url: &str, kind: &str) -> Option<String> {
        if !is_external_url(source_url) {
            return None;
        }

        if let Some(cached_path) = self.artwork_cache.get(source_url) {
            return cached_path.clone();
        }

        let cache_path = build_artwork_cache_path(&self.artwork_cache_dir, source_url, kind);

        if tokio::fs::metadata(&cache_path)
            .await
            .map(|metadata| metadata.is_file())
            .unwrap_or(false)
        {
            let cached = Some(cache_path.to_string_lossy().to_string());
            self.artwork_cache
                .insert(source_url.to_string(), cached.clone());
            return cached;
        }

        if let Some(parent) = cache_path.parent() {
            if let Err(error) = tokio::fs::create_dir_all(parent).await {
                tracing::warn!(
                    kind,
                    source_url,
                    error = ?error,
                    "failed to create artwork cache directory"
                );
                self.artwork_cache.insert(source_url.to_string(), None);
                return None;
            }
        }

        let response = match self.artwork_client.get(source_url).send().await {
            Ok(response) => response,
            Err(error) => {
                tracing::warn!(
                    kind,
                    source_url,
                    error = ?error,
                    "failed to download artwork"
                );
                self.artwork_cache.insert(source_url.to_string(), None);
                return None;
            }
        };

        let response = match response.error_for_status() {
            Ok(response) => response,
            Err(error) => {
                tracing::warn!(
                    kind,
                    source_url,
                    error = ?error,
                    "artwork request returned non-success status"
                );
                self.artwork_cache.insert(source_url.to_string(), None);
                return None;
            }
        };

        if let Some(content_type) = response.headers().get(CONTENT_TYPE) {
            tracing::debug!(kind, source_url, content_type = ?content_type, "downloading artwork");
        }

        let bytes = match response.bytes().await {
            Ok(bytes) => bytes,
            Err(error) => {
                tracing::warn!(
                    kind,
                    source_url,
                    error = ?error,
                    "failed to read artwork response body"
                );
                self.artwork_cache.insert(source_url.to_string(), None);
                return None;
            }
        };

        if let Err(error) = tokio::fs::write(&cache_path, &bytes).await {
            tracing::warn!(
                kind,
                source_url,
                cache_path = %cache_path.display(),
                error = ?error,
                "failed to write artwork cache file"
            );
            self.artwork_cache.insert(source_url.to_string(), None);
            return None;
        }

        let cached = Some(cache_path.to_string_lossy().to_string());
        self.artwork_cache
            .insert(source_url.to_string(), cached.clone());
        cached
    }
}

fn build_generated_episode_still_cache_path(
    artwork_cache_dir: &Path,
    file: &DiscoveredMediaFile,
) -> PathBuf {
    let mut hasher = DefaultHasher::new();
    file.file_path.to_string_lossy().hash(&mut hasher);
    file.file_size.hash(&mut hasher);
    let cache_key = format!("{:016x}", hasher.finish());

    artwork_cache_dir
        .join("generated")
        .join("episode-stills")
        .join(format!("{}.jpg", cache_key))
}

fn run_ffmpeg_frame_capture(input: &Path, output: &Path) -> std::io::Result<()> {
    let status = Command::new("ffmpeg")
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-y")
        .arg("-ss")
        .arg("00:00:01")
        .arg("-i")
        .arg(input)
        .arg("-frames:v")
        .arg("1")
        .arg("-q:v")
        .arg("2")
        .arg(output)
        .status()?;

    if status.success() {
        return Ok(());
    }

    Err(std::io::Error::other(format!(
        "ffmpeg exited with status {}",
        status
    )))
}

fn needs_remote_metadata(file: &DiscoveredMediaFile) -> bool {
    file.original_title.is_none()
        || file.overview.is_none()
        || file.poster_path.is_none()
        || file.backdrop_path.is_none()
        || file.year.is_none()
}

fn build_artwork_cache_path(artwork_cache_dir: &Path, source_url: &str, kind: &str) -> PathBuf {
    let extension = artwork_file_extension(source_url);
    let cache_key = stable_artwork_cache_key(source_url);

    artwork_cache_dir
        .join("tmdb")
        .join(kind)
        .join(format!("{}.{}", cache_key, extension))
}

fn stable_artwork_cache_key(source_url: &str) -> String {
    let mut hasher = DefaultHasher::new();
    source_url.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn artwork_file_extension(source_url: &str) -> &'static str {
    let extension = Url::parse(source_url).ok().and_then(|url| {
        Path::new(url.path())
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.to_ascii_lowercase())
    });

    match extension.as_deref() {
        Some("jpg") | Some("jpeg") => "jpg",
        Some("png") => "png",
        Some("webp") => "webp",
        Some("gif") => "gif",
        Some("avif") => "avif",
        _ => "jpg",
    }
}

fn should_replace_episode_artwork(
    current_path: Option<&str>,
    is_generic_path: fn(&str) -> bool,
) -> bool {
    match current_path {
        None => true,
        Some(path) => {
            is_external_url(path) || is_generated_episode_still_path(path) || is_generic_path(path)
        }
    }
}

fn is_generated_episode_still_path(value: &str) -> bool {
    value.contains("/generated/episode-stills/")
}

fn is_generic_poster_artwork_path(value: &str) -> bool {
    is_generic_artwork_path(value, &["poster", "folder", "cover"])
}

fn is_generic_backdrop_artwork_path(value: &str) -> bool {
    is_generic_artwork_path(value, &["fanart", "backdrop", "background"])
}

fn is_generic_artwork_path(value: &str, generic_stems: &[&str]) -> bool {
    if is_external_url(value) {
        return false;
    }

    let Some(stem) = Path::new(value)
        .file_stem()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
    else {
        return false;
    };

    generic_stems.iter().any(|candidate| stem == *candidate)
}

fn is_external_url(value: &str) -> bool {
    value.starts_with("http://") || value.starts_with("https://")
}

fn metadata_lookup_candidates(
    lookup_type: &str,
    file: &DiscoveredMediaFile,
    metadata_language: &str,
) -> Vec<MetadataLookup> {
    let series_container_metadata = lookup_type
        .eq_ignore_ascii_case("series")
        .then(|| series_container_metadata_for_episode_path(file))
        .flatten();
    let primary_year = file.year.or(series_container_metadata
        .as_ref()
        .and_then(|item| item.year));

    // 元数据匹配应优先使用文件名解析出的原始标题，而不是已经被远端覆盖过的展示标题。
    let mut candidates = Vec::new();
    push_metadata_lookup_candidate(
        &mut candidates,
        lookup_type,
        metadata_language,
        file.source_title.clone(),
        primary_year,
    );

    if let Some(container_metadata) = series_container_metadata {
        if !same_lookup_title(&file.source_title, &container_metadata.title) {
            push_metadata_lookup_candidate(
                &mut candidates,
                lookup_type,
                metadata_language,
                container_metadata.title,
                container_metadata.year.or(file.year),
            );
        }
    }

    prioritize_localized_lookup_candidates(&mut candidates, metadata_language);
    candidates
}

fn prioritize_localized_lookup_candidates(
    candidates: &mut [MetadataLookup],
    metadata_language: &str,
) {
    if !metadata_language
        .trim()
        .to_ascii_lowercase()
        .starts_with("zh")
    {
        return;
    }

    candidates.sort_by_key(|candidate| {
        if contains_cjk_character(&candidate.title) {
            0
        } else {
            1
        }
    });
}

fn push_metadata_lookup_candidate(
    candidates: &mut Vec<MetadataLookup>,
    lookup_type: &str,
    metadata_language: &str,
    title: String,
    year: Option<i32>,
) {
    let title = title.trim();
    if title.is_empty() {
        return;
    }

    if candidates
        .iter()
        .any(|candidate| same_lookup_title(&candidate.title, title) && candidate.year == year)
    {
        return;
    }

    candidates.push(MetadataLookup {
        title: title.to_string(),
        year,
        library_type: lookup_type.to_string(),
        language: Some(metadata_language.to_string()),
        provider_item_id: None,
    });
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SeriesContainerMetadata {
    title: String,
    year: Option<i32>,
}

fn series_container_metadata_for_episode_path(
    file: &DiscoveredMediaFile,
) -> Option<SeriesContainerMetadata> {
    if file.season_number.is_none() || file.episode_number.is_none() {
        return None;
    }

    let parent = file.file_path.parent()?;
    let mut directories = parent
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .filter(|component| !component.trim().is_empty())
        .collect::<Vec<_>>();

    while directories
        .last()
        .is_some_and(|directory| is_series_variant_directory_name(directory))
    {
        directories.pop();
    }

    let season_directory_index = directories
        .iter()
        .rposition(|directory| is_season_directory_name(directory))?;
    if season_directory_index == 0 {
        return None;
    }

    parse_series_container_directory_metadata(directories[season_directory_index - 1])
}

fn parse_series_container_directory_metadata(value: &str) -> Option<SeriesContainerMetadata> {
    let title = humanize_directory_title(value)?;
    let parsed = parse_lookup_title_year(&title);

    Some(SeriesContainerMetadata {
        title: parsed.title,
        year: parsed.year,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedLookupTitleYear {
    title: String,
    year: Option<i32>,
}

fn parse_lookup_title_year(value: &str) -> ParsedLookupTitleYear {
    let mut tokens = value
        .split_whitespace()
        .map(std::string::ToString::to_string)
        .collect::<Vec<_>>();
    let mut title_end = tokens.len();
    let mut year = None;

    for index in 0..tokens.len() {
        if let Some(parsed_year) = parse_lookup_year_token(tokens[index].as_str()) {
            year = Some(parsed_year);
            title_end = index;
            break;
        }

        if let Some((prefix, parsed_year)) =
            split_lookup_trailing_year_suffix(tokens[index].as_str())
        {
            year = Some(parsed_year);
            tokens[index] = prefix;
            title_end = index + 1;
            break;
        }
    }

    while title_end > 0 && tokens[title_end - 1].chars().all(is_lookup_separator_char) {
        title_end -= 1;
    }

    let title = tokens[..title_end].join(" ");

    ParsedLookupTitleYear {
        title: if title.trim().is_empty() {
            value.to_string()
        } else {
            title
        },
        year,
    }
}

fn humanize_directory_title(value: &str) -> Option<String> {
    let title = value
        .replace(['.', '_', '-', '—', '–'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    (!title.trim().is_empty()).then_some(title)
}

fn split_lookup_trailing_year_suffix(token: &str) -> Option<(String, i32)> {
    let trimmed = trim_lookup_wrapping_punctuation(token);
    let characters = trimmed.chars().collect::<Vec<_>>();

    if characters.len() <= 4 {
        return None;
    }

    let suffix = characters[characters.len() - 4..]
        .iter()
        .collect::<String>();
    let year = parse_lookup_year_token(&suffix)?;
    let prefix = characters[..characters.len() - 4]
        .iter()
        .collect::<String>();

    if prefix.is_empty() || prefix.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }

    Some((prefix, year))
}

fn parse_lookup_year_token(token: &str) -> Option<i32> {
    let token = trim_lookup_wrapping_punctuation(token);

    if token.len() != 4 || !token.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }

    let year = token.parse::<i32>().ok()?;
    (1900..=2100).contains(&year).then_some(year)
}

fn trim_lookup_wrapping_punctuation(token: &str) -> &str {
    token.trim_matches(|ch| {
        matches!(
            ch,
            '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | '（' | '）' | '【' | '】' | '《' | '》'
        )
    })
}

fn is_lookup_separator_char(ch: char) -> bool {
    matches!(
        ch,
        '-' | '|' | ':' | '：' | '·' | '•' | '~' | '–' | '—' | '/' | '\\'
    )
}

fn same_lookup_title(left: &str, right: &str) -> bool {
    normalize_lookup_title(left) == normalize_lookup_title(right)
}

fn normalize_lookup_title(value: &str) -> String {
    value
        .replace(['.', '_', '-', '—', '–'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn contains_cjk_character(value: &str) -> bool {
    value.chars().any(|ch| {
        matches!(
            ch,
            '\u{3400}'..='\u{4dbf}'
                | '\u{4e00}'..='\u{9fff}'
                | '\u{f900}'..='\u{faff}'
                | '\u{20000}'..='\u{2a6df}'
                | '\u{2a700}'..='\u{2b73f}'
                | '\u{2b740}'..='\u{2b81f}'
                | '\u{2b820}'..='\u{2ceaf}'
        )
    })
}

fn is_series_variant_directory_name(name: &str) -> bool {
    let normalized = normalize_lookup_title(name);

    matches!(
        normalized.as_str(),
        "dv" | "dovi" | "dolby vision" | "hdr" | "hdr10" | "hdr10+" | "sdr"
    ) || normalized.contains("杜比")
}

fn is_season_directory_name(name: &str) -> bool {
    let normalized = name.trim().replace(['.', '_', '-', '—', '–'], " ");
    let normalized_lower = normalized.to_ascii_lowercase();
    let has_ascii_digit = normalized_lower.chars().any(|value| value.is_ascii_digit());

    if has_ascii_digit && normalized_lower.contains("season") {
        return true;
    }

    if has_ascii_digit && normalized.contains('季') {
        return true;
    }

    normalized_lower.split_whitespace().any(|token| {
        token.strip_prefix('s').is_some_and(|suffix| {
            !suffix.is_empty()
                && suffix.len() <= 2
                && suffix.chars().all(|value| value.is_ascii_digit())
        })
    })
}

#[cfg(test)]
mod tests {
    use super::{
        artwork_file_extension, build_artwork_cache_path, is_generated_episode_still_path,
        is_generic_backdrop_artwork_path, is_generic_poster_artwork_path,
        metadata_lookup_candidates, series_container_metadata_for_episode_path,
        should_replace_episode_artwork, stable_artwork_cache_key,
    };
    use mova_scan::DiscoveredMediaFile;
    use std::path::Path;
    use std::path::PathBuf;

    #[test]
    fn artwork_file_extension_uses_tmdb_url_suffix() {
        assert_eq!(
            artwork_file_extension("https://image.tmdb.org/t/p/original/poster.webp"),
            "webp"
        );
        assert_eq!(
            artwork_file_extension("https://image.tmdb.org/t/p/original/poster"),
            "jpg"
        );
    }

    #[test]
    fn build_artwork_cache_path_places_files_under_kind_directory() {
        let cache_root = Path::new("/tmp/mova-cache");
        let source_url = "https://image.tmdb.org/t/p/original/poster.jpg";

        let path = build_artwork_cache_path(cache_root, source_url, "poster");

        assert_eq!(
            path,
            cache_root
                .join("tmdb")
                .join("poster")
                .join(format!("{}.jpg", stable_artwork_cache_key(source_url)))
        );
    }

    #[test]
    fn generic_artwork_detection_matches_local_generic_names_only() {
        assert!(is_generic_poster_artwork_path(
            "/media/Season 01/poster.jpg"
        ));
        assert!(is_generic_poster_artwork_path(
            "/media/Season 01/folder.png"
        ));
        assert!(!is_generic_poster_artwork_path(
            "/media/Season 01/E01-poster.jpg"
        ));
        assert!(!is_generic_poster_artwork_path(
            "https://image.tmdb.org/t/p/original/poster.jpg"
        ));

        assert!(is_generic_backdrop_artwork_path(
            "/media/Season 01/fanart.jpg"
        ));
        assert!(is_generic_backdrop_artwork_path(
            "/media/Season 01/backdrop.png"
        ));
        assert!(!is_generic_backdrop_artwork_path(
            "/media/Season 01/E01-backdrop.jpg"
        ));
    }

    #[test]
    fn should_replace_episode_artwork_for_external_or_generic_current_paths() {
        assert!(should_replace_episode_artwork(
            None,
            is_generic_poster_artwork_path
        ));
        assert!(should_replace_episode_artwork(
            Some("https://image.tmdb.org/t/p/original/poster.jpg"),
            is_generic_poster_artwork_path
        ));
        assert!(should_replace_episode_artwork(
            Some("/media/Season 01/poster.jpg"),
            is_generic_poster_artwork_path
        ));
        assert!(should_replace_episode_artwork(
            Some("/cache/generated/episode-stills/e01.jpg"),
            is_generic_poster_artwork_path
        ));
        assert!(!should_replace_episode_artwork(
            Some("/media/Season 01/E01-poster.jpg"),
            is_generic_poster_artwork_path
        ));
    }

    #[test]
    fn generated_episode_still_detection_matches_cache_segment() {
        assert!(is_generated_episode_still_path(
            "/cache/generated/episode-stills/e01.jpg"
        ));
        assert!(!is_generated_episode_still_path(
            "/cache/generated/posters/e01.jpg"
        ));
    }

    #[test]
    fn series_container_metadata_for_episode_path_uses_parent_above_season_directory() {
        let mut file = build_discovered_episode();
        file.file_path = PathBuf::from("/media/模范出租车/S01/Taxi.Driver.S01E01.mkv");

        assert_eq!(
            series_container_metadata_for_episode_path(&file),
            Some(super::SeriesContainerMetadata {
                title: "模范出租车".to_string(),
                year: None,
            })
        );

        file.file_path = PathBuf::from("/media/Fallout/S02/DV/Fallout.S02E01.mkv");
        assert_eq!(
            series_container_metadata_for_episode_path(&file),
            Some(super::SeriesContainerMetadata {
                title: "Fallout".to_string(),
                year: None,
            })
        );
    }

    #[test]
    fn metadata_lookup_candidates_keep_file_title_first_for_non_chinese_language() {
        let mut file = build_discovered_episode();
        file.file_path = PathBuf::from("/media/模范出租车/S01/Taxi.Driver.S01E01.mkv");
        file.source_title = "Taxi Driver".to_string();

        let lookups = metadata_lookup_candidates("series", &file, "en-US");

        assert_eq!(lookups.len(), 2);
        assert_eq!(lookups[0].title, "Taxi Driver");
        assert_eq!(lookups[1].title, "模范出租车");
    }

    #[test]
    fn metadata_lookup_candidates_prefer_chinese_container_title_for_chinese_libraries() {
        let mut file = build_discovered_episode();
        file.file_path = PathBuf::from(
            "/media/overseas_tv/都是她的错.2025/Season 01/All.Her.Fault.2025.S01E01.2160p.PCOK.WEB-DL.DDP5.1.H.265-KRATOS.mkv",
        );
        file.source_title = "All Her Fault".to_string();
        file.year = Some(2025);

        let lookups = metadata_lookup_candidates("series", &file, "zh-CN");

        assert_eq!(lookups.len(), 2);
        assert_eq!(lookups[0].title, "都是她的错");
        assert_eq!(lookups[0].year, Some(2025));
        assert_eq!(lookups[1].title, "All Her Fault");
        assert_eq!(lookups[1].year, Some(2025));
    }

    #[test]
    fn metadata_lookup_candidates_use_container_year_for_file_and_container_titles() {
        let mut file = build_discovered_episode();
        file.file_path = PathBuf::from(
            "/media/overseas_tv/流氓读书会 (2025)/第 1 季 - 1080p WEB-DL AVC AAC/Study Group S01E01 - 第 1 集 - 1080p WEB-DL AVC AAC.mp4",
        );
        file.source_title = "Study Group".to_string();
        file.year = None;

        let lookups = metadata_lookup_candidates("series", &file, "zh-CN");

        assert_eq!(lookups.len(), 2);
        assert_eq!(lookups[0].title, "流氓读书会");
        assert_eq!(lookups[0].year, Some(2025));
        assert_eq!(lookups[1].title, "Study Group");
        assert_eq!(lookups[1].year, Some(2025));
    }

    fn build_discovered_episode() -> DiscoveredMediaFile {
        DiscoveredMediaFile {
            file_path: PathBuf::from("/media/series/Show/Season 01/Show.S01E01.mkv"),
            file_modified_at_ms: Some(1_700_000_000_000),
            metadata_provider: None,
            metadata_provider_item_id: None,
            title: "Show".to_string(),
            source_title: "Show".to_string(),
            original_title: None,
            sort_title: None,
            year: Some(2024),
            imdb_rating: None,
            metadata_status: None,
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
            episode_title: None,
            overview: None,
            series_poster_path: None,
            series_backdrop_path: None,
            poster_path: None,
            backdrop_path: None,
            file_size: 1024,
            container: None,
            duration_seconds: None,
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
}
