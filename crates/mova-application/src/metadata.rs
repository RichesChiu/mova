use async_trait::async_trait;
use reqwest::{
    header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION},
    Client,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc, time::Duration};
use time::OffsetDateTime;

use mova_domain::{MediaExternalId, MediaRating, RATING_KIND_AUDIENCE, RATING_SOURCE_TMDB};

pub const DEFAULT_TMDB_LANGUAGE: &str = "zh-CN";
pub const SUPPORTED_TMDB_LANGUAGES: &[&str] = &["zh-CN", "en-US"];
pub const DEFAULT_TMDB_API_BASE_URL: &str = "https://api.themoviedb.org/3";
pub const DEFAULT_TMDB_IMAGE_BASE_URL: &str = "https://image.tmdb.org/t/p/original";
pub const TMDB_PROVIDER_NAME: &str = "tmdb";
const TMDB_MAX_AUTO_MATCH_PAGES: u32 = 20;
const TMDB_MAX_ALTERNATIVE_TITLE_CANDIDATES: usize = 40;
const METADATA_PROVIDER_CONNECT_TIMEOUT: Duration = Duration::from_secs(4);
const METADATA_PROVIDER_REQUEST_TIMEOUT: Duration = Duration::from_secs(12);

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
    /// 电影发行年或剧集首播年。
    pub year: Option<i32>,
    /// 缺少第一季时，后续季文件年份仅用于验证对应季，不参与系列首播年比较。
    pub season_air_year: Option<MetadataSeasonAirYearHint>,
    pub library_type: String,
    pub language: Option<String>,
    pub provider_item_id: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MetadataSeasonAirYearHint {
    pub season_number: i32,
    pub year: i32,
}

