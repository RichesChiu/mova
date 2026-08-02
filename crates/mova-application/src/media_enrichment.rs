use crate::metadata::{
    apply_remote_metadata, MetadataLookup, MetadataProvider, MetadataSeasonAirYearHint,
    RemoteMetadata, RemoteSeriesEpisodeOutline,
};
use crate::{library_artwork_cache_dir, lock_cache_path, write_cache_file_atomically};
use mova_scan::DiscoveredMediaFile;
use reqwest::{
    header::CONTENT_TYPE,
    redirect::{Attempt, Policy},
    Client, StatusCode, Url,
};
use std::{
    borrow::Borrow,
    collections::{
        hash_map::{DefaultHasher, Entry},
        HashMap, VecDeque,
    },
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

const ARTWORK_CONNECT_TIMEOUT: Duration = Duration::from_secs(4);
const ARTWORK_REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_ARTWORK_RESPONSE_BYTES: usize = 20 * 1024 * 1024;
const OFFICIAL_TMDB_ARTWORK_PREFIX: &str = "https://image.tmdb.org/t/p";
const METADATA_LOOKUP_CACHE_CAPACITY: usize = 512;
const SERIES_OUTLINE_CACHE_CAPACITY: usize = 128;
const ARTWORK_RESULT_CACHE_CAPACITY: usize = 2_048;

/// An insertion-ordered cache used for scan-local request reuse.
///
/// Hits intentionally do not update eviction order. This keeps lookups O(1),
/// makes eviction deterministic, and prevents scan-wide caches from growing
/// with the number of discovered media items.
struct BoundedCache<K, V> {
    values: HashMap<K, V>,
    insertion_order: VecDeque<K>,
    capacity: usize,
}

impl<K, V> BoundedCache<K, V>
where
    K: Clone + Eq + Hash,
{
    fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "bounded cache capacity must be positive");
        Self {
            values: HashMap::with_capacity(capacity),
            insertion_order: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    fn get<Q>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: Eq + Hash + ?Sized,
    {
        self.values.get(key)
    }

    fn insert(&mut self, key: K, value: V) {
        if let Entry::Occupied(mut entry) = self.values.entry(key.clone()) {
            entry.insert(value);
            return;
        }

        if self.values.len() == self.capacity {
            let oldest = self
                .insertion_order
                .pop_front()
                .expect("a full bounded cache must have an eviction candidate");
            self.values.remove(&oldest);
        }

        self.insertion_order.push_back(key.clone());
        self.values.insert(key, value);
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.values.len()
    }

    #[cfg(test)]
    fn contains_key<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Eq + Hash + ?Sized,
    {
        self.values.contains_key(key)
    }
}

/// 复用扫描和手动刷新共用的 metadata 补全与图片缓存逻辑。
pub struct MetadataEnrichmentContext {
    artwork_cache_dir: PathBuf,
    metadata_provider: Arc<dyn MetadataProvider>,
    metadata_language: String,
    metadata_cache: BoundedCache<MetadataLookup, Option<RemoteMetadata>>,
    series_outline_cache: BoundedCache<MetadataLookup, Option<RemoteSeriesEpisodeOutline>>,
    artwork_cache: BoundedCache<String, Option<String>>,
    trusted_artwork_bases: Arc<Vec<Url>>,
    artwork_client: Client,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MetadataEnrichmentStage {
    Metadata,
    Artwork,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MetadataEnrichmentOutcome {
    pub remote_lookup_performed: bool,
    pub remote_metadata_applied: bool,
    pub tmdb_remote_snapshot_json: Option<String>,
    pub tmdb_remote_snapshot_renews_retention: bool,
}

impl MetadataEnrichmentContext {
    /// 扫描和手动刷新都会复用这个上下文。
    /// 语言在创建时就绑定下来，确保同一个库的所有 TMDB 请求都落在同一语言版本上。
    pub fn new(
        cache_dir: PathBuf,
        library_id: i64,
        metadata_provider: Arc<dyn MetadataProvider>,
        metadata_language: String,
    ) -> Self {
        let trusted_artwork_bases = trusted_artwork_bases(metadata_provider.as_ref());
        Self {
            artwork_cache_dir: library_artwork_cache_dir(&cache_dir, library_id),
            metadata_provider,
            metadata_language,
            metadata_cache: BoundedCache::new(METADATA_LOOKUP_CACHE_CAPACITY),
            series_outline_cache: BoundedCache::new(SERIES_OUTLINE_CACHE_CAPACITY),
            artwork_cache: BoundedCache::new(ARTWORK_RESULT_CACHE_CAPACITY),
            artwork_client: build_artwork_client(trusted_artwork_bases.clone()),
            trusted_artwork_bases,
        }
    }

    #[cfg(test)]
    pub(crate) async fn enrich_group_with_progress<F>(
        &mut self,
        lookup_type: &str,
        files: &mut [DiscoveredMediaFile],
        season_air_year: Option<MetadataSeasonAirYearHint>,
        on_progress: F,
    ) -> anyhow::Result<MetadataEnrichmentOutcome>
    where
        F: FnMut(MetadataEnrichmentStage, &DiscoveredMediaFile),
    {
        self.enrich_group_with_lookup_hint_and_progress(
            lookup_type,
            files,
            season_air_year,
            None,
            on_progress,
        )
        .await
    }

    pub(crate) async fn enrich_group_with_lookup_hint_and_progress<F>(
        &mut self,
        lookup_type: &str,
        files: &mut [DiscoveredMediaFile],
        season_air_year: Option<MetadataSeasonAirYearHint>,
        provider_item_id_hint: Option<&str>,
        on_progress: F,
    ) -> anyhow::Result<MetadataEnrichmentOutcome>
    where
        F: FnMut(MetadataEnrichmentStage, &DiscoveredMediaFile),
    {
        self.enrich_group_with_lookup_hint_and_progress_mode(
            lookup_type,
            files,
            season_air_year,
            provider_item_id_hint,
            false,
            on_progress,
        )
        .await
    }

    /// A user-requested refresh must actually re-read an accepted provider
    /// identity even when every visible field is already populated. Normal
    /// scans retain their missing-data optimization through the method above.
    pub(crate) async fn refresh_group_with_lookup_hint_and_progress<F>(
        &mut self,
        lookup_type: &str,
        files: &mut [DiscoveredMediaFile],
        season_air_year: Option<MetadataSeasonAirYearHint>,
        provider_item_id_hint: Option<&str>,
        on_progress: F,
    ) -> anyhow::Result<MetadataEnrichmentOutcome>
    where
        F: FnMut(MetadataEnrichmentStage, &DiscoveredMediaFile),
    {
        self.enrich_group_with_lookup_hint_and_progress_mode(
            lookup_type,
            files,
            season_air_year,
            provider_item_id_hint,
            provider_item_id_hint.is_some(),
            on_progress,
        )
        .await
    }

    async fn enrich_group_with_lookup_hint_and_progress_mode<F>(
        &mut self,
        lookup_type: &str,
        files: &mut [DiscoveredMediaFile],
        season_air_year: Option<MetadataSeasonAirYearHint>,
        provider_item_id_hint: Option<&str>,
        force_remote_lookup: bool,
        mut on_progress: F,
    ) -> anyhow::Result<MetadataEnrichmentOutcome>
    where
        F: FnMut(MetadataEnrichmentStage, &DiscoveredMediaFile),
    {
        if files.is_empty() {
            return Ok(MetadataEnrichmentOutcome {
                remote_lookup_performed: false,
                remote_metadata_applied: false,
                tmdb_remote_snapshot_json: None,
                tmdb_remote_snapshot_renews_retention: false,
            });
        }

        let primary_lookup = metadata_group_primary_lookup(
            lookup_type,
            &files[0],
            &self.metadata_language,
            season_air_year,
            provider_item_id_hint,
        );
        let mut episode_outline_lookup = primary_lookup.clone();

        on_progress(MetadataEnrichmentStage::Metadata, &files[0]);

        let remote_lookup_performed = self.metadata_provider.is_enabled()
            && (force_remote_lookup || group_needs_remote_metadata(files));
        let resolved_remote_metadata = if remote_lookup_performed {
            let metadata = self
                .lookup_group_remote_metadata(
                    lookup_type,
                    &files[0],
                    season_air_year,
                    provider_item_id_hint,
                )
                .await?;

            if let Some(remote_metadata) = metadata.as_ref() {
                episode_outline_lookup.provider_item_id = remote_metadata.provider_item_id.clone();

                for file in files.iter_mut() {
                    apply_remote_metadata_to_file(lookup_type, remote_metadata, file);
                    if provider_item_id_hint.is_some() {
                        replace_remote_binding(file, remote_metadata);
                    }
                }
            }

            metadata
        } else {
            None
        };

        on_progress(MetadataEnrichmentStage::Artwork, &files[0]);

        let mut resolved_remote_outline = if lookup_type.eq_ignore_ascii_case("series")
            && resolved_remote_metadata.is_some()
        {
            match self
                .metadata_provider
                .lookup_complete_series_episode_outline(&episode_outline_lookup)
                .await
            {
                Ok(Some(outline)) => {
                    self.series_outline_cache.insert(
                        canonical_metadata_request_key(&episode_outline_lookup),
                        Some(outline.clone()),
                    );
                    Some(outline)
                }
                Ok(None) => None,
                Err(error) => {
                    tracing::warn!(
                        title = %episode_outline_lookup.title,
                        year = episode_outline_lookup.year,
                        provider_item_id = episode_outline_lookup.provider_item_id,
                        error = ?error,
                        "complete remote series outline was unavailable; retaining partial scan enrichment without renewing TMDB ownership"
                    );
                    self.lookup_series_outline_cached(&episode_outline_lookup)
                        .await?
                }
            }
        } else {
            None
        };
        for file in files.iter_mut() {
            apply_remote_episode_outline_to_file(resolved_remote_outline.as_ref(), file);

            self.cache_file_artwork(file).await;
        }

        let mut snapshot_metadata = resolved_remote_metadata.clone();
        if let Some(metadata) = snapshot_metadata.as_mut() {
            self.cache_remote_metadata_artwork(metadata).await;
        }
        if let Some(outline) = resolved_remote_outline.as_mut() {
            self.cache_remote_series_outline_artwork(outline).await;
        }
        let tmdb_remote_snapshot_json = match snapshot_metadata.as_ref() {
            Some(metadata) => Some(crate::tmdb_revalidation::serialize_tmdb_remote_snapshot(
                metadata,
                resolved_remote_outline.clone(),
            )?),
            None => None,
        };

        on_progress(MetadataEnrichmentStage::Completed, &files[0]);
        Ok(MetadataEnrichmentOutcome {
            remote_lookup_performed,
            remote_metadata_applied: resolved_remote_metadata.is_some(),
            tmdb_remote_snapshot_json,
            // Only the dedicated direct-ID compliance revalidation may move
            // the 150/180-day clocks. Normal enrichment records ownership but
            // remains due for strict verification.
            tmdb_remote_snapshot_renews_retention: false,
        })
    }

    async fn lookup_group_remote_metadata(
        &mut self,
        lookup_type: &str,
        file: &DiscoveredMediaFile,
        season_air_year: Option<MetadataSeasonAirYearHint>,
        provider_item_id_hint: Option<&str>,
    ) -> anyhow::Result<Option<RemoteMetadata>> {
        let lookups = metadata_lookup_candidates_with_provider_item_id_hint(
            lookup_type,
            file,
            &self.metadata_language,
            season_air_year,
            provider_item_id_hint,
        );

        for lookup in &lookups {
            let candidate = match self.lookup_remote_metadata_cached(lookup).await {
                Ok(candidate) => candidate,
                Err(error)
                    if provider_item_id_hint.is_some()
                        && metadata_provider_error_is_not_found(&error) =>
                {
                    return Ok(None);
                }
                Err(error) => return Err(error),
            };
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
        let cache_key = canonical_metadata_request_key(lookup);
        if let Some(metadata) = self.metadata_cache.get(&cache_key) {
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

        self.metadata_cache.insert(cache_key, metadata.clone());
        Ok(metadata)
    }

    async fn lookup_series_outline_cached(
        &mut self,
        lookup: &MetadataLookup,
    ) -> anyhow::Result<Option<RemoteSeriesEpisodeOutline>> {
        let cache_key = canonical_metadata_request_key(lookup);
        if let Some(outline) = self.series_outline_cache.get(&cache_key) {
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

        self.series_outline_cache.insert(cache_key, outline.clone());
        Ok(outline)
    }

    #[cfg(test)]
    async fn enrich_episode_like_artwork(
        &mut self,
        lookup: &MetadataLookup,
        file: &mut DiscoveredMediaFile,
        allow_remote_outline: bool,
    ) -> anyhow::Result<()> {
        if allow_remote_outline && self.metadata_provider.is_enabled() {
            let outline = self.lookup_series_outline_cached(lookup).await?;
            apply_remote_episode_outline_to_file(outline.as_ref(), file);
        }
        Ok(())
    }

    async fn cache_file_artwork(&mut self, file: &mut DiscoveredMediaFile) {
        file.series_logo_path = self
            .cache_artwork_source(file.series_logo_path.take(), "logo")
            .await;
        file.series_poster_path = self
            .cache_artwork_source(file.series_poster_path.take(), "poster")
            .await;
        file.series_backdrop_path = self
            .cache_artwork_source(file.series_backdrop_path.take(), "backdrop")
            .await;
        file.season_poster_path = self
            .cache_artwork_source(file.season_poster_path.take(), "poster")
            .await;
        file.season_backdrop_path = self
            .cache_artwork_source(file.season_backdrop_path.take(), "backdrop")
            .await;
        file.poster_path = self
            .cache_artwork_source(file.poster_path.take(), "poster")
            .await;
        file.backdrop_path = self
            .cache_artwork_source(file.backdrop_path.take(), "backdrop")
            .await;
        file.logo_path = self
            .cache_artwork_source(file.logo_path.take(), "logo")
            .await;
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

    /// Removes external artwork that is outside the configured provider image
    /// origins. This is also applied to the pre-enrichment snapshot used when
    /// a later provider request fails, so restoring trusted metadata cannot
    /// reintroduce an arbitrary URL read from an NFO file.
    pub(crate) fn sanitize_file_artwork_sources(&self, file: &mut DiscoveredMediaFile) {
        sanitize_file_artwork_sources(file, &self.trusted_artwork_bases);
    }

    async fn cache_artwork_source(&mut self, source: Option<String>, kind: &str) -> Option<String> {
        let source = source?;

        if !is_external_url(&source) {
            return Some(source);
        }

        if !is_allowed_remote_artwork_url(&source, &self.trusted_artwork_bases) {
            let source_host = Url::parse(&source)
                .ok()
                .and_then(|url| url.host_str().map(str::to_owned));
            tracing::warn!(
                kind,
                source_host = ?source_host,
                "refusing untrusted remote artwork URL"
            );
            return None;
        }

        self.cache_remote_artwork(&source, kind)
            .await
            .or(Some(source))
    }

    async fn cache_remote_artwork(&mut self, source_url: &str, kind: &str) -> Option<String> {
        if !is_allowed_remote_artwork_url(source_url, &self.trusted_artwork_bases) {
            return None;
        }

        if let Some(cached_path) = self.artwork_cache.get(source_url) {
            return cached_path.clone();
        }

        let cache_path = build_artwork_cache_path(&self.artwork_cache_dir, source_url, kind);
        let _cache_guard = lock_cache_path(&cache_path).await;

        if is_valid_artwork_cache_file(&cache_path).await {
            let cached = Some(cache_path.to_string_lossy().to_string());
            self.artwork_cache
                .insert(source_url.to_string(), cached.clone());
            return cached;
        }
        if tokio::fs::metadata(&cache_path).await.is_ok() {
            let _ = tokio::fs::remove_file(&cache_path).await;
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

        let mut response = match response.error_for_status() {
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
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split(';').next())
            .map(str::trim)
            .map(str::to_owned);
        if !content_type
            .as_deref()
            .is_some_and(is_allowed_artwork_content_type)
        {
            tracing::warn!(
                kind,
                source_url,
                content_type = ?content_type,
                "artwork response has an unsupported content type"
            );
            self.artwork_cache.insert(source_url.to_string(), None);
            return None;
        }
        let expected_content_type =
            artwork_content_type_for_extension(artwork_file_extension(source_url));
        if content_type.as_deref() != Some(expected_content_type) {
            tracing::warn!(
                kind,
                source_url,
                content_type = ?content_type,
                expected_content_type,
                "artwork response type does not match its cache file extension"
            );
            self.artwork_cache.insert(source_url.to_string(), None);
            return None;
        }

        if response
            .content_length()
            .is_some_and(|length| length > MAX_ARTWORK_RESPONSE_BYTES as u64)
        {
            tracing::warn!(
                kind,
                source_url,
                content_length = response.content_length(),
                "artwork response exceeds the configured size limit"
            );
            self.artwork_cache.insert(source_url.to_string(), None);
            return None;
        }

        let mut bytes = Vec::with_capacity(
            response
                .content_length()
                .unwrap_or_default()
                .min(MAX_ARTWORK_RESPONSE_BYTES as u64) as usize,
        );
        loop {
            let chunk = match response.chunk().await {
                Ok(chunk) => chunk,
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
            let Some(chunk) = chunk else {
                break;
            };
            if bytes.len().saturating_add(chunk.len()) > MAX_ARTWORK_RESPONSE_BYTES {
                tracing::warn!(
                    kind,
                    source_url,
                    "artwork response exceeded the configured size limit while streaming"
                );
                self.artwork_cache.insert(source_url.to_string(), None);
                return None;
            }
            bytes.extend_from_slice(&chunk);
        }

        if !artwork_bytes_match_content_type(&bytes, content_type.as_deref().unwrap_or_default()) {
            tracing::warn!(
                kind,
                source_url,
                content_type = ?content_type,
                "artwork response body does not match its declared image type"
            );
            self.artwork_cache.insert(source_url.to_string(), None);
            return None;
        }

        if let Err(error) = write_cache_file_atomically(&cache_path, &bytes).await {
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

fn metadata_provider_error_is_not_found(error: &anyhow::Error) -> bool {
    error.chain().any(|source| {
        source
            .downcast_ref::<reqwest::Error>()
            .and_then(reqwest::Error::status)
            == Some(StatusCode::NOT_FOUND)
    })
}

fn canonical_metadata_request_key(lookup: &MetadataLookup) -> MetadataLookup {
    let library_type = lookup.library_type.trim().to_ascii_lowercase();
    let language = lookup
        .language
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase());

    if let Some(provider_item_id) = lookup
        .provider_item_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return MetadataLookup {
            title: String::new(),
            year: None,
            season_air_year: None,
            library_type,
            language,
            provider_item_id: Some(provider_item_id.to_ascii_lowercase()),
        };
    }

    MetadataLookup {
        title: normalize_metadata_cache_title(&lookup.title),
        year: lookup.year,
        season_air_year: lookup.season_air_year,
        library_type,
        language,
        provider_item_id: None,
    }
}

fn normalize_metadata_cache_title(value: &str) -> String {
    value
        .chars()
        .flat_map(char::to_lowercase)
        .map(|character| {
            if character.is_alphanumeric() {
                character
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
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
    provider_item_id_hint: Option<&str>,
) -> MetadataLookup {
    metadata_lookup_candidates_with_provider_item_id_hint(
        lookup_type,
        file,
        metadata_language,
        season_air_year,
        provider_item_id_hint,
    )
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

fn replace_remote_binding(file: &mut DiscoveredMediaFile, metadata: &RemoteMetadata) {
    let Some(provider_item_id) = metadata
        .provider_item_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return;
    };

    file.metadata_provider = Some(crate::metadata::TMDB_PROVIDER_NAME.to_string());
    file.metadata_provider_item_id = Some(provider_item_id.to_string());
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

fn apply_remote_episode_outline_to_file(
    outline: Option<&RemoteSeriesEpisodeOutline>,
    file: &mut DiscoveredMediaFile,
) {
    let (Some(outline), Some(season_number), Some(episode_number)) =
        (outline, file.season_number, file.episode_number)
    else {
        return;
    };
    let Some(remote_season) = outline
        .seasons
        .iter()
        .find(|season| season.season_number == season_number)
    else {
        return;
    };

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

    let Some(remote_episode) = remote_season
        .episodes
        .iter()
        .find(|episode| episode.episode_number == episode_number)
    else {
        return;
    };
    if file.episode_title.is_none() {
        file.episode_title = remote_episode.title.clone();
    }
    if file.episode_overview.is_none() {
        file.episode_overview = remote_episode.overview.clone();
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

fn apply_remote_series_metadata_to_episode_file(
    metadata: &RemoteMetadata,
    file: &mut DiscoveredMediaFile,
) {
    if file.metadata_provider.is_none() && metadata.provider_item_id.is_some() {
        file.metadata_provider = Some(crate::metadata::TMDB_PROVIDER_NAME.to_string());
    }

    if file.metadata_provider_item_id.is_none() {
        file.metadata_provider_item_id = metadata.provider_item_id.clone();
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
    Url::parse(value).ok().is_some_and(|url| {
        url.scheme().eq_ignore_ascii_case("http") || url.scheme().eq_ignore_ascii_case("https")
    })
}

pub(crate) fn trusted_artwork_bases(metadata_provider: &dyn MetadataProvider) -> Arc<Vec<Url>> {
    let mut bases = Vec::new();
    for value in [
        Some(OFFICIAL_TMDB_ARTWORK_PREFIX),
        metadata_provider.trusted_artwork_base_url(),
    ]
    .into_iter()
    .flatten()
    {
        let Ok(url) = Url::parse(value) else {
            tracing::warn!("ignoring invalid trusted artwork base URL");
            continue;
        };
        if !matches!(url.scheme(), "http" | "https")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            tracing::warn!("ignoring malformed trusted artwork base URL");
            continue;
        }
        if !bases.iter().any(|existing| existing == &url) {
            bases.push(url);
        }
    }

    Arc::new(bases)
}

fn build_artwork_client(trusted_bases: Arc<Vec<Url>>) -> Client {
    Client::builder()
        .connect_timeout(ARTWORK_CONNECT_TIMEOUT)
        .timeout(ARTWORK_REQUEST_TIMEOUT)
        .redirect(Policy::custom(move |attempt| {
            validate_artwork_redirect(attempt, &trusted_bases)
        }))
        .build()
        .expect("static artwork HTTP client configuration must be valid")
}

fn validate_artwork_redirect(
    attempt: Attempt<'_>,
    trusted_bases: &[Url],
) -> reqwest::redirect::Action {
    if attempt.previous().len() >= 5 {
        return attempt.error("too many artwork redirects");
    }

    if is_allowed_remote_artwork_url_value(attempt.url(), trusted_bases) {
        attempt.follow()
    } else {
        attempt.error("artwork redirect target is not trusted")
    }
}

fn is_allowed_remote_artwork_url(value: &str, trusted_bases: &[Url]) -> bool {
    Url::parse(value)
        .ok()
        .as_ref()
        .is_some_and(|url| is_allowed_remote_artwork_url_value(url, trusted_bases))
}

fn is_allowed_remote_artwork_url_value(url: &Url, trusted_bases: &[Url]) -> bool {
    url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && trusted_bases
            .iter()
            .any(|base| artwork_url_is_within_base(url, base))
}

pub(crate) fn sanitize_file_artwork_sources(file: &mut DiscoveredMediaFile, trusted_bases: &[Url]) {
    for source in [
        &mut file.series_logo_path,
        &mut file.series_poster_path,
        &mut file.series_backdrop_path,
        &mut file.season_poster_path,
        &mut file.season_backdrop_path,
        &mut file.poster_path,
        &mut file.backdrop_path,
        &mut file.logo_path,
    ] {
        if source.as_deref().is_some_and(|value| {
            is_external_url(value) && !is_allowed_remote_artwork_url(value, trusted_bases)
        }) {
            *source = None;
        }
    }
}

fn artwork_url_is_within_base(url: &Url, base: &Url) -> bool {
    let encoded_path = url.path().to_ascii_lowercase();
    if encoded_path.contains("%2e")
        || encoded_path.contains("%2f")
        || encoded_path.contains("%5c")
        || encoded_path.contains('\\')
    {
        return false;
    }

    if url.scheme() != base.scheme()
        || !url
            .host_str()
            .zip(base.host_str())
            .is_some_and(|(url_host, base_host)| url_host.eq_ignore_ascii_case(base_host))
        || url.port_or_known_default() != base.port_or_known_default()
    {
        return false;
    }

    let base_path = base.path().trim_end_matches('/');
    base_path.is_empty()
        || base_path == "/"
        || url.path() == base_path
        || url
            .path()
            .strip_prefix(base_path)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

fn is_allowed_artwork_content_type(content_type: &str) -> bool {
    matches!(
        content_type.to_ascii_lowercase().as_str(),
        "image/jpeg" | "image/png" | "image/webp" | "image/gif" | "image/avif"
    )
}

fn artwork_content_type_for_extension(extension: &str) -> &'static str {
    match extension {
        "jpg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "avif" => "image/avif",
        _ => "application/octet-stream",
    }
}

fn artwork_bytes_match_content_type(bytes: &[u8], content_type: &str) -> bool {
    match content_type.to_ascii_lowercase().as_str() {
        "image/jpeg" => bytes.starts_with(&[0xff, 0xd8, 0xff]),
        "image/png" => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "image/webp" => bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP",
        "image/gif" => bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a"),
        "image/avif" => {
            bytes.len() >= 12
                && &bytes[4..8] == b"ftyp"
                && matches!(&bytes[8..12], b"avif" | b"avis")
        }
        _ => false,
    }
}

async fn is_valid_artwork_cache_file(path: &Path) -> bool {
    use tokio::io::AsyncReadExt;

    let Ok(metadata) = tokio::fs::metadata(path).await else {
        return false;
    };
    if !metadata.is_file()
        || metadata.len() == 0
        || metadata.len() > MAX_ARTWORK_RESPONSE_BYTES as u64
    {
        return false;
    }

    let Ok(mut file) = tokio::fs::File::open(path).await else {
        return false;
    };
    let mut header = [0_u8; 16];
    let Ok(read) = file.read(&mut header).await else {
        return false;
    };

    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase());
    extension.is_some_and(|extension| {
        artwork_bytes_match_content_type(
            &header[..read],
            artwork_content_type_for_extension(&extension),
        )
    })
}

fn metadata_lookup_candidates_with_provider_item_id_hint(
    lookup_type: &str,
    file: &DiscoveredMediaFile,
    metadata_language: &str,
    season_air_year: Option<MetadataSeasonAirYearHint>,
    provider_item_id_hint: Option<&str>,
) -> Vec<MetadataLookup> {
    let primary_year = file.year;
    let season_air_year = lookup_type
        .eq_ignore_ascii_case("series")
        .then_some(season_air_year)
        .flatten();

    if let Some(provider_item_id) = provider_item_id_hint
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return vec![MetadataLookup {
            title: file.source_title.clone(),
            year: primary_year,
            season_air_year,
            library_type: lookup_type.to_string(),
            language: Some(metadata_language.to_string()),
            provider_item_id: Some(provider_item_id.to_string()),
        }];
    }

    if let Some(provider_item_id) = file
        .metadata_provider_item_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return vec![MetadataLookup {
            title: file.source_title.clone(),
            year: primary_year,
            season_air_year,
            library_type: lookup_type.to_string(),
            language: Some(metadata_language.to_string()),
            provider_item_id: Some(provider_item_id.to_string()),
        }];
    }

    // 元数据匹配应优先使用文件名解析出的原始标题，而不是已经被远端覆盖过的展示标题。
    let mut candidates = Vec::new();
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

    for (index, token) in tokens.iter_mut().enumerate() {
        if let Some(parsed_year) = parse_lookup_year_token(token.as_str()) {
            year = Some(parsed_year);
            title_end = index;
            break;
        }

        if let Some((prefix, parsed_year)) = split_lookup_trailing_year_suffix(token.as_str()) {
            year = Some(parsed_year);
            *token = prefix;
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
        artwork_bytes_match_content_type, artwork_file_extension, build_artwork_cache_path,
        is_allowed_remote_artwork_url, is_generated_episode_still_path,
        is_generic_backdrop_artwork_path, is_generic_poster_artwork_path,
        metadata_lookup_candidates_with_provider_item_id_hint, needs_remote_metadata,
        needs_remote_title_refresh, should_replace_episode_artwork, stable_artwork_cache_key,
        trusted_artwork_bases, MetadataEnrichmentContext, ARTWORK_RESULT_CACHE_CAPACITY,
        METADATA_LOOKUP_CACHE_CAPACITY, SERIES_OUTLINE_CACHE_CAPACITY,
    };
    use crate::metadata::{
        MetadataLookup, MetadataProvider, MetadataSeasonAirYearHint, NullMetadataProvider,
        RemoteMetadata, RemoteSeriesEpisode, RemoteSeriesEpisodeOutline, RemoteSeriesSeason,
    };
    use async_trait::async_trait;
    use mova_scan::DiscoveredMediaFile;
    use std::{
        path::{Path, PathBuf},
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc, Mutex,
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
    fn remote_artwork_allows_only_the_tmdb_https_image_endpoint() {
        let trusted_bases = trusted_artwork_bases(&NullMetadataProvider);
        assert!(is_allowed_remote_artwork_url(
            "https://image.tmdb.org/t/p/original/poster.jpg",
            &trusted_bases
        ));
        assert!(!is_allowed_remote_artwork_url(
            "http://image.tmdb.org/t/p/original/poster.jpg",
            &trusted_bases
        ));
        assert!(!is_allowed_remote_artwork_url(
            "https://image.tmdb.org.evil.example/t/p/original/poster.jpg",
            &trusted_bases
        ));
        assert!(!is_allowed_remote_artwork_url(
            "https://127.0.0.1/t/p/original/poster.jpg",
            &trusted_bases
        ));
        assert!(!is_allowed_remote_artwork_url(
            "https://image.tmdb.org:444/t/p/original/poster.jpg",
            &trusted_bases
        ));
        assert!(!is_allowed_remote_artwork_url(
            "https://user@image.tmdb.org/t/p/original/poster.jpg",
            &trusted_bases
        ));
        assert!(!is_allowed_remote_artwork_url(
            "https://image.tmdb.org/private/poster.jpg",
            &trusted_bases
        ));
        assert!(!is_allowed_remote_artwork_url(
            "https://image.tmdb.org/t/p/original/poster.jpg?url=http://127.0.0.1/private",
            &trusted_bases
        ));
        assert!(!is_allowed_remote_artwork_url(
            "https://image.tmdb.org/t/p/original/poster.jpg#fragment",
            &trusted_bases
        ));
    }

    #[test]
    fn explicitly_configured_artwork_proxy_obeys_origin_and_path_boundaries() {
        struct ProxyMetadataProvider;

        #[async_trait]
        impl MetadataProvider for ProxyMetadataProvider {
            fn trusted_artwork_base_url(&self) -> Option<&str> {
                Some("http://10.0.0.5:8080/tmdb/images")
            }

            async fn lookup(
                &self,
                _lookup: &MetadataLookup,
            ) -> anyhow::Result<Option<RemoteMetadata>> {
                Ok(None)
            }
        }

        let trusted_bases = trusted_artwork_bases(&ProxyMetadataProvider);
        assert!(is_allowed_remote_artwork_url(
            "http://10.0.0.5:8080/tmdb/images/poster.jpg",
            &trusted_bases
        ));
        assert!(!is_allowed_remote_artwork_url(
            "http://10.0.0.5:8080/tmdb/images-evil/poster.jpg",
            &trusted_bases
        ));
        assert!(!is_allowed_remote_artwork_url(
            "http://10.0.0.5:8080/tmdb/images/%2e%2e/admin",
            &trusted_bases
        ));
        assert!(!is_allowed_remote_artwork_url(
            "http://10.0.0.5:8081/tmdb/images/poster.jpg",
            &trusted_bases
        ));
        assert!(!is_allowed_remote_artwork_url(
            "http://10.0.0.6:8080/tmdb/images/poster.jpg",
            &trusted_bases
        ));
        assert!(!is_allowed_remote_artwork_url(
            "http://10.0.0.5:8080/tmdb/images/poster.jpg?source=http://127.0.0.1",
            &trusted_bases
        ));
    }

    #[test]
    fn artwork_payload_must_match_the_declared_image_type() {
        assert!(artwork_bytes_match_content_type(
            b"\x89PNG\r\n\x1a\npayload",
            "image/png"
        ));
        assert!(!artwork_bytes_match_content_type(
            b"<html>not an image</html>",
            "image/jpeg"
        ));
        assert!(!artwork_bytes_match_content_type(
            b"\x89PNG\r\n\x1a\npayload",
            "image/jpeg"
        ));
    }

    #[tokio::test]
    async fn file_artwork_drops_untrusted_remote_urls_and_keeps_local_sidecars() {
        let provider: Arc<dyn MetadataProvider> = Arc::new(NullMetadataProvider);
        let mut context = MetadataEnrichmentContext::new(
            std::env::temp_dir().join("mova-untrusted-artwork-test"),
            1,
            provider,
            "zh-CN".to_string(),
        );
        let mut file = build_discovered_episode();
        file.poster_path = Some("http://127.0.0.1:8080/private.jpg".to_string());
        file.backdrop_path = Some("/media/series/Show/fanart.jpg".to_string());

        context.cache_file_artwork(&mut file).await;

        assert_eq!(file.poster_path, None);
        assert_eq!(
            file.backdrop_path.as_deref(),
            Some("/media/series/Show/fanart.jpg")
        );
    }

    #[test]
    fn artwork_snapshot_sanitizer_drops_untrusted_urls_before_error_restore() {
        let provider: Arc<dyn MetadataProvider> = Arc::new(NullMetadataProvider);
        let context = MetadataEnrichmentContext::new(
            std::env::temp_dir().join("mova-untrusted-artwork-snapshot-test"),
            1,
            provider,
            "zh-CN".to_string(),
        );
        let mut file = build_discovered_episode();
        file.series_poster_path = Some("http://127.0.0.1:8080/private.jpg".to_string());
        file.poster_path =
            Some("https://image.tmdb.org/t/p/original/episode-poster.jpg".to_string());
        file.backdrop_path = Some("/media/series/Show/fanart.jpg".to_string());

        context.sanitize_file_artwork_sources(&mut file);

        assert_eq!(file.series_poster_path, None);
        assert_eq!(
            file.poster_path.as_deref(),
            Some("https://image.tmdb.org/t/p/original/episode-poster.jpg")
        );
        assert_eq!(
            file.backdrop_path.as_deref(),
            Some("/media/series/Show/fanart.jpg")
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
    fn scan_local_caches_stay_bounded_and_evict_the_oldest_insertion() {
        let provider: Arc<dyn MetadataProvider> = Arc::new(NullMetadataProvider);
        let mut context = MetadataEnrichmentContext::new(
            std::env::temp_dir().join("mova-bounded-enrichment-cache-test"),
            1,
            provider,
            "zh-CN".to_string(),
        );
        let lookup = |index: usize| MetadataLookup {
            title: format!("Title {index}"),
            year: Some(2000 + index as i32),
            season_air_year: None,
            library_type: "movie".to_string(),
            language: Some("zh-CN".to_string()),
            provider_item_id: None,
        };

        for index in 0..METADATA_LOOKUP_CACHE_CAPACITY {
            context.metadata_cache.insert(lookup(index), None);
        }
        let oldest_metadata_lookup = lookup(0);
        assert!(context
            .metadata_cache
            .get(&oldest_metadata_lookup)
            .is_some());
        context
            .metadata_cache
            .insert(lookup(METADATA_LOOKUP_CACHE_CAPACITY), None);
        assert_eq!(context.metadata_cache.len(), METADATA_LOOKUP_CACHE_CAPACITY);
        assert!(!context.metadata_cache.contains_key(&oldest_metadata_lookup));
        assert!(context
            .metadata_cache
            .contains_key(&lookup(METADATA_LOOKUP_CACHE_CAPACITY)));

        for index in 0..SERIES_OUTLINE_CACHE_CAPACITY {
            context.series_outline_cache.insert(lookup(index), None);
        }
        let oldest_outline_lookup = lookup(0);
        assert!(context
            .series_outline_cache
            .get(&oldest_outline_lookup)
            .is_some());
        context
            .series_outline_cache
            .insert(lookup(SERIES_OUTLINE_CACHE_CAPACITY), None);
        assert_eq!(
            context.series_outline_cache.len(),
            SERIES_OUTLINE_CACHE_CAPACITY
        );
        assert!(!context
            .series_outline_cache
            .contains_key(&oldest_outline_lookup));
        assert!(context
            .series_outline_cache
            .contains_key(&lookup(SERIES_OUTLINE_CACHE_CAPACITY)));

        for index in 0..ARTWORK_RESULT_CACHE_CAPACITY {
            context
                .artwork_cache
                .insert(format!("artwork-{index}"), None);
        }
        assert!(context.artwork_cache.get("artwork-0").is_some());
        context
            .artwork_cache
            .insert(format!("artwork-{ARTWORK_RESULT_CACHE_CAPACITY}"), None);
        assert_eq!(context.artwork_cache.len(), ARTWORK_RESULT_CACHE_CAPACITY);
        assert!(!context.artwork_cache.contains_key("artwork-0"));
        assert!(context
            .artwork_cache
            .contains_key(format!("artwork-{ARTWORK_RESULT_CACHE_CAPACITY}").as_str()));
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

        let lookups = metadata_lookup_candidates_with_provider_item_id_hint(
            "series", &file, "en-US", None, None,
        );

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

        let lookups = metadata_lookup_candidates_with_provider_item_id_hint(
            "series", &file, "zh-CN", None, None,
        );

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

        let lookups = metadata_lookup_candidates_with_provider_item_id_hint(
            "series", &file, "zh-CN", None, None,
        );

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

        let lookups = metadata_lookup_candidates_with_provider_item_id_hint(
            "series", &file, "zh-CN", None, None,
        );

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
        let lookups = metadata_lookup_candidates_with_provider_item_id_hint(
            "series",
            &file,
            "zh-CN",
            Some(hint),
            None,
        );

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

        let lookups = metadata_lookup_candidates_with_provider_item_id_hint(
            "movie", &file, "zh-CN", None, None,
        );

        assert_eq!(lookups.len(), 3);
        assert_eq!(lookups[0].title, "Avatar： Fire and Ash");
        assert_eq!(lookups[0].year, Some(2025));
        assert_eq!(lookups[1].title, "Avatar: Fire and Ash");
        assert_eq!(lookups[1].year, Some(2025));
        assert_eq!(lookups[2].title, "阿凡达");
        assert_eq!(lookups[2].year, Some(2025));
    }

    #[test]
    fn metadata_lookup_candidates_use_only_existing_provider_item_id() {
        let mut file = build_discovered_episode();
        file.file_path = PathBuf::from("/media/movies/狂野时代 (2025)/狂野时代.2025.mp4");
        file.source_title = "狂野时代".to_string();
        file.year = Some(2025);
        file.season_number = None;
        file.episode_number = None;
        file.metadata_provider_item_id = Some("123_456".to_string());

        let lookups = metadata_lookup_candidates_with_provider_item_id_hint(
            "movie", &file, "zh-CN", None, None,
        );

        assert_eq!(lookups[0].title, "狂野时代");
        assert_eq!(lookups[0].year, Some(2025));
        assert_eq!(lookups[0].provider_item_id, Some("123_456".to_string()));
        assert_eq!(lookups.len(), 1);
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

        let lookups = metadata_lookup_candidates_with_provider_item_id_hint(
            "movie", &file, "zh-CN", None, None,
        );

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
        file.metadata_provider_item_id = Some("77".to_string());
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
        file.metadata_provider_item_id = Some("83533".to_string());
        assert!(!needs_remote_metadata(&file));

        file.metadata_provider = None;
        assert!(needs_remote_metadata(&file));
    }

    #[test]
    fn needs_remote_title_refresh_detects_local_year_display_title() {
        let mut file = build_discovered_episode();
        file.metadata_provider_item_id = Some("259909".to_string());
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
            1,
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

        let outcome = context
            .enrich_group_with_progress("series", &mut files, None, |_, _| {})
            .await
            .expect("group metadata enrichment should succeed");

        assert!(outcome.remote_lookup_performed);
        assert!(outcome.remote_metadata_applied);
        assert_eq!(provider.lookup_count.load(Ordering::SeqCst), 1);
        assert!(files.iter().all(|file| file.title == "诉讼女王"));
        assert!(files
            .iter()
            .all(|file| file.original_title.as_deref() == Some("All's Fair")));
        assert!(files
            .iter()
            .all(|file| file.metadata_provider_item_id.as_deref() == Some("259909")));
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
    async fn partial_series_outline_enriches_scan_without_renewing_retention() {
        let provider = Arc::new(PartialOutlineMetadataProvider {
            complete_lookup_count: AtomicUsize::new(0),
            partial_lookup_count: AtomicUsize::new(0),
        });
        let provider_for_context: Arc<dyn MetadataProvider> = provider.clone();
        let mut context = MetadataEnrichmentContext::new(
            std::env::temp_dir().join("mova-partial-outline-scan-test"),
            1,
            provider_for_context,
            "zh-CN".to_string(),
        );
        let mut files = vec![build_discovered_episode()];

        let outcome = context
            .enrich_group_with_progress("series", &mut files, None, |_, _| {})
            .await
            .expect("a partial outline must not fail normal scan enrichment");

        assert!(outcome.remote_metadata_applied);
        assert!(outcome.tmdb_remote_snapshot_json.is_some());
        assert!(!outcome.tmdb_remote_snapshot_renews_retention);
        assert_eq!(
            files[0].season_title.as_deref(),
            Some("Partially available season")
        );
        assert_eq!(
            files[0].episode_title.as_deref(),
            Some("Partially available episode")
        );
        assert_eq!(provider.complete_lookup_count.load(Ordering::SeqCst), 1);
        assert_eq!(provider.partial_lookup_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn enrich_group_uses_container_tmdb_hint_without_title_search_fallback() {
        let provider = Arc::new(RecordingMetadataProvider {
            lookups: Mutex::new(Vec::new()),
            result: Some(RemoteMetadata {
                provider_item_id: Some("324552".to_string()),
                title: Some("疾速追杀2".to_string()),
                original_title: Some("John Wick: Chapter 2".to_string()),
                year: Some(2017),
                ..RemoteMetadata::default()
            }),
        });
        let provider_for_context: Arc<dyn MetadataProvider> = provider.clone();
        let mut context = MetadataEnrichmentContext::new(
            std::env::temp_dir().join("mova-container-tmdb-hint-test"),
            1,
            provider_for_context,
            "zh-CN".to_string(),
        );
        let mut file = build_discovered_episode();
        file.file_path = PathBuf::from("/media/movies/疾速追杀2 {tmdb-324552}/2017.2160p.mkv");
        file.title = "疾速追杀2".to_string();
        file.source_title = "疾速追杀2".to_string();
        file.year = Some(2017);
        file.season_number = None;
        file.episode_number = None;
        file.episode_title = None;
        let mut files = vec![file];

        let outcome = context
            .enrich_group_with_lookup_hint_and_progress(
                "movie",
                &mut files,
                None,
                Some("324552"),
                |_, _| {},
            )
            .await
            .expect("direct TMDB lookup should succeed");

        let lookups = provider.lookups.lock().expect("lookup lock");
        assert_eq!(lookups.len(), 1);
        assert_eq!(lookups[0].provider_item_id.as_deref(), Some("324552"));
        assert!(outcome.remote_lookup_performed);
        assert!(outcome.remote_metadata_applied);
        assert_eq!(files[0].metadata_provider.as_deref(), Some("tmdb"));
        assert_eq!(
            files[0].metadata_provider_item_id.as_deref(),
            Some("324552")
        );
    }

    #[tokio::test]
    async fn missing_container_tmdb_hint_does_not_fall_back_to_title_search() {
        let provider = Arc::new(RecordingMetadataProvider {
            lookups: Mutex::new(Vec::new()),
            result: None,
        });
        let provider_for_context: Arc<dyn MetadataProvider> = provider.clone();
        let mut context = MetadataEnrichmentContext::new(
            std::env::temp_dir().join("mova-missing-container-tmdb-hint-test"),
            1,
            provider_for_context,
            "zh-CN".to_string(),
        );
        let mut file = build_discovered_episode();
        file.season_number = None;
        file.episode_number = None;
        file.episode_title = None;
        let mut files = vec![file];

        let outcome = context
            .enrich_group_with_lookup_hint_and_progress(
                "movie",
                &mut files,
                None,
                Some("999999"),
                |_, _| {},
            )
            .await
            .expect("missing direct TMDB lookup should be a normal no-match");

        let lookups = provider.lookups.lock().expect("lookup lock");
        assert_eq!(lookups.len(), 1);
        assert_eq!(lookups[0].provider_item_id.as_deref(), Some("999999"));
        assert!(outcome.remote_lookup_performed);
        assert!(!outcome.remote_metadata_applied);
        assert_eq!(files[0].metadata_provider_item_id, None);
    }

    #[tokio::test]
    async fn provider_id_cache_key_deduplicates_equivalent_remote_requests() {
        let provider = Arc::new(CountingMetadataProvider {
            enabled: true,
            lookup_count: AtomicUsize::new(0),
        });
        let provider_for_context: Arc<dyn MetadataProvider> = provider.clone();
        let mut context = MetadataEnrichmentContext::new(
            std::env::temp_dir().join("mova-provider-id-cache-test"),
            1,
            provider_for_context,
            "zh-CN".to_string(),
        );
        let first = MetadataLookup {
            title: "Local title A".to_string(),
            year: Some(2024),
            season_air_year: None,
            library_type: "SERIES".to_string(),
            language: Some("zh-CN".to_string()),
            provider_item_id: Some(" 259909 ".to_string()),
        };
        let second = MetadataLookup {
            title: "A translated title".to_string(),
            year: None,
            season_air_year: Some(MetadataSeasonAirYearHint {
                season_number: 2,
                year: 2025,
            }),
            library_type: "series".to_string(),
            language: Some("ZH-cn".to_string()),
            provider_item_id: Some("259909".to_string()),
        };

        context.lookup_remote_metadata_cached(&first).await.unwrap();
        context
            .lookup_remote_metadata_cached(&second)
            .await
            .unwrap();

        assert_eq!(provider.lookup_count.load(Ordering::SeqCst), 1);
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
            1,
            provider_for_context,
            "zh-CN".to_string(),
        );
        let mut file = build_discovered_episode();
        file.title = "Local Series".to_string();
        file.source_title = "Local Series".to_string();
        file.overview = Some("Local overview".to_string());
        let mut files = vec![file];

        let outcome = context
            .enrich_group_with_progress("series", &mut files, None, |_, _| {})
            .await
            .expect("disabled provider should not block local enrichment");

        assert!(!outcome.remote_lookup_performed);
        assert!(!outcome.remote_metadata_applied);
        assert_eq!(provider.lookup_count.load(Ordering::SeqCst), 0);
        assert_eq!(files[0].title, "Local Series");
        assert_eq!(files[0].overview.as_deref(), Some("Local overview"));
        assert_eq!(files[0].metadata_provider_item_id, None);
    }

    #[tokio::test]
    async fn enrich_group_reports_non_authoritative_when_complete_metadata_skips_lookup() {
        let provider = Arc::new(CountingMetadataProvider {
            enabled: true,
            lookup_count: AtomicUsize::new(0),
        });
        let provider_for_context: Arc<dyn MetadataProvider> = provider.clone();
        let mut context = MetadataEnrichmentContext::new(
            std::env::temp_dir().join("mova-test-complete-artwork-cache"),
            1,
            provider_for_context,
            "zh-CN".to_string(),
        );
        let mut file = build_discovered_episode();
        file.metadata_provider = Some("tmdb".to_string());
        file.metadata_provider_item_id = Some("259909".to_string());
        file.title = "All's Fair".to_string();
        file.source_title = "All's Fair".to_string();
        file.original_title = Some("All's Fair".to_string());
        file.year = Some(2025);
        file.overview = Some("Complete overview".to_string());
        file.poster_path = Some("/cache/episode-poster.jpg".to_string());
        file.backdrop_path = Some("/cache/episode-backdrop.jpg".to_string());
        file.series_poster_path = Some("/cache/series-poster.jpg".to_string());
        file.series_backdrop_path = Some("/cache/series-backdrop.jpg".to_string());
        file.season_poster_path = Some("/cache/season-poster.jpg".to_string());
        file.season_backdrop_path = Some("/cache/season-backdrop.jpg".to_string());
        let mut files = vec![file];

        let outcome = context
            .enrich_group_with_progress("series", &mut files, None, |_, _| {})
            .await
            .expect("complete metadata should pass through without a lookup");

        assert_eq!(provider.lookup_count.load(Ordering::SeqCst), 0);
        assert!(!outcome.remote_lookup_performed);
        assert!(!outcome.remote_metadata_applied);
    }

    #[tokio::test]
    async fn manual_refresh_forces_direct_lookup_for_a_complete_bound_item() {
        let provider = Arc::new(CountingMetadataProvider {
            enabled: true,
            lookup_count: AtomicUsize::new(0),
        });
        let provider_for_context: Arc<dyn MetadataProvider> = provider.clone();
        let mut context = MetadataEnrichmentContext::new(
            std::env::temp_dir().join("mova-test-manual-refresh-artwork-cache"),
            1,
            provider_for_context,
            "zh-CN".to_string(),
        );
        let mut file = build_discovered_episode();
        file.metadata_provider = Some("tmdb".to_string());
        file.metadata_provider_item_id = Some("259909".to_string());
        file.title = "All's Fair".to_string();
        file.source_title = "All's Fair".to_string();
        file.original_title = Some("All's Fair".to_string());
        file.year = Some(2025);
        file.overview = Some("Complete overview".to_string());
        file.poster_path = Some("/cache/episode-poster.jpg".to_string());
        file.backdrop_path = Some("/cache/episode-backdrop.jpg".to_string());
        file.series_poster_path = Some("/cache/series-poster.jpg".to_string());
        file.series_backdrop_path = Some("/cache/series-backdrop.jpg".to_string());
        file.season_poster_path = Some("/cache/season-poster.jpg".to_string());
        file.season_backdrop_path = Some("/cache/season-backdrop.jpg".to_string());
        let mut files = vec![file];

        let outcome = context
            .refresh_group_with_lookup_hint_and_progress(
                "series",
                &mut files,
                None,
                Some("259909"),
                |_, _| {},
            )
            .await
            .expect("manual refresh should re-read an accepted direct identity");

        assert_eq!(provider.lookup_count.load(Ordering::SeqCst), 1);
        assert!(outcome.remote_lookup_performed);
        assert!(outcome.remote_metadata_applied);
        assert_eq!(files[0].title, "诉讼女王");
    }

    #[tokio::test]
    async fn enrich_episode_artwork_keeps_remote_season_artwork() {
        let provider: Arc<dyn MetadataProvider> = Arc::new(SeasonArtworkProvider);
        let mut context = MetadataEnrichmentContext::new(
            std::env::temp_dir().join("mova-test-artwork-cache"),
            1,
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
            1,
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
                provider_item_id: Some("259909".to_string()),
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
    struct PartialOutlineMetadataProvider {
        complete_lookup_count: AtomicUsize,
        partial_lookup_count: AtomicUsize,
    }

    #[async_trait]
    impl MetadataProvider for PartialOutlineMetadataProvider {
        async fn lookup(&self, _lookup: &MetadataLookup) -> anyhow::Result<Option<RemoteMetadata>> {
            Ok(Some(RemoteMetadata {
                provider_item_id: Some("123".to_string()),
                title: Some("Show".to_string()),
                ..RemoteMetadata::default()
            }))
        }

        async fn lookup_series_episode_outline(
            &self,
            _lookup: &MetadataLookup,
        ) -> anyhow::Result<Option<RemoteSeriesEpisodeOutline>> {
            self.partial_lookup_count.fetch_add(1, Ordering::SeqCst);
            Ok(Some(RemoteSeriesEpisodeOutline {
                seasons: vec![RemoteSeriesSeason {
                    season_number: 1,
                    title: Some("Partially available season".to_string()),
                    episodes: vec![RemoteSeriesEpisode {
                        episode_number: 1,
                        title: Some("Partially available episode".to_string()),
                        ..RemoteSeriesEpisode::default()
                    }],
                    ..RemoteSeriesSeason::default()
                }],
            }))
        }

        async fn lookup_complete_series_episode_outline(
            &self,
            _lookup: &MetadataLookup,
        ) -> anyhow::Result<Option<RemoteSeriesEpisodeOutline>> {
            self.complete_lookup_count.fetch_add(1, Ordering::SeqCst);
            anyhow::bail!("one TMDB season request failed")
        }
    }

    #[derive(Debug)]
    struct RecordingMetadataProvider {
        lookups: Mutex<Vec<MetadataLookup>>,
        result: Option<RemoteMetadata>,
    }

    #[async_trait]
    impl MetadataProvider for RecordingMetadataProvider {
        async fn lookup(&self, lookup: &MetadataLookup) -> anyhow::Result<Option<RemoteMetadata>> {
            self.lookups
                .lock()
                .expect("lookup lock")
                .push(lookup.clone());
            Ok(self.result.clone())
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
            provider_item_id: Some("123".to_string()),
        }
    }

    fn build_discovered_episode() -> DiscoveredMediaFile {
        DiscoveredMediaFile {
            file_path: PathBuf::from("/media/series/Show/Season 01/Show.S01E01.mkv"),
            file_modified_at_ms: Some(1_700_000_000_000),
            sidecar_fingerprint: String::new(),
            probe_error: None,
            metadata_provider: None,
            metadata_provider_item_id: None,
            title: "Show".to_string(),
            source_title: "Show".to_string(),
            original_title: None,
            sort_title: None,
            tagline: None,
            premiere_date: None,
            content_rating: None,
            series_sidecar_title: None,
            series_sidecar_year: None,
            local_nfo: None,
            series_local_nfo: None,
            invalid_local_nfo_source_path: None,
            invalid_series_local_nfo_source_path: None,
            local_nfo_is_selected: false,
            series_local_nfo_is_selected: false,
            removed_local_nfo_source_path: None,
            removed_series_local_nfo_source_path: None,
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
            episode_original_title: None,
            episode_sort_title: None,
            episode_year: None,
            episode_overview: None,
            episode_tagline: None,
            episode_premiere_date: None,
            episode_content_rating: None,
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
