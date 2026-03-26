use async_trait::async_trait;
use reqwest::{
    header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION},
    Client,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};

pub const DEFAULT_TMDB_LANGUAGE: &str = "zh-CN";
pub const SUPPORTED_TMDB_LANGUAGES: &[&str] = &["zh-CN", "en-US"];
pub const DEFAULT_TMDB_API_BASE_URL: &str = "https://api.themoviedb.org/3";
pub const DEFAULT_TMDB_IMAGE_BASE_URL: &str = "https://image.tmdb.org/t/p/original";

/// 服务启动时解析出的元数据 provider 配置。
#[derive(Debug, Clone)]
pub enum MetadataProviderConfig {
    Disabled,
    Tmdb(TmdbMetadataProviderConfig),
}

/// 元数据查询入参。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MetadataLookup {
    pub title: String,
    pub year: Option<i32>,
    pub library_type: String,
    pub language: Option<String>,
    pub provider_item_id: Option<i64>,
}

/// 第三方元数据源返回的统一结构。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RemoteMetadata {
    pub title: Option<String>,
    pub original_title: Option<String>,
    pub year: Option<i32>,
    pub overview: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RemoteCastMember {
    pub person_id: Option<i64>,
    pub sort_order: i32,
    pub name: String,
    pub character_name: Option<String>,
    pub profile_path: Option<String>,
}

/// 手动匹配元数据时返回的候选条目。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RemoteMetadataSearchResult {
    pub provider_item_id: i64,
    pub title: String,
    pub original_title: Option<String>,
    pub year: Option<i32>,
    pub overview: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
}

