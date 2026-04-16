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
        let lookup = MetadataLookup {
            // 元数据匹配应优先使用文件名解析出的原始标题，而不是已经被远端覆盖过的展示标题。
            title: file.source_title.clone(),
            year: file.year,
            library_type: lookup_type.to_string(),
            language: Some(self.metadata_language.clone()),
            provider_item_id: None,
        };

        on_progress(MetadataEnrichmentStage::Metadata, file);
        let mut resolved_remote_metadata = None;

        if self.metadata_provider.is_enabled() && needs_remote_metadata(file) {
            let metadata = self.lookup_remote_metadata_cached(&lookup).await;
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
            self.enrich_episode_like_artwork(&lookup, file, resolved_remote_metadata.is_some())
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
        Some(path) => is_external_url(path) || is_generic_path(path),
    }
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

#[cfg(test)]
mod tests {
    use super::{
        artwork_file_extension, build_artwork_cache_path, is_generic_backdrop_artwork_path,
        is_generic_poster_artwork_path, should_replace_episode_artwork, stable_artwork_cache_key,
        MetadataEnrichmentContext,
    };
    use crate::NullMetadataProvider;
    use mova_scan::DiscoveredMediaFile;
    use std::path::Path;
    use std::{path::PathBuf, sync::Arc};

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
        assert!(!should_replace_episode_artwork(
            Some("/media/Season 01/E01-poster.jpg"),
            is_generic_poster_artwork_path
        ));
    }

    fn build_discovered_episode() -> DiscoveredMediaFile {
        DiscoveredMediaFile {
            file_path: PathBuf::from("/media/series/Show/Season 01/Show.S01E01.mkv"),
            metadata_provider: None,
            metadata_provider_item_id: None,
            title: "Show".to_string(),
            source_title: "Show".to_string(),
            original_title: None,
            sort_title: None,
            year: Some(2024),
            imdb_rating: None,
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
            audio_tracks: Vec::new(),
            subtitle_tracks: Vec::new(),
        }
    }
}
