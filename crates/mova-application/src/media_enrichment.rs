use crate::metadata::{
    apply_remote_metadata, MetadataLookup, MetadataLookupCache, MetadataProvider,
    MetadataSeasonAirYearHint, RemoteMetadata, RemoteSeriesEpisodeOutline,
};
use mova_scan::DiscoveredMediaFile;
use reqwest::{header::CONTENT_TYPE, Client, Url};
use std::{
    collections::{hash_map::DefaultHasher, HashMap},
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    sync::Arc,
};

/// 复用扫描和手动刷新共用的 metadata 补全与图片缓存逻辑。
pub struct MetadataEnrichmentContext {
    artwork_cache_dir: PathBuf,
    metadata_provider: Arc<dyn MetadataProvider>,
    metadata_language: String,
    metadata_cache: MetadataLookupCache,
    series_outline_cache: HashMap<MetadataLookup, Option<RemoteSeriesEpisodeOutline>>,
    artwork_cache: HashMap<String, Option<String>>,
    artwork_client: Client,
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
        }
    }

    pub async fn enrich_file(
        &mut self,
        lookup_type: &str,
        file: &mut DiscoveredMediaFile,
    ) -> anyhow::Result<()> {
        self.enrich_file_with_progress(lookup_type, file, |_, _| {})
            .await
    }

    pub(crate) async fn enrich_group_with_progress<F>(
        &mut self,
        lookup_type: &str,
        files: &mut [DiscoveredMediaFile],
        season_air_year: Option<MetadataSeasonAirYearHint>,
        mut on_progress: F,
    ) -> anyhow::Result<()>
    where
        F: FnMut(MetadataEnrichmentStage, &DiscoveredMediaFile),
    {
        if files.is_empty() {
            return Ok(());
        }

        let primary_lookup = metadata_group_primary_lookup(
            lookup_type,
            &files[0],
            &self.metadata_language,
            season_air_year,
        );
        let mut episode_outline_lookup = primary_lookup.clone();

        on_progress(MetadataEnrichmentStage::Metadata, &files[0]);

        let resolved_remote_metadata =
            if self.metadata_provider.is_enabled() && group_needs_remote_metadata(files) {
                let metadata = self
                    .lookup_group_remote_metadata(lookup_type, &files[0], season_air_year)
                    .await?;

                if let Some(remote_metadata) = metadata.as_ref() {
                    episode_outline_lookup.provider_item_id = remote_metadata.provider_item_id;

                    for file in files.iter_mut() {
                        apply_remote_metadata_to_file(lookup_type, remote_metadata, file);
                    }
                }

                metadata
            } else {
                None
            };

        on_progress(MetadataEnrichmentStage::Artwork, &files[0]);

        let allow_remote_outline = resolved_remote_metadata.is_some();
        for file in files.iter_mut() {
            if lookup_type.eq_ignore_ascii_case("series") {
                self.enrich_episode_like_artwork(
                    &episode_outline_lookup,
                    file,
                    allow_remote_outline,
                )
                .await?;
            }

            self.cache_file_artwork(file).await;
        }

        on_progress(MetadataEnrichmentStage::Completed, &files[0]);
        Ok(())
    }

    pub async fn enrich_file_with_progress<F>(
        &mut self,
        lookup_type: &str,
        file: &mut DiscoveredMediaFile,
        mut on_progress: F,
    ) -> anyhow::Result<()>
    where
        F: FnMut(MetadataEnrichmentStage, &DiscoveredMediaFile),
    {
        let lookups = metadata_lookup_candidates(lookup_type, file, &self.metadata_language, None);
        let primary_lookup = lookups.first().cloned().unwrap_or_else(|| MetadataLookup {
            title: file.source_title.clone(),
            year: file.year,
            season_air_year: None,
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
                let candidate = self.lookup_remote_metadata_cached(lookup).await?;
                if candidate.is_some() {
                    episode_outline_lookup = lookup.clone();
                    metadata = candidate;
                    break;
                }
            }

            if let Some(remote_metadata) = metadata.as_ref() {
                episode_outline_lookup.provider_item_id = remote_metadata.provider_item_id;
            }

            if let Some(remote_metadata) = metadata.as_ref() {
                apply_remote_metadata_to_file(lookup_type, remote_metadata, file);
            }
            resolved_remote_metadata = metadata;
        }

        on_progress(MetadataEnrichmentStage::Artwork, file);

        if lookup_type.eq_ignore_ascii_case("series") {
            self.enrich_episode_like_artwork(
                &episode_outline_lookup,
                file,
                resolved_remote_metadata.is_some(),
            )
            .await?;
        }

        self.cache_file_artwork(file).await;

        on_progress(MetadataEnrichmentStage::Completed, file);
        Ok(())
    }

    async fn lookup_group_remote_metadata(
        &mut self,
        lookup_type: &str,
        file: &DiscoveredMediaFile,
        season_air_year: Option<MetadataSeasonAirYearHint>,
    ) -> anyhow::Result<Option<RemoteMetadata>> {
        let lookups =
            metadata_lookup_candidates(lookup_type, file, &self.metadata_language, season_air_year);

        for lookup in &lookups {
            let candidate = self.lookup_remote_metadata_cached(lookup).await?;
            if candidate.is_some() {
                return Ok(candidate);
            }
        }

        Ok(None)
    }

    async fn lookup_remote_metadata_cached(
        &mut self,
        lookup: &MetadataLookup,
    ) -> anyhow::Result<Option<RemoteMetadata>> {
        if let Some(metadata) = self.metadata_cache.get(lookup) {
            return Ok(metadata.clone());
        }

        let metadata = match self.metadata_provider.lookup(lookup).await {
            Ok(metadata) => metadata,
            Err(error) => {
                tracing::warn!(
                    title = %lookup.title,
                    year = lookup.year,
                    library_type = %lookup.library_type,
                    error = ?error,
                    "metadata enrichment stage failed to fetch remote metadata"
                );
                return Err(error);
            }
        };

        self.metadata_cache.insert(lookup.clone(), metadata.clone());
        Ok(metadata)
    }

    async fn lookup_series_outline_cached(
        &mut self,
        lookup: &MetadataLookup,
    ) -> anyhow::Result<Option<RemoteSeriesEpisodeOutline>> {
        if let Some(outline) = self.series_outline_cache.get(lookup) {
            return Ok(outline.clone());
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
                    "metadata enrichment stage failed to fetch remote episode outline metadata"
                );
                return Err(error);
            }
        };

        self.series_outline_cache
            .insert(lookup.clone(), outline.clone());
        Ok(outline)
    }

    async fn enrich_episode_like_artwork(
        &mut self,
        lookup: &MetadataLookup,
        file: &mut DiscoveredMediaFile,
        allow_remote_outline: bool,
    ) -> anyhow::Result<()> {
        let Some(season_number) = file.season_number else {
            return Ok(());
        };
        let Some(episode_number) = file.episode_number else {
            return Ok(());
        };

        if allow_remote_outline && self.metadata_provider.is_enabled() {
            if let Some(outline) = self.lookup_series_outline_cached(lookup).await? {
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

        Ok(())
    }
    async fn cache_file_artwork(&mut self, file: &mut DiscoveredMediaFile) {
        if let Some(series_logo_path) = file.series_logo_path.clone() {
            if let Some(cached_path) = self.cache_remote_artwork(&series_logo_path, "logo").await {
                file.series_logo_path = Some(cached_path);
            }
        }

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

        if let Some(logo_path) = file.logo_path.clone() {
            if let Some(cached_path) = self.cache_remote_artwork(&logo_path, "logo").await {
                file.logo_path = Some(cached_path);
            }
        }
    }

    pub async fn cache_remote_metadata_artwork(&mut self, metadata: &mut RemoteMetadata) {
        metadata.poster_path = self
            .cache_artwork_source(metadata.poster_path.take(), "poster")
            .await;
        metadata.backdrop_path = self
            .cache_artwork_source(metadata.backdrop_path.take(), "backdrop")
            .await;
        metadata.logo_path = self
            .cache_artwork_source(metadata.logo_path.take(), "logo")
            .await;
    }

    pub async fn cache_remote_series_outline_artwork(
        &mut self,
        outline: &mut RemoteSeriesEpisodeOutline,
    ) {
        for season in &mut outline.seasons {
            season.poster_path = self
                .cache_artwork_source(season.poster_path.take(), "poster")
                .await;
            season.backdrop_path = self
                .cache_artwork_source(season.backdrop_path.take(), "backdrop")
                .await;

            for episode in &mut season.episodes {
                episode.poster_path = self
                    .cache_artwork_source(episode.poster_path.take(), "poster")
                    .await;
                episode.backdrop_path = self
                    .cache_artwork_source(episode.backdrop_path.take(), "backdrop")
                    .await;
            }
        }
    }

    async fn cache_artwork_source(&mut self, source: Option<String>, kind: &str) -> Option<String> {
        let source = source?;

        self.cache_remote_artwork(&source, kind)
            .await
            .or(Some(source))
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

fn needs_remote_metadata(file: &DiscoveredMediaFile) -> bool {
    !has_remote_provider_binding(file)
        || file.original_title.is_none()
        || file.overview.is_none()
        || file.poster_path.is_none()
        || file.backdrop_path.is_none()
        || file.year.is_none()
        || needs_episode_container_artwork_metadata(file)
}

fn has_remote_provider_binding(file: &DiscoveredMediaFile) -> bool {
    file.metadata_provider
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
        && file.metadata_provider_item_id.is_some()
}

fn group_needs_remote_metadata(files: &[DiscoveredMediaFile]) -> bool {
    files
        .iter()
        .any(|file| needs_remote_metadata(file) || needs_remote_title_refresh(file))
}

fn needs_remote_title_refresh(file: &DiscoveredMediaFile) -> bool {
    if file.metadata_provider_item_id.is_none() {
        return false;
    }

    let source_title = file.source_title.trim();
    let title = file.title.trim();
    if source_title.is_empty() || title.is_empty() {
        return false;
    }

    let Some(year) = file.year else {
        return false;
    };

    normalize_local_title_for_refresh(title)
        == format!(
            "{} {}",
            normalize_local_title_for_refresh(source_title),
            year
        )
}

fn normalize_local_title_for_refresh(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_alphanumeric() {
                ch.to_lowercase().collect::<String>()
            } else {
                " ".to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn metadata_group_primary_lookup(
    lookup_type: &str,
    file: &DiscoveredMediaFile,
    metadata_language: &str,
    season_air_year: Option<MetadataSeasonAirYearHint>,
) -> MetadataLookup {
    metadata_lookup_candidates(lookup_type, file, metadata_language, season_air_year)
        .into_iter()
        .next()
        .unwrap_or_else(|| MetadataLookup {
            title: file.source_title.clone(),
            year: file.year,
            season_air_year,
            library_type: lookup_type.to_string(),
            language: Some(metadata_language.to_string()),
            provider_item_id: None,
        })
}

fn apply_remote_metadata_to_file(
    lookup_type: &str,
    metadata: &RemoteMetadata,
    file: &mut DiscoveredMediaFile,
) {
    if lookup_type.eq_ignore_ascii_case("series")
        && file.season_number.is_some()
        && file.episode_number.is_some()
    {
        apply_remote_series_metadata_to_episode_file(metadata, file);
        return;
    }

    apply_remote_metadata(
        Some(metadata.clone()),
        &mut file.metadata_provider,
        &mut file.metadata_provider_item_id,
        &mut file.title,
        &mut file.original_title,
        &mut file.year,
        &mut file.external_ids,
        &mut file.ratings,
        &mut file.country,
        &mut file.genres,
        &mut file.studio,
        &mut file.overview,
        &mut file.poster_path,
        &mut file.backdrop_path,
    );

    if metadata.logo_path.is_some() || is_missing_or_external_url(file.logo_path.as_deref()) {
        file.logo_path = metadata.logo_path.clone();
    }
}

fn apply_remote_series_metadata_to_episode_file(
    metadata: &RemoteMetadata,
    file: &mut DiscoveredMediaFile,
) {
    if file.metadata_provider.is_none() && metadata.provider_item_id.is_some() {
        file.metadata_provider = Some(crate::metadata::TMDB_PROVIDER_NAME.to_string());
    }

    if file.metadata_provider_item_id.is_none() {
        file.metadata_provider_item_id = metadata.provider_item_id;
    }

    if let Some(remote_title) = crate::metadata::normalize_optional_value(metadata.title.clone()) {
        file.title = remote_title;
    }

    if file.original_title.is_none() {
        file.original_title = metadata.original_title.clone();
    }

    if file.year.is_none() {
        file.year = metadata.year;
    }

    file.external_ids = metadata.external_ids.clone();
    file.ratings = metadata.ratings.clone();

    if file.country.is_none() {
        file.country = metadata.country.clone();
    }

    if file.genres.is_none() {
        file.genres = metadata.genres.clone();
    }

    if file.studio.is_none() {
        file.studio = metadata.studio.clone();
    }

    if file.overview.is_none() {
        file.overview = metadata.overview.clone();
    }

    if metadata.poster_path.is_some()
        || is_missing_or_external_url(file.series_poster_path.as_deref())
    {
        file.series_poster_path = metadata.poster_path.clone();
    }

    if metadata.backdrop_path.is_some()
        || is_missing_or_external_url(file.series_backdrop_path.as_deref())
    {
        file.series_backdrop_path = metadata.backdrop_path.clone();
    }

    if metadata.logo_path.is_some() || is_missing_or_external_url(file.series_logo_path.as_deref())
    {
        file.series_logo_path = metadata.logo_path.clone();
    }
}

fn needs_episode_container_artwork_metadata(file: &DiscoveredMediaFile) -> bool {
    if file.season_number.is_none() || file.episode_number.is_none() {
        return false;
    }

    is_missing_or_external_url(file.series_poster_path.as_deref())
        || is_missing_or_external_url(file.series_backdrop_path.as_deref())
        || is_missing_or_external_url(file.season_poster_path.as_deref())
        || is_missing_or_external_url(file.season_backdrop_path.as_deref())
}

fn is_missing_or_external_url(value: Option<&str>) -> bool {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return true;
    };

    is_external_url(value)
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
    season_air_year: Option<MetadataSeasonAirYearHint>,
) -> Vec<MetadataLookup> {
    let primary_year = file.year;
    let season_air_year = lookup_type
        .eq_ignore_ascii_case("series")
        .then_some(season_air_year)
        .flatten();

    // 元数据匹配应优先使用文件名解析出的原始标题，而不是已经被远端覆盖过的展示标题。
    let mut candidates = Vec::new();
    if let Some(provider_item_id) = file.metadata_provider_item_id {
        push_metadata_lookup_candidate_with_provider_item_id(
            &mut candidates,
            lookup_type,
            metadata_language,
            file.source_title.clone(),
            primary_year,
            season_air_year,
            provider_item_id,
        );
    }

    push_metadata_lookup_candidate(
        &mut candidates,
        lookup_type,
        metadata_language,
        file.source_title.clone(),
        primary_year,
        season_air_year,
    );
    let normalized_source_title = normalize_lookup_punctuation_candidate(&file.source_title);
    if normalized_source_title != file.source_title {
        push_metadata_lookup_candidate(
            &mut candidates,
            lookup_type,
            metadata_language,
            normalized_source_title,
            primary_year,
            season_air_year,
        );
    }

    if lookup_type.eq_ignore_ascii_case("movie") {
        if let Some(container_metadata) = movie_container_metadata_for_file_path(file) {
            if !same_lookup_title(&file.source_title, &container_metadata.title) {
                push_metadata_lookup_candidate(
                    &mut candidates,
                    lookup_type,
                    metadata_language,
                    container_metadata.title,
                    container_metadata.year.or(file.year),
                    None,
                );
            }
        }
    }

    candidates
}

fn push_metadata_lookup_candidate(
    candidates: &mut Vec<MetadataLookup>,
    lookup_type: &str,
    metadata_language: &str,
    title: String,
    year: Option<i32>,
    season_air_year: Option<MetadataSeasonAirYearHint>,
) {
    let title = title.trim();
    if title.is_empty() {
        return;
    }

    if candidates.iter().any(|candidate| {
        candidate.provider_item_id.is_none()
            && same_lookup_title(&candidate.title, title)
            && candidate.year == year
            && candidate.season_air_year == season_air_year
    }) {
        return;
    }

    candidates.push(MetadataLookup {
        title: title.to_string(),
        year,
        season_air_year,
        library_type: lookup_type.to_string(),
        language: Some(metadata_language.to_string()),
        provider_item_id: None,
    });
}

fn push_metadata_lookup_candidate_with_provider_item_id(
    candidates: &mut Vec<MetadataLookup>,
    lookup_type: &str,
    metadata_language: &str,
    title: String,
    year: Option<i32>,
    season_air_year: Option<MetadataSeasonAirYearHint>,
    provider_item_id: i64,
) {
    let title = title.trim();
    if title.is_empty() {
        return;
    }

    if candidates.iter().any(|candidate| {
        candidate.provider_item_id == Some(provider_item_id)
            && candidate.library_type == lookup_type
            && candidate.language.as_deref() == Some(metadata_language)
    }) {
        return;
    }

    candidates.push(MetadataLookup {
        title: title.to_string(),
        year,
        season_air_year,
        library_type: lookup_type.to_string(),
        language: Some(metadata_language.to_string()),
        provider_item_id: Some(provider_item_id),
    });
}

pub(crate) fn normalize_lookup_punctuation_candidate(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            '：' => ':',
            '（' => '(',
            '）' => ')',
            '【' => '[',
            '】' => ']',
            '《' => '<',
            '》' => '>',
            _ => ch,
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SeriesContainerMetadata {
    title: String,
    year: Option<i32>,
}

fn movie_container_metadata_for_file_path(
    file: &DiscoveredMediaFile,
) -> Option<SeriesContainerMetadata> {
    let parent = file.file_path.parent()?;
    let directory = parent.file_name()?.to_str()?;
    let metadata = parse_series_container_directory_metadata(directory)?;

    if !contains_cjk_character(&metadata.title) || is_generic_container_title(&metadata.title) {
        return None;
    }

    Some(metadata)
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

fn is_generic_container_title(value: &str) -> bool {
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
    let prefix = trim_lookup_wrapping_punctuation(&prefix)
        .trim_matches(is_lookup_separator_char)
        .trim()
        .to_string();

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

#[cfg(test)]
mod tests {
    use super::{
        artwork_file_extension, build_artwork_cache_path, is_generated_episode_still_path,
        is_generic_backdrop_artwork_path, is_generic_poster_artwork_path,
        metadata_lookup_candidates, needs_remote_metadata, needs_remote_title_refresh,
        should_replace_episode_artwork, stable_artwork_cache_key, MetadataEnrichmentContext,
    };
    use crate::metadata::{
        MetadataLookup, MetadataProvider, MetadataSeasonAirYearHint, RemoteMetadata,
        RemoteSeriesEpisode, RemoteSeriesEpisodeOutline, RemoteSeriesSeason,
    };
    use async_trait::async_trait;
    use mova_scan::DiscoveredMediaFile;
    use std::{
        path::{Path, PathBuf},
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
    };

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
    fn metadata_lookup_candidates_ignore_series_directory_title_for_non_chinese_language() {
        let mut file = build_discovered_episode();
        file.file_path = PathBuf::from("/media/模范出租车/S01/Taxi.Driver.S01E01.mkv");
        file.source_title = "Taxi Driver".to_string();

        let lookups = metadata_lookup_candidates("series", &file, "en-US", None);

        assert_eq!(lookups.len(), 1);
        assert_eq!(lookups[0].title, "Taxi Driver");
    }

    #[test]
    fn metadata_lookup_candidates_ignore_series_directory_title_for_chinese_libraries() {
        let mut file = build_discovered_episode();
        file.file_path = PathBuf::from(
            "/media/overseas_tv/都是她的错.2025/Season 01/All.Her.Fault.2025.S01E01.2160p.PCOK.WEB-DL.DDP5.1.H.265-KRATOS.mkv",
        );
        file.source_title = "All Her Fault".to_string();
        file.year = Some(2025);

        let lookups = metadata_lookup_candidates("series", &file, "zh-CN", None);

        assert_eq!(lookups.len(), 1);
        assert_eq!(lookups[0].title, "All Her Fault");
        assert_eq!(lookups[0].year, Some(2025));
    }

    #[test]
    fn metadata_lookup_candidates_ignore_series_directory_year() {
        let mut file = build_discovered_episode();
        file.file_path = PathBuf::from(
            "/media/overseas_tv/流氓读书会 (2025)/第 1 季 - 1080p WEB-DL AVC AAC/Study Group S01E01 - 第 1 集 - 1080p WEB-DL AVC AAC.mp4",
        );
        file.source_title = "Study Group".to_string();
        file.year = None;

        let lookups = metadata_lookup_candidates("series", &file, "zh-CN", None);

        assert_eq!(lookups.len(), 1);
        assert_eq!(lookups[0].title, "Study Group");
        assert_eq!(lookups[0].year, None);
    }

    #[test]
    fn metadata_lookup_candidates_keep_file_year_without_directory_fallback() {
        let mut file = build_discovered_episode();
        file.file_path = PathBuf::from(
            "/media/overseas_tv/莎拉的真伪人生(2026)/The.Art.of.Sarah.S01E01.2160p.NF.WEB-DL.DDP.5.1.DV.H.265.mkv",
        );
        file.source_title = "The Art of Sarah".to_string();
        file.year = Some(2026);

        let lookups = metadata_lookup_candidates("series", &file, "zh-CN", None);

        assert_eq!(lookups.len(), 1);
        assert_eq!(lookups[0].title, "The Art of Sarah");
        assert_eq!(lookups[0].year, Some(2026));
    }

    #[test]
    fn metadata_lookup_candidates_keep_later_season_year_out_of_series_year() {
        let mut file = build_discovered_episode();
        file.file_path = PathBuf::from("/media/Fallout/S02/Fallout.S02E01.2025.mkv");
        file.source_title = "Fallout".to_string();
        file.year = None;
        file.season_number = Some(2);

        let hint = MetadataSeasonAirYearHint {
            season_number: 2,
            year: 2025,
        };
        let lookups = metadata_lookup_candidates("series", &file, "zh-CN", Some(hint));

        assert_eq!(lookups.len(), 1);
        assert_eq!(lookups[0].title, "Fallout");
        assert_eq!(lookups[0].year, None);
        assert_eq!(lookups[0].season_air_year, Some(hint));
    }

    #[test]
    fn metadata_lookup_candidates_add_ascii_punctuation_variant() {
        let mut file = build_discovered_episode();
        file.file_path = PathBuf::from(
            "/media/movies/阿凡达.2025/Avatar： Fire and Ash (2025) - 1080p WEB-DL.mkv",
        );
        file.source_title = "Avatar： Fire and Ash".to_string();
        file.year = Some(2025);
        file.season_number = None;
        file.episode_number = None;

        let lookups = metadata_lookup_candidates("movie", &file, "zh-CN", None);

        assert_eq!(lookups.len(), 3);
        assert_eq!(lookups[0].title, "Avatar： Fire and Ash");
        assert_eq!(lookups[0].year, Some(2025));
        assert_eq!(lookups[1].title, "Avatar: Fire and Ash");
        assert_eq!(lookups[1].year, Some(2025));
        assert_eq!(lookups[2].title, "阿凡达");
        assert_eq!(lookups[2].year, Some(2025));
    }

    #[test]
    fn metadata_lookup_candidates_prefer_existing_provider_item_id() {
        let mut file = build_discovered_episode();
        file.file_path = PathBuf::from("/media/movies/狂野时代 (2025)/狂野时代.2025.mp4");
        file.source_title = "狂野时代".to_string();
        file.year = Some(2025);
        file.season_number = None;
        file.episode_number = None;
        file.metadata_provider_item_id = Some(123_456);

        let lookups = metadata_lookup_candidates("movie", &file, "zh-CN", None);

        assert_eq!(lookups[0].title, "狂野时代");
        assert_eq!(lookups[0].year, Some(2025));
        assert_eq!(lookups[0].provider_item_id, Some(123_456));
        assert_eq!(lookups[1].title, "狂野时代");
        assert_eq!(lookups[1].year, Some(2025));
        assert_eq!(lookups[1].provider_item_id, None);
        assert_eq!(lookups.len(), 2);
    }

    #[test]
    fn metadata_lookup_candidates_add_cjk_movie_parent_directory_fallback() {
        let mut file = build_discovered_episode();
        file.file_path = PathBuf::from(
            "/media/movies/过家家/Unexpected Family (2026) - 2160p WEB-DL DV HQ H265 DTS 5.1.mkv",
        );
        file.source_title = "Unexpected Family".to_string();
        file.year = Some(2026);
        file.season_number = None;
        file.episode_number = None;

        let lookups = metadata_lookup_candidates("movie", &file, "zh-CN", None);

        assert_eq!(lookups.len(), 2);
        assert_eq!(lookups[0].title, "Unexpected Family");
        assert_eq!(lookups[0].year, Some(2026));
        assert_eq!(lookups[1].title, "过家家");
        assert_eq!(lookups[1].year, Some(2026));
    }

    #[test]
    fn needs_remote_metadata_retries_missing_or_external_episode_container_artwork() {
        let mut file = build_discovered_episode();
        file.metadata_provider = Some("tmdb".to_string());
        file.metadata_provider_item_id = Some(77);
        file.original_title = Some("Show Original".to_string());
        file.overview = Some("Overview".to_string());
        file.poster_path = Some("/cache/episode-poster.jpg".to_string());
        file.backdrop_path = Some("/cache/episode-backdrop.jpg".to_string());
        file.series_poster_path = Some("/cache/series-poster.jpg".to_string());
        file.series_backdrop_path = Some("/cache/series-backdrop.jpg".to_string());
        file.season_poster_path = Some("/cache/season-poster.jpg".to_string());
        file.season_backdrop_path = Some("/cache/season-backdrop.jpg".to_string());

        assert!(!needs_remote_metadata(&file));

        file.series_poster_path = None;
        assert!(needs_remote_metadata(&file));

        file.series_poster_path =
            Some("https://image.tmdb.org/t/p/original/series-poster.jpg".to_string());
        assert!(needs_remote_metadata(&file));
    }

    #[test]
    fn needs_remote_metadata_retries_visible_items_without_remote_binding() {
        let mut file = build_discovered_episode();
        file.original_title = Some("Avatar: Fire and Ash".to_string());
        file.overview = Some("Overview".to_string());
        file.poster_path = Some("/api/media-items/915/poster?v=1".to_string());
        file.backdrop_path = Some("/api/media-items/915/backdrop?v=1".to_string());
        file.year = Some(2025);
        file.series_poster_path = Some("/api/media-items/915/poster?v=1".to_string());
        file.series_backdrop_path = Some("/api/media-items/915/backdrop?v=1".to_string());
        file.season_poster_path = Some("/api/media-items/915/poster?v=1".to_string());
        file.season_backdrop_path = Some("/api/media-items/915/backdrop?v=1".to_string());

        assert!(needs_remote_metadata(&file));

        file.metadata_provider = Some("tmdb".to_string());
        file.metadata_provider_item_id = Some(83533);
        assert!(!needs_remote_metadata(&file));

        file.metadata_provider = None;
        assert!(needs_remote_metadata(&file));
    }

    #[test]
    fn needs_remote_title_refresh_detects_local_year_display_title() {
        let mut file = build_discovered_episode();
        file.metadata_provider_item_id = Some(259909);
        file.source_title = "Alls Fair".to_string();
        file.title = "Alls Fair (2025)".to_string();
        file.year = Some(2025);

        assert!(needs_remote_title_refresh(&file));

        file.title = "诉讼女王".to_string();
        assert!(!needs_remote_title_refresh(&file));
    }

    #[tokio::test]
    async fn enrich_group_fetches_remote_metadata_once_and_applies_to_all_files() {
        let provider = Arc::new(CountingMetadataProvider {
            enabled: true,
            lookup_count: AtomicUsize::new(0),
        });
        let provider_for_context: Arc<dyn MetadataProvider> = provider.clone();
        let mut context = MetadataEnrichmentContext::new(
            std::env::temp_dir().join("mova-test-artwork-cache"),
            provider_for_context,
            "zh-CN".to_string(),
        );
        let mut first = build_discovered_episode();
        first.file_path = PathBuf::from(
            "/media/overseas_tv/All's Fair (2025)/Season 01/Alls Fair (2025) - S01E01.mkv",
        );
        first.title = "Alls Fair (2025)".to_string();
        first.source_title = "Alls Fair".to_string();
        first.year = Some(2025);
        first.episode_number = Some(1);

        let mut second = first.clone();
        second.file_path = PathBuf::from(
            "/media/overseas_tv/All's Fair (2025)/Season 01/Alls Fair (2025) - S01E02.mkv",
        );
        second.episode_number = Some(2);

        let mut files = vec![first, second];

        context
            .enrich_group_with_progress("series", &mut files, None, |_, _| {})
            .await
            .expect("group metadata enrichment should succeed");

        assert_eq!(provider.lookup_count.load(Ordering::SeqCst), 1);
        assert!(files.iter().all(|file| file.title == "诉讼女王"));
        assert!(files
            .iter()
            .all(|file| file.original_title.as_deref() == Some("All's Fair")));
        assert!(files
            .iter()
            .all(|file| file.metadata_provider_item_id == Some(259909)));
        assert!(files
            .iter()
            .all(|file| file.series_poster_path.as_deref() == Some("/cache/series-poster.jpg")));
        assert!(
            files
                .iter()
                .all(|file| file.series_backdrop_path.as_deref()
                    == Some("/cache/series-backdrop.jpg"))
        );
        assert!(files.iter().all(|file| file.poster_path.is_none()));
        assert!(files.iter().all(|file| file.backdrop_path.is_none()));
    }

    #[tokio::test]
    async fn enrich_group_skips_remote_lookup_when_provider_is_disabled() {
        let provider = Arc::new(CountingMetadataProvider {
            enabled: false,
            lookup_count: AtomicUsize::new(0),
        });
        let provider_for_context: Arc<dyn MetadataProvider> = provider.clone();
        let mut context = MetadataEnrichmentContext::new(
            std::env::temp_dir().join("mova-test-disabled-artwork-cache"),
            provider_for_context,
            "zh-CN".to_string(),
        );
        let mut file = build_discovered_episode();
        file.title = "Local Series".to_string();
        file.source_title = "Local Series".to_string();
        file.overview = Some("Local overview".to_string());
        let mut files = vec![file];

        context
            .enrich_group_with_progress("series", &mut files, None, |_, _| {})
            .await
            .expect("disabled provider should not block local enrichment");

        assert_eq!(provider.lookup_count.load(Ordering::SeqCst), 0);
        assert_eq!(files[0].title, "Local Series");
        assert_eq!(files[0].overview.as_deref(), Some("Local overview"));
        assert_eq!(files[0].metadata_provider_item_id, None);
    }

    #[tokio::test]
    async fn enrich_episode_artwork_keeps_remote_season_artwork() {
        let provider: Arc<dyn MetadataProvider> = Arc::new(SeasonArtworkProvider);
        let mut context = MetadataEnrichmentContext::new(
            std::env::temp_dir().join("mova-test-artwork-cache"),
            provider,
            "zh-CN".to_string(),
        );
        let mut file = build_discovered_episode();

        context
            .enrich_episode_like_artwork(&series_lookup(), &mut file, true)
            .await
            .expect("season artwork enrichment should succeed");

        assert_eq!(
            file.season_poster_path.as_deref(),
            Some("https://image.tmdb.org/t/p/original/season-poster.jpg")
        );
        assert_eq!(
            file.season_backdrop_path.as_deref(),
            Some("https://image.tmdb.org/t/p/original/season-backdrop.jpg")
        );
    }

    #[tokio::test]
    async fn enrich_episode_artwork_does_not_promote_episode_stills_to_season_artwork() {
        let provider: Arc<dyn MetadataProvider> = Arc::new(EpisodeStillOutlineProvider);
        let mut context = MetadataEnrichmentContext::new(
            std::env::temp_dir().join("mova-test-artwork-cache"),
            provider,
            "zh-CN".to_string(),
        );
        let mut file = build_discovered_episode();
        file.poster_path = Some("/cache/generated/episode-stills/show-s01e01.jpg".to_string());
        file.backdrop_path = Some("/cache/generated/episode-stills/show-s01e01.jpg".to_string());

        context
            .enrich_episode_like_artwork(&series_lookup(), &mut file, true)
            .await
            .expect("episode still enrichment should succeed");

        assert_eq!(file.series_poster_path, None);
        assert_eq!(file.series_backdrop_path, None);
        assert_eq!(file.season_poster_path, None);
        assert_eq!(file.season_backdrop_path, None);
        assert_eq!(
            file.poster_path.as_deref(),
            Some("https://image.tmdb.org/t/p/original/episode-still-poster.jpg")
        );
        assert_eq!(
            file.backdrop_path.as_deref(),
            Some("https://image.tmdb.org/t/p/original/episode-still-backdrop.jpg")
        );
    }

    #[derive(Debug)]
    struct CountingMetadataProvider {
        enabled: bool,
        lookup_count: AtomicUsize,
    }

    #[async_trait]
    impl MetadataProvider for CountingMetadataProvider {
        fn is_enabled(&self) -> bool {
            self.enabled
        }

        async fn lookup(&self, _lookup: &MetadataLookup) -> anyhow::Result<Option<RemoteMetadata>> {
            self.lookup_count.fetch_add(1, Ordering::SeqCst);

            Ok(Some(RemoteMetadata {
                provider_item_id: Some(259909),
                title: Some("诉讼女王".to_string()),
                original_title: Some("All's Fair".to_string()),
                year: Some(2025),
                overview: Some("Remote overview".to_string()),
                poster_path: Some("/cache/series-poster.jpg".to_string()),
                backdrop_path: Some("/cache/series-backdrop.jpg".to_string()),
                ..RemoteMetadata::default()
            }))
        }
    }

    #[derive(Debug)]
    struct SeasonArtworkProvider;

    #[async_trait]
    impl MetadataProvider for SeasonArtworkProvider {
        async fn lookup(&self, _lookup: &MetadataLookup) -> anyhow::Result<Option<RemoteMetadata>> {
            Ok(None)
        }

        async fn lookup_series_episode_outline(
            &self,
            _lookup: &MetadataLookup,
        ) -> anyhow::Result<Option<RemoteSeriesEpisodeOutline>> {
            Ok(Some(RemoteSeriesEpisodeOutline {
                seasons: vec![RemoteSeriesSeason {
                    season_number: 1,
                    title: Some("Season 1".to_string()),
                    poster_path: Some(
                        "https://image.tmdb.org/t/p/original/season-poster.jpg".to_string(),
                    ),
                    backdrop_path: Some(
                        "https://image.tmdb.org/t/p/original/season-backdrop.jpg".to_string(),
                    ),
                    ..RemoteSeriesSeason::default()
                }],
            }))
        }
    }

    #[derive(Debug)]
    struct EpisodeStillOutlineProvider;

    #[async_trait]
    impl MetadataProvider for EpisodeStillOutlineProvider {
        async fn lookup(&self, _lookup: &MetadataLookup) -> anyhow::Result<Option<RemoteMetadata>> {
            Ok(None)
        }

        async fn lookup_series_episode_outline(
            &self,
            _lookup: &MetadataLookup,
        ) -> anyhow::Result<Option<RemoteSeriesEpisodeOutline>> {
            Ok(Some(RemoteSeriesEpisodeOutline {
                seasons: vec![RemoteSeriesSeason {
                    season_number: 1,
                    episodes: vec![RemoteSeriesEpisode {
                        episode_number: 1,
                        poster_path: Some(
                            "https://image.tmdb.org/t/p/original/episode-still-poster.jpg"
                                .to_string(),
                        ),
                        backdrop_path: Some(
                            "https://image.tmdb.org/t/p/original/episode-still-backdrop.jpg"
                                .to_string(),
                        ),
                        ..RemoteSeriesEpisode::default()
                    }],
                    ..RemoteSeriesSeason::default()
                }],
            }))
        }
    }

    fn series_lookup() -> MetadataLookup {
        MetadataLookup {
            title: "Show".to_string(),
            year: Some(2024),
            season_air_year: None,
            library_type: "series".to_string(),
            language: Some("zh-CN".to_string()),
            provider_item_id: Some(123),
        }
    }

    fn build_discovered_episode() -> DiscoveredMediaFile {
        DiscoveredMediaFile {
            file_path: PathBuf::from("/media/series/Show/Season 01/Show.S01E01.mkv"),
            file_modified_at_ms: Some(1_700_000_000_000),
            probe_error: None,
            metadata_provider: None,
            metadata_provider_item_id: None,
            title: "Show".to_string(),
            source_title: "Show".to_string(),
            original_title: None,
            sort_title: None,
            series_sidecar_title: None,
            series_sidecar_year: None,
            year: Some(2024),
            external_ids: Vec::new(),
            ratings: Vec::new(),
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
            series_logo_path: None,
            poster_path: None,
            backdrop_path: None,
            logo_path: None,
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