/// 第三方元数据源返回的剧集季/集大纲结构。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteSeriesEpisodeOutline {
    pub seasons: Vec<RemoteSeriesSeason>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteSeriesSeason {
    pub season_number: i32,
    pub title: Option<String>,
    pub year: Option<i32>,
    pub overview: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub episodes: Vec<RemoteSeriesEpisode>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteSeriesEpisode {
    pub episode_number: i32,
    pub title: Option<String>,
    pub overview: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
}

/// 不同元数据源共享的抽象接口。
#[async_trait]
pub trait MetadataProvider: Send + Sync {
    fn is_enabled(&self) -> bool {
        true
    }

    async fn lookup(&self, lookup: &MetadataLookup) -> anyhow::Result<Option<RemoteMetadata>>;

    async fn lookup_series_episode_outline(
        &self,
        _lookup: &MetadataLookup,
    ) -> anyhow::Result<Option<RemoteSeriesEpisodeOutline>> {
        Ok(None)
    }

    async fn lookup_cast(
        &self,
        _lookup: &MetadataLookup,
    ) -> anyhow::Result<Option<Vec<RemoteCastMember>>> {
        Ok(None)
    }

    async fn search(
        &self,
        _lookup: &MetadataLookup,
    ) -> anyhow::Result<Vec<RemoteMetadataSearchResult>> {
        Ok(Vec::new())
    }
}

/// 未配置第三方元数据时使用的空实现。
#[derive(Debug, Default)]
pub struct NullMetadataProvider;

#[async_trait]
impl MetadataProvider for NullMetadataProvider {
    fn is_enabled(&self) -> bool {
        false
    }

    async fn lookup(&self, _lookup: &MetadataLookup) -> anyhow::Result<Option<RemoteMetadata>> {
        Ok(None)
    }

    async fn search(
        &self,
        _lookup: &MetadataLookup,
    ) -> anyhow::Result<Vec<RemoteMetadataSearchResult>> {
        Ok(Vec::new())
    }
}

/// 根据启动配置构造可用的元数据 provider。
pub fn build_metadata_provider(
    config: MetadataProviderConfig,
) -> anyhow::Result<Arc<dyn MetadataProvider>> {
    match config {
        MetadataProviderConfig::Disabled => Ok(Arc::new(NullMetadataProvider)),
        MetadataProviderConfig::Tmdb(config) => Ok(Arc::new(TmdbMetadataProvider::new(config)?)),
    }
}

/// TMDB provider 的运行时配置。
#[derive(Debug, Clone)]
pub struct TmdbMetadataProviderConfig {
    pub access_token: String,
    pub language: String,
    pub api_base_url: String,
    pub image_base_url: String,
}

/// 基于 TMDB 的电影/剧集元数据 provider。
#[derive(Clone)]
pub struct TmdbMetadataProvider {
    client: Client,
    language: String,
    api_base_url: String,
    image_base_url: String,
}

impl TmdbMetadataProvider {
    pub fn new(config: TmdbMetadataProviderConfig) -> anyhow::Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", config.access_token.trim()))?,
        );

        let client = Client::builder()
            .default_headers(headers)
            .user_agent(format!("mova/{}", env!("CARGO_PKG_VERSION")))
            .build()?;

        Ok(Self {
            client,
            language: config.language.trim().to_string(),
            api_base_url: config.api_base_url.trim_end_matches('/').to_string(),
            image_base_url: config.image_base_url.trim_end_matches('/').to_string(),
        })
    }

    async fn lookup_movie(
        &self,
        lookup: &MetadataLookup,
    ) -> anyhow::Result<Option<RemoteMetadata>> {
        let request_language = self.request_language(lookup);
        let movie_id = match lookup.provider_item_id {
            Some(movie_id) => movie_id,
            None => {
                let response = self.search_movie_response(lookup).await?;
                let Some(best_match) =
                    select_best_match(&lookup.title, lookup.year, &response.results)
                else {
                    return Ok(None);
                };

                best_match.id
            }
        };

        let details = self.fetch_movie_details(movie_id, request_language).await?;
        Ok(Some(self.map_movie_details(details)))
    }

    async fn search_movie_response(
        &self,
        lookup: &MetadataLookup,
    ) -> anyhow::Result<TmdbSearchResponse<TmdbMovieSearchResult>> {
        let request_language = self.request_language(lookup);
        let mut query = vec![
            ("query", lookup.title.clone()),
            ("include_adult", "false".to_string()),
            ("page", "1".to_string()),
            ("language", request_language.to_string()),
        ];

        if let Some(year) = lookup.year {
            query.push(("year", year.to_string()));
        }

        let response = self
            .client
            .get(format!("{}/search/movie", self.api_base_url))
            .query(&query)
            .send()
            .await?
            .error_for_status()?
            .json::<TmdbSearchResponse<TmdbMovieSearchResult>>()
            .await?;

        Ok(response)
    }

    async fn search_tv_best_match(
        &self,
        lookup: &MetadataLookup,
    ) -> anyhow::Result<Option<TmdbTvSearchResult>> {
        let response = self.search_tv_response(lookup).await?;

        let best_match = select_best_match(&lookup.title, lookup.year, &response.results).cloned();

        Ok(best_match)
    }

    async fn search_tv_response(
        &self,
        lookup: &MetadataLookup,
    ) -> anyhow::Result<TmdbSearchResponse<TmdbTvSearchResult>> {
        let request_language = self.request_language(lookup);
        let mut query = vec![
            ("query", lookup.title.clone()),
            ("include_adult", "false".to_string()),
            ("page", "1".to_string()),
            ("language", request_language.to_string()),
        ];

        if let Some(year) = lookup.year {
            query.push(("first_air_date_year", year.to_string()));
        }

        let response = self
            .client
            .get(format!("{}/search/tv", self.api_base_url))
            .query(&query)
            .send()
            .await?
            .error_for_status()?
            .json::<TmdbSearchResponse<TmdbTvSearchResult>>()
            .await?;

        Ok(response)
    }

    async fn fetch_movie_details(
        &self,
        movie_id: i64,
        language: &str,
    ) -> anyhow::Result<TmdbMovieDetails> {
        let details = self
            .client
            .get(format!("{}/movie/{}", self.api_base_url, movie_id))
            .query(&[("language", language)])
            .send()
            .await?
            .error_for_status()?
            .json::<TmdbMovieDetails>()
            .await?;

        Ok(details)
    }

    async fn fetch_tv_details(&self, tv_id: i64, language: &str) -> anyhow::Result<TmdbTvDetails> {
        let details = self
            .client
            .get(format!("{}/tv/{}", self.api_base_url, tv_id))
            .query(&[("language", language)])
            .send()
            .await?
            .error_for_status()?
            .json::<TmdbTvDetails>()
            .await?;

        Ok(details)
    }

    async fn fetch_tv_season_details(
        &self,
        tv_id: i64,
        season_number: i32,
        language: &str,
    ) -> anyhow::Result<TmdbTvSeasonDetails> {
        let season_details = self
            .client
            .get(format!(
                "{}/tv/{}/season/{}",
                self.api_base_url, tv_id, season_number
            ))
            .query(&[("language", language)])
            .send()
            .await?
            .error_for_status()?
            .json::<TmdbTvSeasonDetails>()
            .await?;

        Ok(season_details)
    }

    async fn fetch_movie_credits(
        &self,
        movie_id: i64,
        language: &str,
    ) -> anyhow::Result<TmdbCreditsResponse> {
        let credits = self
            .client
            .get(format!("{}/movie/{}/credits", self.api_base_url, movie_id))
            .query(&[("language", language)])
            .send()
            .await?
            .error_for_status()?
            .json::<TmdbCreditsResponse>()
            .await?;

        Ok(credits)
    }

    async fn fetch_tv_aggregate_credits(
        &self,
        tv_id: i64,
        language: &str,
    ) -> anyhow::Result<TmdbTvAggregateCreditsResponse> {
        let credits = self
            .client
            .get(format!(
                "{}/tv/{}/aggregate_credits",
                self.api_base_url, tv_id
            ))
            .query(&[("language", language)])
            .send()
            .await?
            .error_for_status()?
            .json::<TmdbTvAggregateCreditsResponse>()
            .await?;

        Ok(credits)
    }

    async fn lookup_tv(&self, lookup: &MetadataLookup) -> anyhow::Result<Option<RemoteMetadata>> {
        let tv_id = match lookup.provider_item_id {
            Some(tv_id) => tv_id,
            None => {
                let Some(best_match) = self.search_tv_best_match(lookup).await? else {
                    return Ok(None);
                };

                best_match.id
            }
        };

        let details = self
            .fetch_tv_details(tv_id, self.request_language(lookup))
            .await?;

        Ok(Some(RemoteMetadata {
            title: empty_to_none(details.name),
            original_title: empty_to_none(details.original_name),
            year: parse_year(details.first_air_date.as_deref()),
            overview: empty_to_none(details.overview),
            poster_path: details
                .poster_path
                .as_deref()
                .map(|path| self.build_image_url(path)),
            backdrop_path: details
                .backdrop_path
                .as_deref()
                .map(|path| self.build_image_url(path)),
        }))
    }

    async fn lookup_tv_episode_outline(
        &self,
        lookup: &MetadataLookup,
    ) -> anyhow::Result<Option<RemoteSeriesEpisodeOutline>> {
        let tv_id = match lookup.provider_item_id {
            Some(tv_id) => tv_id,
            None => {
                let Some(best_match) = self.search_tv_best_match(lookup).await? else {
                    return Ok(None);
                };

                best_match.id
            }
        };

        let request_language = self.request_language(lookup);
        let details = self.fetch_tv_details(tv_id, request_language).await?;
        let mut seasons = Vec::new();

        for season in details
            .seasons
            .into_iter()
            .filter(|season| season.season_number >= 1)
        {
            let season_details = match self
                .fetch_tv_season_details(tv_id, season.season_number, request_language)
                .await
            {
                Ok(season_details) => season_details,
                Err(error) => {
                    tracing::warn!(
                        tv_id,
                        season_number = season.season_number,
                        error = ?error,
                        "failed to fetch tmdb season details"
                    );
                    continue;
                }
            };

            let mut episodes = season_details
                .episodes
                .into_iter()
                .filter(|episode| episode.episode_number >= 1)
                .map(|episode| {
                    let still_path = episode
                        .still_path
                        .as_deref()
                        .map(|path| self.build_image_url(path));

                    RemoteSeriesEpisode {
                        episode_number: episode.episode_number,
                        title: empty_to_none(episode.name),
                        overview: empty_to_none(episode.overview),
                        poster_path: still_path.clone(),
                        backdrop_path: still_path,
                    }
                })
                .collect::<Vec<_>>();
            episodes.sort_by_key(|episode| episode.episode_number);

            let season_poster_path = season_details
                .poster_path
                .as_deref()
                .or(season.poster_path.as_deref())
                .map(|path| self.build_image_url(path));

            seasons.push(RemoteSeriesSeason {
                season_number: season.season_number,
                title: empty_to_none(season_details.name).or_else(|| empty_to_none(season.name)),
                year: parse_year(
                    season_details
                        .air_date
                        .as_deref()
                        .or(season.air_date.as_deref()),
                ),
                overview: empty_to_none(season_details.overview),
                poster_path: season_poster_path,
                backdrop_path: None,
                episodes,
            });
        }

        seasons.sort_by_key(|season| season.season_number);

        Ok(Some(RemoteSeriesEpisodeOutline { seasons }))
    }

    async fn lookup_movie_cast(
        &self,
        lookup: &MetadataLookup,
    ) -> anyhow::Result<Option<Vec<RemoteCastMember>>> {
        let request_language = self.request_language(lookup);
        let movie_id = match lookup.provider_item_id {
            Some(movie_id) => movie_id,
            None => {
                let response = self.search_movie_response(lookup).await?;
                let Some(best_match) =
                    select_best_match(&lookup.title, lookup.year, &response.results)
                else {
                    return Ok(None);
                };

                best_match.id
            }
        };

        let credits = self.fetch_movie_credits(movie_id, request_language).await?;
        Ok(Some(
            credits
                .cast
                .into_iter()
                .enumerate()
                .filter_map(|(index, cast)| {
                    let name = empty_to_none(cast.name)?;

                    Some(RemoteCastMember {
                        person_id: Some(cast.id),
                        sort_order: cast
                            .order
                            .unwrap_or_else(|| i32::try_from(index).unwrap_or(i32::MAX)),
                        name,
                        character_name: empty_to_none(cast.character),
                        profile_path: cast
                            .profile_path
                            .as_deref()
                            .map(|path| self.build_image_url(path)),
                    })
                })
                .collect(),
        ))
    }

    async fn lookup_tv_cast(
        &self,
        lookup: &MetadataLookup,
    ) -> anyhow::Result<Option<Vec<RemoteCastMember>>> {
        let tv_id = match lookup.provider_item_id {
            Some(tv_id) => tv_id,
            None => {
                let Some(best_match) = self.search_tv_best_match(lookup).await? else {
                    return Ok(None);
                };

                best_match.id
            }
        };

        let credits = self
            .fetch_tv_aggregate_credits(tv_id, self.request_language(lookup))
            .await?;
        Ok(Some(
            credits
                .cast
                .into_iter()
                .enumerate()
                .filter_map(|(index, cast)| {
                    let name = empty_to_none(cast.name)?;

                    Some(RemoteCastMember {
                        person_id: Some(cast.id),
                        sort_order: cast
                            .order
                            .unwrap_or_else(|| i32::try_from(index).unwrap_or(i32::MAX)),
                        name,
                        character_name: pick_primary_character_name(&cast.roles),
                        profile_path: cast
                            .profile_path
                            .as_deref()
                            .map(|path| self.build_image_url(path)),
                    })
                })
                .collect(),
        ))
    }

    fn build_image_url(&self, path: &str) -> String {
        if path.starts_with('/') {
            format!("{}{}", self.image_base_url, path)
        } else {
            format!("{}/{}", self.image_base_url, path)
        }
    }

    fn map_movie_details(&self, details: TmdbMovieDetails) -> RemoteMetadata {
        RemoteMetadata {
            title: empty_to_none(details.title),
            original_title: empty_to_none(details.original_title),
            year: parse_year(details.release_date.as_deref()),
            overview: empty_to_none(details.overview),
            poster_path: details
                .poster_path
                .as_deref()
                .map(|path| self.build_image_url(path)),
            backdrop_path: details
                .backdrop_path
                .as_deref()
                .map(|path| self.build_image_url(path)),
        }
    }

    fn map_search_result(
        &self,
        provider_item_id: i64,
        title: Option<String>,
        original_title: Option<String>,
        year: Option<i32>,
        overview: Option<String>,
        poster_path: Option<String>,
        backdrop_path: Option<String>,
    ) -> Option<RemoteMetadataSearchResult> {
        let title = empty_to_none(title).or_else(|| empty_to_none(original_title.clone()))?;

        Some(RemoteMetadataSearchResult {
            provider_item_id,
            title,
            original_title: empty_to_none(original_title),
            year,
            overview: empty_to_none(overview),
            poster_path: poster_path
                .as_deref()
                .map(|path| self.build_image_url(path)),
            backdrop_path: backdrop_path
                .as_deref()
                .map(|path| self.build_image_url(path)),
        })
    }

    /// provider 本身保留一个默认语言，但具体请求允许按媒体库覆写。
    /// 这样同一个 TMDB token 可以服务多个不同语言的媒体库。
    fn request_language<'a>(&'a self, lookup: &'a MetadataLookup) -> &'a str {
        lookup.language.as_deref().unwrap_or(self.language.as_str())
    }
}