/// 第三方元数据源返回的统一结构。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RemoteMetadata {
    pub provider_item_id: Option<i64>,
    pub title: Option<String>,
    pub original_title: Option<String>,
    pub year: Option<i32>,
    pub external_ids: Vec<MediaExternalId>,
    pub ratings: Vec<MediaRating>,
    pub country: Option<String>,
    pub genres: Option<String>,
    pub studio: Option<String>,
    pub overview: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub logo_path: Option<String>,
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
            .connect_timeout(METADATA_PROVIDER_CONNECT_TIMEOUT)
            .timeout(METADATA_PROVIDER_REQUEST_TIMEOUT)
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
                let candidates = self.search_movie_candidates(lookup).await?;
                let Some(best_match) = self.select_strict_movie_match(lookup, &candidates).await?
                else {
                    return Ok(None);
                };

                best_match.id
            }
        };

        let details = self.fetch_movie_details(movie_id, request_language).await?;
        Ok(Some(self.map_movie_details(
            movie_id,
            details,
            request_language,
        )))
    }

    async fn search_movie_candidates(
        &self,
        lookup: &MetadataLookup,
    ) -> anyhow::Result<Vec<TmdbMovieSearchResult>> {
        let first_page = self.search_movie_response(lookup, 1).await?;
        let total_pages = first_page.total_pages.unwrap_or(1).max(1);
        if total_pages > TMDB_MAX_AUTO_MATCH_PAGES {
            tracing::warn!(
                title = %lookup.title,
                total_pages,
                max_pages = TMDB_MAX_AUTO_MATCH_PAGES,
                "tmdb movie search is too broad for strict automatic matching"
            );
            return Ok(Vec::new());
        }
        let mut results = first_page.results;

        for page in 2..=total_pages {
            results.extend(self.search_movie_response(lookup, page).await?.results);
        }

        Ok(deduplicate_search_results(results, |result| result.id))
    }

    async fn search_movie_response(
        &self,
        lookup: &MetadataLookup,
        page: u32,
    ) -> anyhow::Result<TmdbSearchResponse<TmdbMovieSearchResult>> {
        let request_language = self.request_language(lookup);
        let mut query = vec![
            ("query", lookup.title.clone()),
            ("include_adult", "false".to_string()),
            ("page", page.to_string()),
            ("language", request_language.to_string()),
        ];

        if let Some(year) = lookup.year {
            query.push(("primary_release_year", year.to_string()));
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
        let candidates = self.search_tv_candidates(lookup).await?;

        let best_match = self
            .select_strict_tv_match(lookup, &candidates)
            .await?
            .cloned();

        Ok(best_match)
    }

    async fn search_tv_candidates(
        &self,
        lookup: &MetadataLookup,
    ) -> anyhow::Result<Vec<TmdbTvSearchResult>> {
        let first_page = self.search_tv_response(lookup, 1).await?;
        let total_pages = first_page.total_pages.unwrap_or(1).max(1);
        if total_pages > TMDB_MAX_AUTO_MATCH_PAGES {
            tracing::warn!(
                title = %lookup.title,
                total_pages,
                max_pages = TMDB_MAX_AUTO_MATCH_PAGES,
                "tmdb tv search is too broad for strict automatic matching"
            );
            return Ok(Vec::new());
        }
        let mut results = first_page.results;

        for page in 2..=total_pages {
            results.extend(self.search_tv_response(lookup, page).await?.results);
        }

        Ok(deduplicate_search_results(results, |result| result.id))
    }

    async fn search_tv_response(
        &self,
        lookup: &MetadataLookup,
        page: u32,
    ) -> anyhow::Result<TmdbSearchResponse<TmdbTvSearchResult>> {
        let request_language = self.request_language(lookup);
        let mut query = vec![
            ("query", lookup.title.clone()),
            ("include_adult", "false".to_string()),
            ("page", page.to_string()),
            ("language", request_language.to_string()),
        ];

        if let Some((parameter, year)) = tv_search_year_filter(lookup) {
            query.push((parameter, year.to_string()));
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

    async fn fetch_movie_alternative_titles(&self, movie_id: i64) -> anyhow::Result<Vec<String>> {
        let response = self
            .client
            .get(format!(
                "{}/movie/{}/alternative_titles",
                self.api_base_url, movie_id
            ))
            .send()
            .await?
            .error_for_status()?
            .json::<TmdbMovieAlternativeTitlesResponse>()
            .await?;

        Ok(response
            .titles
            .into_iter()
            .filter_map(|title| empty_to_none(title.title))
            .collect())
    }

    async fn fetch_tv_alternative_titles(&self, tv_id: i64) -> anyhow::Result<Vec<String>> {
        let response = self
            .client
            .get(format!(
                "{}/tv/{}/alternative_titles",
                self.api_base_url, tv_id
            ))
            .send()
            .await?
            .error_for_status()?
            .json::<TmdbTvAlternativeTitlesResponse>()
            .await?;

        Ok(response
            .results
            .into_iter()
            .filter_map(|title| empty_to_none(title.title))
            .collect())
    }

    async fn select_strict_movie_match<'a>(
        &self,
        lookup: &MetadataLookup,
        candidates: &'a [TmdbMovieSearchResult],
    ) -> anyhow::Result<Option<&'a TmdbMovieSearchResult>> {
        let eligible_candidates = candidates
            .iter()
            .filter(|candidate| candidate_matches_year(lookup.year, *candidate))
            .collect::<Vec<_>>();
        let direct_candidates = strongest_direct_title_matches(
            &lookup.title,
            eligible_candidates.iter().copied().collect(),
        );
        if !direct_candidates.is_empty() {
            return Ok(select_strict_candidate(lookup.year, direct_candidates));
        }
        if eligible_candidates.len() > TMDB_MAX_ALTERNATIVE_TITLE_CANDIDATES {
            tracing::warn!(
                title = %lookup.title,
                candidate_count = eligible_candidates.len(),
                max_candidates = TMDB_MAX_ALTERNATIVE_TITLE_CANDIDATES,
                "tmdb movie candidates are too broad for alternative-title verification"
            );
            return Ok(None);
        }

        let mut exact_candidates = Vec::new();
        let mut compatible_candidates = Vec::new();

        for candidate in eligible_candidates {
            let alternative_titles = match self.fetch_movie_alternative_titles(candidate.id).await {
                Ok(titles) => titles,
                Err(error) => {
                    tracing::warn!(
                        movie_id = candidate.id,
                        error = ?error,
                        "failed to verify tmdb movie alternative titles"
                    );
                    continue;
                }
            };
            if alternative_titles
                .iter()
                .any(|title| titles_match_exactly(&lookup.title, title))
            {
                exact_candidates.push(candidate);
            } else if alternative_titles
                .iter()
                .any(|title| titles_match_numbered_subtitle(&lookup.title, title))
            {
                compatible_candidates.push(candidate);
            }
        }

        Ok(select_strict_candidate(
            lookup.year,
            if exact_candidates.is_empty() {
                compatible_candidates
            } else {
                exact_candidates
            },
        ))
    }

    async fn select_strict_tv_match<'a>(
        &self,
        lookup: &MetadataLookup,
        candidates: &'a [TmdbTvSearchResult],
    ) -> anyhow::Result<Option<&'a TmdbTvSearchResult>> {
        let eligible_candidates = candidates
            .iter()
            .filter(|candidate| candidate_matches_year(lookup.year, *candidate))
            .collect::<Vec<_>>();
        let direct_candidates = strongest_direct_title_matches(
            &lookup.title,
            eligible_candidates.iter().copied().collect(),
        );
        if !direct_candidates.is_empty() {
            let direct_candidates = self
                .filter_tv_candidates_by_season_air_year(lookup, direct_candidates)
                .await;
            return Ok(select_strict_tv_candidate(lookup, direct_candidates));
        }
        if eligible_candidates.len() > TMDB_MAX_ALTERNATIVE_TITLE_CANDIDATES {
            tracing::warn!(
                title = %lookup.title,
                candidate_count = eligible_candidates.len(),
                max_candidates = TMDB_MAX_ALTERNATIVE_TITLE_CANDIDATES,
                "tmdb tv candidates are too broad for alternative-title verification"
            );
            return Ok(None);
        }

        let mut exact_candidates = Vec::new();
        let mut compatible_candidates = Vec::new();

        for candidate in eligible_candidates {
            let alternative_titles = match self.fetch_tv_alternative_titles(candidate.id).await {
                Ok(titles) => titles,
                Err(error) => {
                    tracing::warn!(
                        tv_id = candidate.id,
                        error = ?error,
                        "failed to verify tmdb tv alternative titles"
                    );
                    continue;
                }
            };
            if alternative_titles
                .iter()
                .any(|title| titles_match_exactly(&lookup.title, title))
            {
                exact_candidates.push(candidate);
            } else if alternative_titles
                .iter()
                .any(|title| titles_match_numbered_subtitle(&lookup.title, title))
            {
                compatible_candidates.push(candidate);
            }
        }

        let exact_candidates = if exact_candidates.is_empty() {
            compatible_candidates
        } else {
            exact_candidates
        };
        let exact_candidates = self
            .filter_tv_candidates_by_season_air_year(lookup, exact_candidates)
            .await;

        Ok(select_strict_tv_candidate(lookup, exact_candidates))
    }

    async fn filter_tv_candidates_by_season_air_year<'a>(
        &self,
        lookup: &MetadataLookup,
        candidates: Vec<&'a TmdbTvSearchResult>,
    ) -> Vec<&'a TmdbTvSearchResult> {
        let Some(hint) = lookup.season_air_year else {
            return candidates;
        };

        if candidates.len() > TMDB_MAX_ALTERNATIVE_TITLE_CANDIDATES {
            tracing::warn!(
                title = %lookup.title,
                season_number = hint.season_number,
                season_air_year = hint.year,
                candidate_count = candidates.len(),
                max_candidates = TMDB_MAX_ALTERNATIVE_TITLE_CANDIDATES,
                "tmdb tv candidates are too broad for season-year verification"
            );
            return Vec::new();
        }

        let request_language = self.request_language(lookup);
        let mut verified = Vec::new();
        for candidate in candidates {
            match self
                .fetch_tv_season_details(candidate.id, hint.season_number, request_language)
                .await
            {
                Ok(details) if tmdb_tv_season_matches_air_year(&details, hint.year) => {
                    verified.push(candidate);
                }
                Ok(_) => {}
                Err(error) => {
                    tracing::warn!(
                        tv_id = candidate.id,
                        season_number = hint.season_number,
                        season_air_year = hint.year,
                        error = ?error,
                        "failed to verify tmdb tv season air year"
                    );
                }
            }
        }

        verified
    }

    async fn fetch_movie_details(
        &self,
        movie_id: i64,
        language: &str,
    ) -> anyhow::Result<TmdbMovieDetails> {
        let image_languages = tmdb_image_languages(language);
        let details = self
            .client
            .get(format!("{}/movie/{}", self.api_base_url, movie_id))
            .query(&[
                ("language", language),
                ("append_to_response", "external_ids,images"),
                ("include_image_language", image_languages.as_str()),
            ])
            .send()
            .await?
            .error_for_status()?
            .json::<TmdbMovieDetails>()
            .await?;

        Ok(details)
    }

    async fn fetch_tv_details(&self, tv_id: i64, language: &str) -> anyhow::Result<TmdbTvDetails> {
        let image_languages = tmdb_image_languages(language);
        let details = self
            .client
            .get(format!("{}/tv/{}", self.api_base_url, tv_id))
            .query(&[
                ("language", language),
                ("append_to_response", "external_ids,images"),
                ("include_image_language", image_languages.as_str()),
            ])
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

        let request_language = self.request_language(lookup);
        let details = self.fetch_tv_details(tv_id, request_language).await?;

        Ok(Some(RemoteMetadata {
            provider_item_id: Some(tv_id),
            title: empty_to_none(details.name),
            original_title: empty_to_none(details.original_name),
            year: parse_year(details.first_air_date.as_deref()),
            external_ids: tmdb_external_ids(tv_id, details.external_ids.as_ref()),
            ratings: tmdb_ratings(details.vote_average, details.vote_count),
            country: format_country_codes(&details.origin_country),
            genres: format_named_items(&details.genres),
            studio: format_named_items(&details.production_companies),
            overview: empty_to_none(details.overview),
            poster_path: details
                .poster_path
                .as_deref()
                .map(|path| self.build_image_url(path)),
            backdrop_path: details
                .backdrop_path
                .as_deref()
                .map(|path| self.build_image_url(path)),
            logo_path: select_tmdb_logo(details.images.as_ref(), request_language)
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
                        poster_path: still_path,
                        backdrop_path: None,
                    }
                })
                .collect::<Vec<_>>();
            episodes.sort_by_key(|episode| episode.episode_number);

            let season_poster_path = season_details
                .poster_path
                .as_deref()
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
                let candidates = self.search_movie_candidates(lookup).await?;
                let Some(best_match) = self.select_strict_movie_match(lookup, &candidates).await?
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

    fn map_movie_details(
        &self,
        movie_id: i64,
        details: TmdbMovieDetails,
        request_language: &str,
    ) -> RemoteMetadata {
        let poster_path = details
            .poster_path
            .as_deref()
            .map(|path| self.build_image_url(path));
        let backdrop_path = details
            .backdrop_path
            .as_deref()
            .map(|path| self.build_image_url(path));

        RemoteMetadata {
            provider_item_id: Some(movie_id),
            title: empty_to_none(details.title),
            original_title: empty_to_none(details.original_title),
            year: parse_year(details.release_date.as_deref()),
            external_ids: tmdb_external_ids(movie_id, details.external_ids.as_ref()),
            ratings: tmdb_ratings(details.vote_average, details.vote_count),
            country: format_production_countries(&details.production_countries),
            genres: format_named_items(&details.genres),
            studio: format_named_items(&details.production_companies),
            overview: empty_to_none(details.overview),
            poster_path,
            backdrop_path,
            logo_path: select_tmdb_logo(details.images.as_ref(), request_language)
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
            let results = self.search_tv_response(lookup, 1).await?.results;
            return Ok(results
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

        let response = self.search_movie_response(lookup, 1).await?.results;
        Ok(response
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

fn deduplicate_search_results<T, F>(results: Vec<T>, id_fn: F) -> Vec<T>
where
    F: Fn(&T) -> i64,
{
    let mut deduplicated = Vec::with_capacity(results.len());
    let mut seen_ids = std::collections::HashSet::new();

    for item in results {
        if seen_ids.insert(id_fn(&item)) {
            deduplicated.push(item);
        }
    }

    deduplicated
}

/// 把远程元数据补到本地扫描结果里。
/// 展示标题会优先使用远端返回的本地化标题；原始文件名标题则单独存到 `source_title`。
pub fn apply_remote_metadata(
    metadata: Option<RemoteMetadata>,
    metadata_provider: &mut Option<String>,
    metadata_provider_item_id: &mut Option<i64>,
    title: &mut String,
    original_title: &mut Option<String>,
    year: &mut Option<i32>,
    external_ids: &mut Vec<MediaExternalId>,
    ratings: &mut Vec<MediaRating>,
    country: &mut Option<String>,
    genres: &mut Option<String>,
    studio: &mut Option<String>,
    overview: &mut Option<String>,
    poster_path: &mut Option<String>,
    backdrop_path: &mut Option<String>,
) {
    let Some(metadata) = metadata else {
        return;
    };

    if metadata_provider.is_none() && metadata.provider_item_id.is_some() {
        *metadata_provider = Some(TMDB_PROVIDER_NAME.to_string());
    }

    if metadata_provider_item_id.is_none() {
        *metadata_provider_item_id = metadata.provider_item_id;
    }

    if let Some(remote_title) = normalize_optional_value(metadata.title) {
        *title = remote_title;
    }

    if original_title.is_none() {
        *original_title = metadata.original_title;
    }

    if year.is_none() {
        *year = metadata.year;
    }

    *external_ids = metadata.external_ids;
    *ratings = metadata.ratings;

    if country.is_none() {
        *country = metadata.country;
    }

    if genres.is_none() {
        *genres = metadata.genres;
    }

    if studio.is_none() {
        *studio = metadata.studio;
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
    fn candidate_date(&self) -> Option<&str>;
}

#[derive(Debug, Deserialize)]
struct TmdbSearchResponse<T> {
    results: Vec<T>,
    total_pages: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
struct TmdbMovieSearchResult {
    id: i64,
    title: Option<String>,
    original_title: Option<String>,
    release_date: Option<String>,
    overview: Option<String>,
    poster_path: Option<String>,
    backdrop_path: Option<String>,
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

    fn candidate_date(&self) -> Option<&str> {
        self.release_date.as_deref()
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

    fn candidate_date(&self) -> Option<&str> {
        self.first_air_date.as_deref()
    }
}

#[derive(Debug, Deserialize)]
struct TmdbMovieAlternativeTitlesResponse {
    #[serde(default)]
    titles: Vec<TmdbAlternativeTitle>,
}

#[derive(Debug, Deserialize)]
struct TmdbTvAlternativeTitlesResponse {
    #[serde(default)]
    results: Vec<TmdbAlternativeTitle>,
}

#[derive(Debug, Deserialize)]
struct TmdbAlternativeTitle {
    title: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TmdbMovieDetails {
    title: Option<String>,
    original_title: Option<String>,
    release_date: Option<String>,
    overview: Option<String>,
    poster_path: Option<String>,
    backdrop_path: Option<String>,
    #[serde(default)]
    production_countries: Vec<TmdbProductionCountry>,
    #[serde(default)]
    genres: Vec<TmdbNamedItem>,
    #[serde(default)]
    production_companies: Vec<TmdbNamedItem>,
    external_ids: Option<TmdbExternalIds>,
    images: Option<TmdbImagesResponse>,
    vote_average: Option<f64>,
    vote_count: Option<i64>,
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
    origin_country: Vec<String>,
    #[serde(default)]
    genres: Vec<TmdbNamedItem>,
    #[serde(default)]
    production_companies: Vec<TmdbNamedItem>,
    external_ids: Option<TmdbExternalIds>,
    images: Option<TmdbImagesResponse>,
    vote_average: Option<f64>,
    vote_count: Option<i64>,
    #[serde(default)]
    seasons: Vec<TmdbTvSeasonSummary>,
}

#[derive(Debug, Deserialize)]
struct TmdbProductionCountry {
    iso_3166_1: Option<String>,
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TmdbNamedItem {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TmdbExternalIds {
    imdb_id: Option<String>,
    tvdb_id: Option<i64>,
    wikidata_id: Option<String>,
    facebook_id: Option<String>,
    instagram_id: Option<String>,
    twitter_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TmdbImagesResponse {
    #[serde(default)]
    logos: Vec<TmdbLogo>,
}

#[derive(Debug, Deserialize)]
struct TmdbLogo {
    file_path: Option<String>,
    iso_639_1: Option<String>,
    width: Option<i64>,
    height: Option<i64>,
    vote_average: Option<f64>,
    vote_count: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct TmdbTvSeasonSummary {
    season_number: i32,
    name: Option<String>,
    air_date: Option<String>,
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
    air_date: Option<String>,
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

fn strongest_direct_title_matches<'a, T>(query_title: &str, candidates: Vec<&'a T>) -> Vec<&'a T>
where
    T: TmdbSearchCandidate,
{
    let exact_original_title_matches = candidates
        .iter()
        .copied()
        .filter(|candidate| {
            candidate
                .candidate_original_title()
                .is_some_and(|title| titles_match_exactly(query_title, title))
        })
        .collect::<Vec<_>>();
    if !exact_original_title_matches.is_empty() {
        return exact_original_title_matches;
    }

    let exact_localized_title_matches = candidates
        .iter()
        .copied()
        .filter(|candidate| {
            candidate
                .candidate_title()
                .is_some_and(|title| titles_match_exactly(query_title, title))
        })
        .collect::<Vec<_>>();
    if !exact_localized_title_matches.is_empty() {
        return exact_localized_title_matches;
    }

    let compatible_original_title_matches = candidates
        .iter()
        .copied()
        .filter(|candidate| {
            candidate
                .candidate_original_title()
                .is_some_and(|title| titles_match_numbered_subtitle(query_title, title))
        })
        .collect::<Vec<_>>();
    if !compatible_original_title_matches.is_empty() {
        return compatible_original_title_matches;
    }

    candidates
        .into_iter()
        .filter(|candidate| {
            candidate
                .candidate_title()
                .is_some_and(|title| titles_match_numbered_subtitle(query_title, title))
        })
        .collect()
}

fn titles_match_exactly(local_title: &str, remote_title: &str) -> bool {
    let normalized_local_title = normalize_title(local_title);
    if normalized_local_title.is_empty() {
        return false;
    }

    normalized_local_title == normalize_title(remote_title)
}

fn titles_match_numbered_subtitle(local_title: &str, remote_title: &str) -> bool {
    let normalized_local_title = normalize_title(local_title);
    if normalized_local_title.is_empty() {
        return false;
    }

    normalized_local_title
        .chars()
        .last()
        .is_some_and(|value| value.is_ascii_digit())
        && title_before_explicit_subtitle(remote_title)
            .is_some_and(|title| normalized_local_title == normalize_title(title))
}

fn title_before_explicit_subtitle(value: &str) -> Option<&str> {
    value.char_indices().find_map(|(index, separator)| {
        if !matches!(separator, ':' | '：' | '|' | '｜' | '–' | '—') {
            return None;
        }

        let title = value[..index].trim();
        let subtitle = value[index + separator.len_utf8()..].trim();
        (!title.is_empty() && !subtitle.is_empty()).then_some(title)
    })
}

fn candidate_matches_year<T>(query_year: Option<i32>, candidate: &T) -> bool
where
    T: TmdbSearchCandidate,
{
    match query_year {
        Some(query_year) => candidate.candidate_year() == Some(query_year),
        None => true,
    }
}

fn tv_search_year_filter(lookup: &MetadataLookup) -> Option<(&'static str, i32)> {
    lookup
        .year
        .map(|year| ("first_air_date_year", year))
        .or_else(|| lookup.season_air_year.map(|hint| ("year", hint.year)))
}

fn tmdb_tv_season_matches_air_year(details: &TmdbTvSeasonDetails, year: i32) -> bool {
    parse_year(details.air_date.as_deref()) == Some(year)
        || details
            .episodes
            .iter()
            .any(|episode| parse_year(episode.air_date.as_deref()) == Some(year))
}

fn select_strict_candidate<'a, T>(query_year: Option<i32>, candidates: Vec<&'a T>) -> Option<&'a T>
where
    T: TmdbSearchCandidate,
{
    if query_year.is_some() {
        return (candidates.len() == 1).then(|| candidates[0]);
    }

    let newest_date = candidates
        .iter()
        .filter_map(|candidate| normalize_tmdb_date(candidate.candidate_date()))
        .max()?;
    let newest_candidates = candidates
        .into_iter()
        .filter(|candidate| normalize_tmdb_date(candidate.candidate_date()) == Some(newest_date))
        .collect::<Vec<_>>();

    (newest_candidates.len() == 1).then(|| newest_candidates[0])
}

fn select_strict_tv_candidate<'a>(
    lookup: &MetadataLookup,
    candidates: Vec<&'a TmdbTvSearchResult>,
) -> Option<&'a TmdbTvSearchResult> {
    if lookup.season_air_year.is_some() {
        return (candidates.len() == 1).then(|| candidates[0]);
    }

    select_strict_candidate(lookup.year, candidates)
}

fn normalize_tmdb_date(value: Option<&str>) -> Option<&str> {
    let value = value?.trim();
    if value.len() == 10
        && value.as_bytes()[4] == b'-'
        && value.as_bytes()[7] == b'-'
        && value
            .chars()
            .enumerate()
            .all(|(index, ch)| index == 4 || index == 7 || ch.is_ascii_digit())
    {
        Some(value)
    } else {
        None
    }
}

fn normalize_title(value: &str) -> String {
    let characters = value.chars().collect::<Vec<_>>();

    characters
        .iter()
        .enumerate()
        .filter_map(|(index, ch)| {
            if is_intra_word_stylized_dollar(&characters, index) {
                return Some("s".to_string());
            }

            if is_ignorable_title_punctuation(*ch) {
                return None;
            }

            if ch.is_alphanumeric() {
                return Some(ch.to_lowercase().collect::<String>());
            }

            Some(" ".to_string())
        })
        .collect::<Vec<_>>()
        .join("")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_intra_word_stylized_dollar(characters: &[char], index: usize) -> bool {
    characters.get(index) == Some(&'$')
        && index > 0
        && characters
            .get(index - 1)
            .is_some_and(|ch| ch.is_ascii_alphabetic())
        && characters
            .get(index + 1)
            .is_some_and(|ch| ch.is_ascii_alphabetic())
}

fn is_ignorable_title_punctuation(ch: char) -> bool {
    matches!(
        ch,
        '\'' | '’' | '‘' | '`' | 'ʼ' | '·' | '・' | '•' | '‧' | '∙' | '⋅'
    )
}

fn tmdb_external_ids(tmdb_id: i64, external_ids: Option<&TmdbExternalIds>) -> Vec<MediaExternalId> {
    let mut ids = vec![MediaExternalId {
        provider: TMDB_PROVIDER_NAME.to_string(),
        external_id: tmdb_id.to_string(),
    }];

    if let Some(external_ids) = external_ids {
        push_external_id(&mut ids, "imdb", external_ids.imdb_id.as_deref());
        if let Some(tvdb_id) = external_ids.tvdb_id.filter(|id| *id > 0) {
            ids.push(MediaExternalId {
                provider: "tvdb".to_string(),
                external_id: tvdb_id.to_string(),
            });
        }
        push_external_id(&mut ids, "wikidata", external_ids.wikidata_id.as_deref());
        push_external_id(&mut ids, "facebook", external_ids.facebook_id.as_deref());
        push_external_id(&mut ids, "instagram", external_ids.instagram_id.as_deref());
        push_external_id(&mut ids, "twitter", external_ids.twitter_id.as_deref());
    }

    ids
}

fn push_external_id(ids: &mut Vec<MediaExternalId>, provider: &str, value: Option<&str>) {
    let Some(external_id) = value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
    else {
        return;
    };

    ids.push(MediaExternalId {
        provider: provider.to_string(),
        external_id,
    });
}

fn select_tmdb_logo<'a>(
    images: Option<&'a TmdbImagesResponse>,
    request_language: &str,
) -> Option<&'a str> {
    images?
        .logos
        .iter()
        .filter(|logo| {
            logo.file_path
                .as_deref()
                .is_some_and(|path| !path.trim().is_empty())
                && tmdb_logo_language_priority(logo.iso_639_1.as_deref(), request_language) < 3
        })
        .min_by(|left, right| {
            tmdb_logo_language_priority(left.iso_639_1.as_deref(), request_language)
                .cmp(&tmdb_logo_language_priority(
                    right.iso_639_1.as_deref(),
                    request_language,
                ))
                .then_with(|| {
                    right
                        .vote_average
                        .unwrap_or_default()
                        .total_cmp(&left.vote_average.unwrap_or_default())
                })
                .then_with(|| {
                    right
                        .vote_count
                        .unwrap_or_default()
                        .cmp(&left.vote_count.unwrap_or_default())
                })
                .then_with(|| {
                    let left_area = i64::from(left.width.unwrap_or_default())
                        * i64::from(left.height.unwrap_or_default());
                    let right_area = i64::from(right.width.unwrap_or_default())
                        * i64::from(right.height.unwrap_or_default());
                    right_area.cmp(&left_area)
                })
        })
        .and_then(|logo| logo.file_path.as_deref())
        .map(str::trim)
}

fn tmdb_logo_language_priority(logo_language: Option<&str>, request_language: &str) -> u8 {
    let request_language = request_language
        .split_once('-')
        .map(|(language, _)| language)
        .unwrap_or(request_language);

    if request_language.eq_ignore_ascii_case("zh") {
        return match logo_language {
            Some(language) if language.eq_ignore_ascii_case("en") => 0,
            None => 1,
            Some(language) if language.eq_ignore_ascii_case("zh") => 2,
            _ => 3,
        };
    }

    match logo_language {
        Some(language) if language.eq_ignore_ascii_case(request_language) => 0,
        Some(language) if language.eq_ignore_ascii_case("en") => 1,
        None => 2,
        _ => 3,
    }
}

fn tmdb_image_languages(request_language: &str) -> String {
    let request_language = request_language
        .split_once('-')
        .map(|(language, _)| language)
        .unwrap_or(request_language)
        .trim()
        .to_ascii_lowercase();

    match request_language.as_str() {
        "zh" => "en,null,zh".to_string(),
        "en" | "" => "en,null".to_string(),
        language => format!("{language},en,null"),
    }
}

fn tmdb_ratings(vote_average: Option<f64>, vote_count: Option<i64>) -> Vec<MediaRating> {
    let Some(score) =
        vote_average.filter(|score| score.is_finite() && (0.0..=10.0).contains(score))
    else {
        return Vec::new();
    };
    let Some(rating_count) = vote_count.filter(|count| *count > 0) else {
        return Vec::new();
    };

    vec![MediaRating {
        source: RATING_SOURCE_TMDB.to_string(),
        kind: RATING_KIND_AUDIENCE.to_string(),
        score,
        scale: 10.0,
        rating_count: Some(rating_count),
        retrieved_via: TMDB_PROVIDER_NAME.to_string(),
        attributes: serde_json::json!({}),
        fetched_at: OffsetDateTime::now_utc(),
    }]
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

fn format_country_codes(codes: &[String]) -> Option<String> {
    join_non_empty_values(
        codes
            .iter()
            .filter_map(|code| empty_to_none(Some(code.clone())))
            .collect(),
    )
}

fn format_production_countries(countries: &[TmdbProductionCountry]) -> Option<String> {
    join_non_empty_values(
        countries
            .iter()
            .filter_map(|country| {
                empty_to_none(country.name.clone())
                    .or_else(|| empty_to_none(country.iso_3166_1.clone()))
            })
            .collect(),
    )
}

fn format_named_items(items: &[TmdbNamedItem]) -> Option<String> {
    join_non_empty_values(
        items
            .iter()
            .filter_map(|item| empty_to_none(item.name.clone()))
            .collect(),
    )
}

fn join_non_empty_values(values: Vec<String>) -> Option<String> {
    let mut unique_values = Vec::<String>::new();

    for value in values {
        if !unique_values.iter().any(|existing| existing == &value) {
            unique_values.push(value);
        }
    }

    (!unique_values.is_empty()).then(|| unique_values.join(" · "))
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
        apply_remote_metadata, build_metadata_provider, deduplicate_search_results,
        format_country_codes, normalize_base_url, normalize_metadata_language,
        normalize_optional_value, normalize_title, parse_year, pick_primary_character_name,
        select_strict_candidate, select_strict_tv_candidate, select_tmdb_logo,
        strongest_direct_title_matches, titles_match_exactly, titles_match_numbered_subtitle,
        tmdb_external_ids, tmdb_image_languages, tmdb_ratings, tmdb_tv_season_matches_air_year,
        tv_search_year_filter, MetadataLookup, MetadataProviderConfig, MetadataSeasonAirYearHint,
        RemoteMetadata, TmdbExternalIds, TmdbImagesResponse, TmdbLogo, TmdbMetadataProvider,
        TmdbMetadataProviderConfig, TmdbMovieDetails, TmdbMovieSearchResult, TmdbTvAggregateRole,
        TmdbTvEpisodeDetails, TmdbTvSearchResult, TmdbTvSeasonDetails, TMDB_PROVIDER_NAME,
    };
    use mova_domain::{MediaExternalId, MediaRating};
    use time::OffsetDateTime;

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
        let mut metadata_provider = None;
        let mut metadata_provider_item_id = None;
        let mut title = "Spirited Away".to_string();
        let mut original_title = None;
        let mut year = Some(2001);
        let mut external_ids = Vec::new();
        let mut ratings = Vec::new();
        let mut country = None;
        let mut genres = None;
        let mut studio = None;
        let mut overview = None;
        let mut poster_path = None;
        let mut backdrop_path = Some("/local/backdrop.jpg".to_string());

        apply_remote_metadata(
            Some(RemoteMetadata {
                provider_item_id: Some(129),
                title: Some("Sen to Chihiro no Kamikakushi".to_string()),
                original_title: Some("Sen to Chihiro no Kamikakushi".to_string()),
                year: Some(2001),
                external_ids: vec![MediaExternalId {
                    provider: "tmdb".to_string(),
                    external_id: "129".to_string(),
                }],
                ratings: vec![MediaRating {
                    source: "tmdb".to_string(),
                    kind: "audience".to_string(),
                    score: 8.6,
                    scale: 10.0,
                    rating_count: Some(1_000),
                    retrieved_via: "tmdb".to_string(),
                    attributes: serde_json::json!({}),
                    fetched_at: OffsetDateTime::UNIX_EPOCH,
                }],
                country: Some("Japan".to_string()),
                genres: Some("Animation · Fantasy".to_string()),
                studio: Some("Studio Ghibli".to_string()),
                overview: Some("A girl enters the spirit world.".to_string()),
                poster_path: Some("https://images.example.com/poster.jpg".to_string()),
                backdrop_path: Some("https://images.example.com/backdrop.jpg".to_string()),
                logo_path: None,
            }),
            &mut metadata_provider,
            &mut metadata_provider_item_id,
            &mut title,
            &mut original_title,
            &mut year,
            &mut external_ids,
            &mut ratings,
            &mut country,
            &mut genres,
            &mut studio,
            &mut overview,
            &mut poster_path,
            &mut backdrop_path,
        );

        assert_eq!(metadata_provider.as_deref(), Some(TMDB_PROVIDER_NAME));
        assert_eq!(metadata_provider_item_id, Some(129));
        assert_eq!(title, "Sen to Chihiro no Kamikakushi");
        assert_eq!(
            original_title.as_deref(),
            Some("Sen to Chihiro no Kamikakushi")
        );
        assert_eq!(year, Some(2001));
        assert_eq!(external_ids[0].external_id, "129");
        assert_eq!(ratings[0].score, 8.6);
        assert_eq!(country.as_deref(), Some("Japan"));
        assert_eq!(genres.as_deref(), Some("Animation · Fantasy"));
        assert_eq!(studio.as_deref(), Some("Studio Ghibli"));
        assert_eq!(overview.as_deref(), Some("A girl enters the spirit world."));
        assert_eq!(
            poster_path.as_deref(),
            Some("https://images.example.com/poster.jpg")
        );
        assert_eq!(backdrop_path.as_deref(), Some("/local/backdrop.jpg"));
    }

    #[test]
    fn apply_remote_metadata_ignores_empty_remote_title() {
        let mut metadata_provider = None;
        let mut metadata_provider_item_id = None;
        let mut title = "Local Title".to_string();
        let mut original_title = None;
        let mut year = None;
        let mut external_ids = Vec::new();
        let mut ratings = Vec::new();
        let mut country = None;
        let mut genres = None;
        let mut studio = None;
        let mut overview = None;
        let mut poster_path = None;
        let mut backdrop_path = None;

        apply_remote_metadata(
            Some(RemoteMetadata {
                provider_item_id: Some(321),
                title: Some("   ".to_string()),
                original_title: None,
                year: None,
                external_ids: Vec::new(),
                ratings: Vec::new(),
                country: None,
                genres: None,
                studio: None,
                overview: None,
                poster_path: None,
                backdrop_path: None,
                logo_path: None,
            }),
            &mut metadata_provider,
            &mut metadata_provider_item_id,
            &mut title,
            &mut original_title,
            &mut year,
            &mut external_ids,
            &mut ratings,
            &mut country,
            &mut genres,
            &mut studio,
            &mut overview,
            &mut poster_path,
            &mut backdrop_path,
        );

        assert_eq!(metadata_provider.as_deref(), Some(TMDB_PROVIDER_NAME));
        assert_eq!(metadata_provider_item_id, Some(321));
        assert_eq!(title, "Local Title");
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
    fn map_movie_details_keeps_missing_images_empty() {
        let provider = TmdbMetadataProvider::new(TmdbMetadataProviderConfig {
            access_token: "token".to_string(),
            language: "zh-CN".to_string(),
            api_base_url: "https://api.themoviedb.org/3".to_string(),
            image_base_url: "https://image.tmdb.org/t/p/original".to_string(),
        })
        .unwrap();

        let metadata = provider.map_movie_details(
            88,
            TmdbMovieDetails {
                title: Some("狂野时代".to_string()),
                original_title: Some("Resurrection".to_string()),
                release_date: Some("2025-01-01".to_string()),
                overview: None,
                poster_path: None,
                backdrop_path: None,
                production_countries: Vec::new(),
                genres: Vec::new(),
                production_companies: Vec::new(),
                external_ids: None,
                images: None,
                vote_average: None,
                vote_count: None,
            },
            "zh-CN",
        );

        assert_eq!(metadata.poster_path, None);
        assert_eq!(metadata.backdrop_path, None);
    }

    #[test]
    fn simplified_chinese_metadata_prefers_english_logo_over_ambiguous_chinese_logo() {
        let images = TmdbImagesResponse {
            logos: vec![
                TmdbLogo {
                    file_path: Some("/zh.png".to_string()),
                    iso_639_1: Some("zh".to_string()),
                    width: Some(1000),
                    height: Some(400),
                    vote_average: Some(10.0),
                    vote_count: Some(20),
                },
                TmdbLogo {
                    file_path: Some("/en.png".to_string()),
                    iso_639_1: Some("en".to_string()),
                    width: Some(800),
                    height: Some(300),
                    vote_average: Some(5.0),
                    vote_count: Some(1),
                },
            ],
        };

        assert_eq!(select_tmdb_logo(Some(&images), "zh-CN"), Some("/en.png"));
        assert_eq!(tmdb_image_languages("zh-CN"), "en,null,zh");
        assert_eq!(tmdb_image_languages("ja-JP"), "ja,en,null");

        let unsupported_images = TmdbImagesResponse {
            logos: vec![TmdbLogo {
                file_path: Some("/fr.png".to_string()),
                iso_639_1: Some("fr".to_string()),
                width: Some(800),
                height: Some(300),
                vote_average: Some(9.0),
                vote_count: Some(10),
            }],
        };
        assert_eq!(select_tmdb_logo(Some(&unsupported_images), "zh-CN"), None);
    }

    #[test]
    fn logo_selection_uses_votes_then_resolution_within_the_same_language() {
        let images = TmdbImagesResponse {
            logos: vec![
                TmdbLogo {
                    file_path: Some("/small.png".to_string()),
                    iso_639_1: Some("en".to_string()),
                    width: Some(400),
                    height: Some(100),
                    vote_average: Some(8.0),
                    vote_count: Some(3),
                },
                TmdbLogo {
                    file_path: Some("/large.png".to_string()),
                    iso_639_1: Some("en".to_string()),
                    width: Some(1200),
                    height: Some(300),
                    vote_average: Some(8.0),
                    vote_count: Some(3),
                },
            ],
        };

        assert_eq!(select_tmdb_logo(Some(&images), "en-US"), Some("/large.png"));
    }

    #[test]
    fn tmdb_ratings_require_real_votes_and_keep_native_scale() {
        assert!(tmdb_ratings(Some(0.0), Some(0)).is_empty());

        let ratings = tmdb_ratings(Some(8.4), Some(12_345));

        assert_eq!(ratings.len(), 1);
        assert_eq!(ratings[0].source, "tmdb");
        assert_eq!(ratings[0].kind, "audience");
        assert_eq!(ratings[0].score, 8.4);
        assert_eq!(ratings[0].scale, 10.0);
        assert_eq!(ratings[0].rating_count, Some(12_345));
    }

    #[test]
    fn tmdb_external_ids_keeps_all_supported_namespaces() {
        let external_ids = tmdb_external_ids(
            88,
            Some(&TmdbExternalIds {
                imdb_id: Some(" tt1234567 ".to_string()),
                tvdb_id: Some(765_432),
                wikidata_id: Some("Q123".to_string()),
                facebook_id: Some("movie-page".to_string()),
                instagram_id: Some("movie_account".to_string()),
                twitter_id: Some("movie_account".to_string()),
            }),
        );

        assert_eq!(
            external_ids,
            vec![
                MediaExternalId {
                    provider: "tmdb".to_string(),
                    external_id: "88".to_string(),
                },
                MediaExternalId {
                    provider: "imdb".to_string(),
                    external_id: "tt1234567".to_string(),
                },
                MediaExternalId {
                    provider: "tvdb".to_string(),
                    external_id: "765432".to_string(),
                },
                MediaExternalId {
                    provider: "wikidata".to_string(),
                    external_id: "Q123".to_string(),
                },
                MediaExternalId {
                    provider: "facebook".to_string(),
                    external_id: "movie-page".to_string(),
                },
                MediaExternalId {
                    provider: "instagram".to_string(),
                    external_id: "movie_account".to_string(),
                },
                MediaExternalId {
                    provider: "twitter".to_string(),
                    external_id: "movie_account".to_string(),
                },
            ]
        );
    }

    #[test]
    fn format_country_codes_joins_unique_values() {
        assert_eq!(
            format_country_codes(&["JP".to_string(), "US".to_string(), "JP".to_string()]),
            Some("JP · US".to_string())
        );
    }

    #[test]
    fn strict_title_match_accepts_numbered_main_title_with_explicit_subtitle() {
        assert!(titles_match_numbered_subtitle(
            "东北恋哥3",
            "东北恋哥3：冬天里的一把火",
        ));
        assert!(titles_match_numbered_subtitle(
            "Northeastern Bro 3",
            "Northeastern Bro 3: A Fire in Winter",
        ));
        assert!(!titles_match_numbered_subtitle("Dune", "Dune: Part Two"));
        assert!(!titles_match_numbered_subtitle(
            "东北恋哥3",
            "东北恋哥30：冬天里的一把火"
        ));
    }

    #[test]
    fn exact_original_title_outranks_numbered_subtitle_compatibility() {
        let candidates = vec![
            TmdbMovieSearchResult {
                id: 324_552,
                title: Some("疾速追杀2".to_string()),
                original_title: Some("John Wick: Chapter 2".to_string()),
                release_date: Some("2017-02-08".to_string()),
                overview: None,
                poster_path: Some("/john-wick-2.jpg".to_string()),
                backdrop_path: None,
            },
            TmdbMovieSearchResult {
                id: 651_445,
                title: Some("John Wick Chapter 2: Wick-vizzed".to_string()),
                original_title: Some("John Wick Chapter 2: Wick-vizzed".to_string()),
                release_date: Some("2017-06-13".to_string()),
                overview: None,
                poster_path: Some("/wick-vizzed.jpg".to_string()),
                backdrop_path: None,
            },
        ];

        let strongest =
            strongest_direct_title_matches("John Wick Chapter 2", candidates.iter().collect());

        assert_eq!(strongest.len(), 1);
        assert_eq!(strongest[0].id, 324_552);
        assert_eq!(
            select_strict_candidate(Some(2017), strongest).map(|candidate| candidate.id),
            Some(324_552)
        );
    }

    #[test]
    fn strict_match_prefers_original_title_over_localized_title_only() {
        let candidates = vec![
            TmdbMovieSearchResult {
                id: 1_395_515,
                title: Some("奇遇".to_string()),
                original_title: Some("奇遇".to_string()),
                release_date: Some("2025-08-08".to_string()),
                overview: None,
                poster_path: Some("/china-poster.jpg".to_string()),
                backdrop_path: None,
            },
            TmdbMovieSearchResult {
                id: 1_317_616,
                title: Some("奇遇".to_string()),
                original_title: Some("L'Aventura".to_string()),
                release_date: Some("2025-07-02".to_string()),
                overview: None,
                poster_path: Some("/france-poster.jpg".to_string()),
                backdrop_path: None,
            },
        ];

        let prioritized = strongest_direct_title_matches("奇遇", candidates.iter().collect());

        assert_eq!(prioritized.len(), 1);
        assert_eq!(prioritized[0].id, 1_395_515);
        assert_eq!(
            select_strict_candidate(Some(2025), prioritized).map(|candidate| candidate.id),
            Some(1_395_515)
        );
    }

    #[test]
    fn strict_match_rejects_multiple_original_title_matches() {
        let candidates = vec![
            TmdbMovieSearchResult {
                id: 1,
                title: Some("Same Title".to_string()),
                original_title: Some("Same Title".to_string()),
                release_date: Some("2025-01-01".to_string()),
                overview: None,
                poster_path: None,
                backdrop_path: None,
            },
            TmdbMovieSearchResult {
                id: 2,
                title: Some("Same Title".to_string()),
                original_title: Some("Same Title".to_string()),
                release_date: Some("2025-08-01".to_string()),
                overview: None,
                poster_path: None,
                backdrop_path: None,
            },
        ];

        let prioritized = strongest_direct_title_matches("Same Title", candidates.iter().collect());

        assert_eq!(prioritized.len(), 2);
        assert!(select_strict_candidate(Some(2025), prioritized).is_none());
    }

    #[test]
    fn strict_match_requires_exact_title_and_year() {
        let candidates = vec![
            TmdbMovieSearchResult {
                id: 1,
                title: Some("Castle in the Sky".to_string()),
                original_title: Some("Tenkū no Shiro Rapyuta".to_string()),
                release_date: Some("1986-08-02".to_string()),
                overview: None,
                poster_path: None,
                backdrop_path: None,
            },
            TmdbMovieSearchResult {
                id: 2,
                title: Some("Castle in the Sky".to_string()),
                original_title: Some("Laputa".to_string()),
                release_date: Some("1988-01-01".to_string()),
                overview: None,
                poster_path: None,
                backdrop_path: None,
            },
        ];

        let eligible_candidates = candidates
            .iter()
            .filter(|candidate| {
                candidate
                    .release_date
                    .as_deref()
                    .is_some_and(|date| date.starts_with("1986"))
            })
            .collect::<Vec<_>>();
        let exact_candidates =
            strongest_direct_title_matches("Castle in the Sky", eligible_candidates);
        let best_match = select_strict_candidate(Some(1986), exact_candidates).unwrap();

        assert_eq!(best_match.id, 1);
    }

    #[test]
    fn strict_match_without_year_selects_newest_exact_title() {
        let candidates = vec![
            TmdbMovieSearchResult {
                id: 1,
                title: Some("Dune".to_string()),
                original_title: None,
                release_date: Some("1984-12-14".to_string()),
                overview: None,
                poster_path: None,
                backdrop_path: None,
            },
            TmdbMovieSearchResult {
                id: 2,
                title: Some("Dune".to_string()),
                original_title: None,
                release_date: Some("2021-10-22".to_string()),
                overview: None,
                poster_path: None,
                backdrop_path: None,
            },
        ];

        let best_match = select_strict_candidate(None, candidates.iter().collect()).unwrap();

        assert_eq!(best_match.id, 2);
    }

    #[test]
    fn strict_match_without_year_rejects_tied_latest_candidates() {
        let candidates = vec![
            TmdbMovieSearchResult {
                id: 1,
                title: Some("Same".to_string()),
                original_title: None,
                release_date: Some("2025-01-01".to_string()),
                overview: None,
                poster_path: None,
                backdrop_path: None,
            },
            TmdbMovieSearchResult {
                id: 2,
                title: Some("Same".to_string()),
                original_title: None,
                release_date: Some("2025-01-01".to_string()),
                overview: None,
                poster_path: None,
                backdrop_path: None,
            },
        ];

        assert!(select_strict_candidate(None, candidates.iter().collect()).is_none());
    }

    #[test]
    fn strict_tv_match_with_season_year_requires_one_verified_candidate() {
        let lookup = MetadataLookup {
            title: "Fallout".to_string(),
            year: None,
            season_air_year: Some(MetadataSeasonAirYearHint {
                season_number: 2,
                year: 2025,
            }),
            library_type: "series".to_string(),
            language: Some("zh-CN".to_string()),
            provider_item_id: None,
        };
        let candidates = [
            TmdbTvSearchResult {
                id: 1,
                name: Some("Fallout".to_string()),
                original_name: None,
                first_air_date: Some("2024-01-01".to_string()),
                overview: None,
                poster_path: None,
                backdrop_path: None,
            },
            TmdbTvSearchResult {
                id: 2,
                name: Some("Fallout".to_string()),
                original_name: None,
                first_air_date: Some("2020-01-01".to_string()),
                overview: None,
                poster_path: None,
                backdrop_path: None,
            },
        ];

        assert!(select_strict_tv_candidate(&lookup, candidates.iter().collect()).is_none());
        assert_eq!(
            select_strict_tv_candidate(&lookup, vec![&candidates[0]]).map(|item| item.id),
            Some(1)
        );
    }

    #[test]
    fn deduplicate_search_results_keeps_first_occurrence() {
        let deduplicated = deduplicate_search_results(
            vec![
                TmdbMovieSearchResult {
                    id: 2,
                    title: Some("The Legend of the Condor Heroes".to_string()),
                    original_title: None,
                    release_date: Some("1994-01-01".to_string()),
                    overview: None,
                    poster_path: None,
                    backdrop_path: None,
                },
                TmdbMovieSearchResult {
                    id: 3,
                    title: Some("Another Result".to_string()),
                    original_title: None,
                    release_date: Some("1995-01-01".to_string()),
                    overview: None,
                    poster_path: None,
                    backdrop_path: None,
                },
                TmdbMovieSearchResult {
                    id: 2,
                    title: Some("The Legend of the Condor Heroes".to_string()),
                    original_title: None,
                    release_date: Some("1994-01-01".to_string()),
                    overview: None,
                    poster_path: None,
                    backdrop_path: None,
                },
                TmdbMovieSearchResult {
                    id: 1,
                    title: Some("The Legend of the Condor Heroes".to_string()),
                    original_title: None,
                    release_date: Some("1983-01-01".to_string()),
                    overview: None,
                    poster_path: None,
                    backdrop_path: None,
                },
            ],
            |result| result.id,
        );

        let result_ids = deduplicated
            .into_iter()
            .map(|result| result.id)
            .collect::<Vec<_>>();

        assert_eq!(result_ids, vec![2, 3, 1]);
    }

    #[test]
    fn parse_year_extracts_first_four_digits() {
        assert_eq!(parse_year(Some("2001-07-20")), Some(2001));
        assert_eq!(parse_year(Some("1999")), Some(1999));
        assert_eq!(parse_year(Some("  ")), None);
    }

    #[test]
    fn tv_search_year_filter_distinguishes_series_and_season_years() {
        let mut lookup = MetadataLookup {
            title: "Fallout".to_string(),
            year: Some(2024),
            season_air_year: Some(MetadataSeasonAirYearHint {
                season_number: 2,
                year: 2025,
            }),
            library_type: "series".to_string(),
            language: Some("zh-CN".to_string()),
            provider_item_id: None,
        };

        assert_eq!(
            tv_search_year_filter(&lookup),
            Some(("first_air_date_year", 2024))
        );

        lookup.year = None;
        assert_eq!(tv_search_year_filter(&lookup), Some(("year", 2025)));
    }

    #[test]
    fn tmdb_season_year_validation_accepts_season_or_episode_air_date() {
        let season_date = TmdbTvSeasonDetails {
            name: None,
            air_date: Some("2025-10-01".to_string()),
            overview: None,
            poster_path: None,
            episodes: Vec::new(),
        };
        assert!(tmdb_tv_season_matches_air_year(&season_date, 2025));

        let episode_date = TmdbTvSeasonDetails {
            name: None,
            air_date: None,
            overview: None,
            poster_path: None,
            episodes: vec![TmdbTvEpisodeDetails {
                episode_number: 1,
                name: None,
                air_date: Some("2025-10-01".to_string()),
                overview: None,
                still_path: None,
            }],
        };
        assert!(tmdb_tv_season_matches_air_year(&episode_date, 2025));
        assert!(!tmdb_tv_season_matches_air_year(&episode_date, 2024));
    }

    #[test]
    fn normalize_title_drops_punctuation_and_lowercases() {
        assert_eq!(normalize_title("My.Movie: Part-1"), "my movie part 1");
        assert_eq!(normalize_title("All's Fair"), "alls fair");
        assert_eq!(normalize_title("Alls Fair"), "alls fair");
        assert_eq!(normalize_title("向阳·花"), "向阳花");
        assert_eq!(normalize_title("新・驯龙高手"), "新驯龙高手");
        assert_eq!(normalize_title("新•驯龙高手"), "新驯龙高手");
        assert_eq!(normalize_title("Ca$hero"), "cashero");
        assert_eq!(normalize_title("$100"), "100");
    }

    #[test]
    fn strict_match_accepts_intra_word_dollar_as_stylized_s() {
        assert!(titles_match_exactly("Cashero", "Ca$hero"));
        assert!(!titles_match_exactly("S100", "$100"));
        assert!(!titles_match_exactly("No Where", "Nowhere"));
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
            season_air_year: None,
            library_type: "movie".to_string(),
            language: Some("zh-CN".to_string()),
            provider_item_id: None,
        };
        let series_lookup = MetadataLookup {
            title: "Dune".to_string(),
            year: Some(2021),
            season_air_year: None,
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