#[async_trait]
impl MetadataProvider for TmdbMetadataProvider {
    async fn lookup(&self, lookup: &MetadataLookup) -> anyhow::Result<Option<RemoteMetadata>> {
        if lookup.title.trim().is_empty() && lookup.provider_item_id.is_none() {
            return Ok(None);
        }

        if lookup.library_type.eq_ignore_ascii_case("series") {
            self.lookup_tv(lookup).await
        } else {
            self.lookup_movie(lookup).await
        }
    }

    async fn lookup_series_episode_outline(
        &self,
        lookup: &MetadataLookup,
    ) -> anyhow::Result<Option<RemoteSeriesEpisodeOutline>> {
        if (lookup.title.trim().is_empty() && lookup.provider_item_id.is_none())
            || !lookup.library_type.eq_ignore_ascii_case("series")
        {
            return Ok(None);
        }

        self.lookup_tv_episode_outline(lookup).await
    }

    async fn lookup_cast(
        &self,
        lookup: &MetadataLookup,
    ) -> anyhow::Result<Option<Vec<RemoteCastMember>>> {
        if lookup.title.trim().is_empty() && lookup.provider_item_id.is_none() {
            return Ok(None);
        }

        if lookup.library_type.eq_ignore_ascii_case("series") {
            self.lookup_tv_cast(lookup).await
        } else {
            self.lookup_movie_cast(lookup).await
        }
    }

    async fn search(
        &self,
        lookup: &MetadataLookup,
    ) -> anyhow::Result<Vec<RemoteMetadataSearchResult>> {
        if lookup.title.trim().is_empty() {
            return Ok(Vec::new());
        }

        if lookup.library_type.eq_ignore_ascii_case("series") {
            let response = self.search_tv_response(lookup).await?;
            return Ok(response
                .results
                .into_iter()
                .filter_map(|result| {
                    self.map_search_result(
                        result.id,
                        result.name,
                        result.original_name,
                        parse_year(result.first_air_date.as_deref()),
                        result.overview,
                        result.poster_path,
                        result.backdrop_path,
                    )
                })
                .collect());
        }

        let response = self.search_movie_response(lookup).await?;
        Ok(response
            .results
            .into_iter()
            .filter_map(|result| {
                self.map_search_result(
                    result.id,
                    result.title,
                    result.original_title,
                    parse_year(result.release_date.as_deref()),
                    result.overview,
                    result.poster_path,
                    result.backdrop_path,
                )
            })
            .collect())
    }
}

/// 把远程元数据补到本地扫描结果里。
/// 展示标题会优先使用远端返回的本地化标题；原始文件名标题则单独存到 `source_title`。
pub fn apply_remote_metadata(
    metadata: Option<RemoteMetadata>,
    title: &mut String,
    original_title: &mut Option<String>,
    year: &mut Option<i32>,
    overview: &mut Option<String>,
    poster_path: &mut Option<String>,
    backdrop_path: &mut Option<String>,
) {
    let Some(metadata) = metadata else {
        return;
    };

    if let Some(remote_title) = metadata.title {
        *title = remote_title;
    }

    if original_title.is_none() {
        *original_title = metadata.original_title;
    }

    if year.is_none() {
        *year = metadata.year;
    }

    if overview.is_none() {
        *overview = metadata.overview;
    }

    if poster_path.is_none() {
        *poster_path = metadata.poster_path;
    }

    if backdrop_path.is_none() {
        *backdrop_path = metadata.backdrop_path;
    }
}

pub fn normalize_optional_value(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

pub fn normalize_required_value(field_name: &str, value: String) -> anyhow::Result<String> {
    let value = value.trim().to_string();
    if value.is_empty() {
        anyhow::bail!("{} cannot be empty", field_name);
    }

    Ok(value)
}

pub fn normalize_base_url(field_name: &str, value: String) -> anyhow::Result<String> {
    Ok(normalize_required_value(field_name, value)?
        .trim_end_matches('/')
        .to_string())
}

/// 媒体库的 TMDB 语言目前只开放有限选项，避免扫库后混入不可预期的本地化结果。
pub fn normalize_metadata_language(
    value: Option<String>,
    default_language: &str,
) -> anyhow::Result<String> {
    let normalized = value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default_language.to_string());

    if let Some(supported) = SUPPORTED_TMDB_LANGUAGES
        .iter()
        .find(|supported| supported.eq_ignore_ascii_case(&normalized))
    {
        return Ok((*supported).to_string());
    }

    anyhow::bail!(
        "metadata language must be one of: {}",
        SUPPORTED_TMDB_LANGUAGES.join(", ")
    );
}

trait TmdbSearchCandidate {
    fn candidate_title(&self) -> Option<&str>;
    fn candidate_original_title(&self) -> Option<&str>;
    fn candidate_year(&self) -> Option<i32>;
    fn candidate_popularity(&self) -> f64;
}

#[derive(Debug, Deserialize)]
struct TmdbSearchResponse<T> {
    results: Vec<T>,
}

#[derive(Debug, Deserialize)]
struct TmdbMovieSearchResult {
    id: i64,
    title: Option<String>,
    original_title: Option<String>,
    release_date: Option<String>,
    overview: Option<String>,
    poster_path: Option<String>,
    backdrop_path: Option<String>,
    #[serde(default)]
    popularity: f64,
}

impl TmdbSearchCandidate for TmdbMovieSearchResult {
    fn candidate_title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    fn candidate_original_title(&self) -> Option<&str> {
        self.original_title.as_deref()
    }

    fn candidate_year(&self) -> Option<i32> {
        parse_year(self.release_date.as_deref())
    }

    fn candidate_popularity(&self) -> f64 {
        self.popularity
    }
}

#[derive(Debug, Clone, Deserialize)]
struct TmdbTvSearchResult {
    id: i64,
    name: Option<String>,
    original_name: Option<String>,
    first_air_date: Option<String>,
    overview: Option<String>,
    poster_path: Option<String>,
    backdrop_path: Option<String>,
    #[serde(default)]
    popularity: f64,
}

impl TmdbSearchCandidate for TmdbTvSearchResult {
    fn candidate_title(&self) -> Option<&str> {
        self.name.as_deref()
    }

    fn candidate_original_title(&self) -> Option<&str> {
        self.original_name.as_deref()
    }

    fn candidate_year(&self) -> Option<i32> {
        parse_year(self.first_air_date.as_deref())
    }

    fn candidate_popularity(&self) -> f64 {
        self.popularity
    }
}

#[derive(Debug, Deserialize)]
struct TmdbMovieDetails {
    title: Option<String>,
    original_title: Option<String>,
    release_date: Option<String>,
    overview: Option<String>,
    poster_path: Option<String>,
    backdrop_path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TmdbCreditsResponse {
    #[serde(default)]
    cast: Vec<TmdbCastCredit>,
}

#[derive(Debug, Deserialize)]
struct TmdbCastCredit {
    id: i64,
    name: Option<String>,
    character: Option<String>,
    profile_path: Option<String>,
    order: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct TmdbTvDetails {
    name: Option<String>,
    original_name: Option<String>,
    first_air_date: Option<String>,
    overview: Option<String>,
    poster_path: Option<String>,
    backdrop_path: Option<String>,
    #[serde(default)]
    seasons: Vec<TmdbTvSeasonSummary>,
}

#[derive(Debug, Deserialize)]
struct TmdbTvSeasonSummary {
    season_number: i32,
    name: Option<String>,
    air_date: Option<String>,
    poster_path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TmdbTvSeasonDetails {
    name: Option<String>,
    air_date: Option<String>,
    overview: Option<String>,
    poster_path: Option<String>,
    #[serde(default)]
    episodes: Vec<TmdbTvEpisodeDetails>,
}

#[derive(Debug, Deserialize)]
struct TmdbTvEpisodeDetails {
    episode_number: i32,
    name: Option<String>,
    overview: Option<String>,
    still_path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TmdbTvAggregateCreditsResponse {
    #[serde(default)]
    cast: Vec<TmdbTvAggregateCastCredit>,
}

#[derive(Debug, Deserialize)]
struct TmdbTvAggregateCastCredit {
    id: i64,
    name: Option<String>,
    profile_path: Option<String>,
    order: Option<i32>,
    #[serde(default)]
    roles: Vec<TmdbTvAggregateRole>,
}

#[derive(Debug, Deserialize)]
struct TmdbTvAggregateRole {
    character: Option<String>,
    episode_count: Option<i32>,
}

fn select_best_match<'a, T>(
    query_title: &str,
    query_year: Option<i32>,
    candidates: &'a [T],
) -> Option<&'a T>
where
    T: TmdbSearchCandidate,
{
    let normalized_query = normalize_title(query_title);

    candidates
        .iter()
        .map(|candidate| {
            let best_title_score = [
                candidate.candidate_title(),
                candidate.candidate_original_title(),
            ]
            .into_iter()
            .flatten()
            .map(|title| title_match_score(&normalized_query, &normalize_title(title)))
            .max()
            .unwrap_or(0);

            let year_score = match (query_year, candidate.candidate_year()) {
                (Some(left), Some(right)) if left == right => 20,
                (Some(left), Some(right)) if (left - right).abs() == 1 => 10,
                (Some(_), Some(_)) => -10,
                _ => 0,
            };

            let score = best_title_score + year_score;

            (
                score,
                (candidate.candidate_popularity() * 1000.0) as i64,
                candidate,
            )
        })
        .filter(|(score, _, _)| *score > 0)
        .max_by_key(|(score, popularity, _)| (*score, *popularity))
        .map(|(_, _, candidate)| candidate)
}

fn title_match_score(query: &str, candidate: &str) -> i32 {
    if query.is_empty() || candidate.is_empty() {
        return 0;
    }

    if query == candidate {
        100
    } else if candidate.starts_with(query) || query.starts_with(candidate) {
        80
    } else if candidate.contains(query) || query.contains(candidate) {
        60
    } else {
        0
    }
}

fn normalize_title(value: &str) -> String {
    value
        .chars()
        .flat_map(|ch| {
            if ch.is_alphanumeric() {
                ch.to_lowercase().collect::<Vec<_>>()
            } else if ch.is_whitespace() {
                vec![' ']
            } else {
                vec![' ']
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn parse_year(value: Option<&str>) -> Option<i32> {
    let value = value?.trim();
    if value.len() < 4 {
        return None;
    }

    value[..4].parse::<i32>().ok()
}

fn empty_to_none(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn pick_primary_character_name(roles: &[TmdbTvAggregateRole]) -> Option<String> {
    roles
        .iter()
        .filter_map(|role| {
            empty_to_none(role.character.clone())
                .map(|character_name| (role.episode_count.unwrap_or(0), character_name))
        })
        .max_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)))
        .map(|(_, character_name)| character_name)
}

/// 扫描任务内部使用的查询缓存。
pub type MetadataLookupCache = HashMap<MetadataLookup, Option<RemoteMetadata>>;

#[cfg(test)]
mod tests {
    use super::{
        apply_remote_metadata, build_metadata_provider, normalize_base_url,
        normalize_metadata_language, normalize_optional_value, normalize_title, parse_year,
        pick_primary_character_name, select_best_match, MetadataLookup, MetadataProviderConfig,
        RemoteMetadata, TmdbMetadataProvider, TmdbMetadataProviderConfig, TmdbMovieSearchResult,
        TmdbTvAggregateRole,
    };

    #[test]
    fn build_metadata_provider_returns_disabled_provider() {
        let provider = build_metadata_provider(MetadataProviderConfig::Disabled).unwrap();

        assert!(!provider.is_enabled());
    }

    #[test]
    fn normalize_optional_value_discards_blank_strings() {
        assert_eq!(normalize_optional_value(Some("   ".to_string())), None);
        assert_eq!(
            normalize_optional_value(Some(" token ".to_string())),
            Some("token".to_string())
        );
    }

    #[test]
    fn normalize_base_url_trims_trailing_slash() {
        assert_eq!(
            normalize_base_url("api base url", "https://api.example.com/".to_string()).unwrap(),
            "https://api.example.com".to_string()
        );
    }

    #[test]
    fn normalize_metadata_language_accepts_supported_values() {
        assert_eq!(
            normalize_metadata_language(Some("en-US".to_string()), "zh-CN").unwrap(),
            "en-US".to_string()
        );
        assert_eq!(
            normalize_metadata_language(None, "zh-CN").unwrap(),
            "zh-CN".to_string()
        );
    }

    #[test]
    fn apply_remote_metadata_uses_remote_title_for_display() {
        let mut title = "Spirited Away".to_string();
        let mut original_title = None;
        let mut year = Some(2001);
        let mut overview = None;
        let mut poster_path = None;
        let mut backdrop_path = Some("/local/backdrop.jpg".to_string());

        apply_remote_metadata(
            Some(RemoteMetadata {
                title: Some("Sen to Chihiro no Kamikakushi".to_string()),
                original_title: Some("Sen to Chihiro no Kamikakushi".to_string()),
                year: Some(2001),
                overview: Some("A girl enters the spirit world.".to_string()),
                poster_path: Some("https://images.example.com/poster.jpg".to_string()),
                backdrop_path: Some("https://images.example.com/backdrop.jpg".to_string()),
            }),
            &mut title,
            &mut original_title,
            &mut year,
            &mut overview,
            &mut poster_path,
            &mut backdrop_path,
        );

        assert_eq!(title, "Sen to Chihiro no Kamikakushi");
        assert_eq!(
            original_title.as_deref(),
            Some("Sen to Chihiro no Kamikakushi")
        );
        assert_eq!(year, Some(2001));
        assert_eq!(overview.as_deref(), Some("A girl enters the spirit world."));
        assert_eq!(
            poster_path.as_deref(),
            Some("https://images.example.com/poster.jpg")
        );
        assert_eq!(backdrop_path.as_deref(), Some("/local/backdrop.jpg"));
    }

    #[test]
    fn tmdb_provider_builds_absolute_image_urls() {
        let provider = TmdbMetadataProvider::new(TmdbMetadataProviderConfig {
            access_token: "token".to_string(),
            language: "zh-CN".to_string(),
            api_base_url: "https://api.themoviedb.org/3".to_string(),
            image_base_url: "https://image.tmdb.org/t/p/original".to_string(),
        })
        .unwrap();

        assert_eq!(
            provider.build_image_url("/poster.jpg"),
            "https://image.tmdb.org/t/p/original/poster.jpg".to_string()
        );
    }

    #[test]
    fn select_best_match_prefers_exact_title_and_year() {
        let candidates = vec![
            TmdbMovieSearchResult {
                id: 1,
                title: Some("Castle in the Sky".to_string()),
                original_title: Some("Tenkū no Shiro Rapyuta".to_string()),
                release_date: Some("1986-08-02".to_string()),
                overview: None,
                poster_path: None,
                backdrop_path: None,
                popularity: 10.0,
            },
            TmdbMovieSearchResult {
                id: 2,
                title: Some("Castle in the Sky".to_string()),
                original_title: Some("Laputa".to_string()),
                release_date: Some("1988-01-01".to_string()),
                overview: None,
                poster_path: None,
                backdrop_path: None,
                popularity: 50.0,
            },
        ];

        let best_match = select_best_match("Castle in the Sky", Some(1986), &candidates).unwrap();

        assert_eq!(best_match.id, 1);
    }

    #[test]
    fn parse_year_extracts_first_four_digits() {
        assert_eq!(parse_year(Some("2001-07-20")), Some(2001));
        assert_eq!(parse_year(Some("1999")), Some(1999));
        assert_eq!(parse_year(Some("  ")), None);
    }

    #[test]
    fn normalize_title_drops_punctuation_and_lowercases() {
        assert_eq!(normalize_title("My.Movie: Part-1"), "my movie part 1");
    }

    #[test]
    fn normalize_title_preserves_unicode_letters() {
        assert_eq!(normalize_title("创：战神"), "创 战神");
    }

    #[test]
    fn metadata_lookup_keeps_library_type_for_movie_and_series() {
        let movie_lookup = MetadataLookup {
            title: "Dune".to_string(),
            year: Some(2021),
            library_type: "movie".to_string(),
            language: Some("zh-CN".to_string()),
            provider_item_id: None,
        };
        let series_lookup = MetadataLookup {
            title: "Dune".to_string(),
            year: Some(2021),
            library_type: "series".to_string(),
            language: Some("zh-CN".to_string()),
            provider_item_id: None,
        };

        assert_ne!(movie_lookup, series_lookup);
    }

    #[test]
    fn pick_primary_character_name_prefers_most_episodes() {
        let character_name = pick_primary_character_name(&[
            TmdbTvAggregateRole {
                character: Some("Guard".to_string()),
                episode_count: Some(2),
            },
            TmdbTvAggregateRole {
                character: Some("Commander".to_string()),
                episode_count: Some(8),
            },
        ]);

        assert_eq!(character_name.as_deref(), Some("Commander"));
    }
}
