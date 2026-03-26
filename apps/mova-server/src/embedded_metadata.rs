use anyhow::Result;
use mova_application::{
    normalize_base_url, normalize_optional_value, normalize_required_value, MetadataProviderConfig,
    TmdbMetadataProviderConfig, DEFAULT_TMDB_API_BASE_URL, DEFAULT_TMDB_IMAGE_BASE_URL,
    DEFAULT_TMDB_LANGUAGE,
};

/// 当前直接编译进程序的 TMDB 配置。
/// 如果未来要更换 token，只改这里，然后重新 `docker compose up -d --build`。
///
/// 注意：把真实 token 写进源码后，只适合你自己控制的私有构建流程。
/// 如果仓库将来要公开，这里必须换掉，不要把真实 token 提交到公开仓库。
const TMDB_ACCESS_TOKEN: Option<&str> = Some(
    "eyJhbGciOiJIUzI1NiJ9.eyJhdWQiOiI2MmU3MTNiNmNmMGFjYzc5MTE4ZTcxZGExMDkxYWU5ZCIsIm5iZiI6MTc3Mzc5OTMyNC4wODE5OTk4LCJzdWIiOiI2OWJhMDc5YzNkMjFlMWVhMDIzYmE4ZWUiLCJzY29wZXMiOlsiYXBpX3JlYWQiXSwidmVyc2lvbiI6MX0.6JXfEnfohnrVEOWThPgbKdKINgwUU7NFlaZFbfqXVg8",
);
const TMDB_LANGUAGE: &str = DEFAULT_TMDB_LANGUAGE;
const TMDB_API_BASE_URL: &str = DEFAULT_TMDB_API_BASE_URL;
const TMDB_IMAGE_BASE_URL: &str = DEFAULT_TMDB_IMAGE_BASE_URL;

pub fn metadata_provider_config() -> Result<MetadataProviderConfig> {
    let Some(access_token) = normalize_optional_value(TMDB_ACCESS_TOKEN.map(str::to_string)) else {
        return Ok(MetadataProviderConfig::Disabled);
    };

    Ok(MetadataProviderConfig::Tmdb(TmdbMetadataProviderConfig {
        access_token,
        language: normalize_required_value("tmdb language", TMDB_LANGUAGE.to_string())?,
        api_base_url: normalize_base_url("tmdb api base url", TMDB_API_BASE_URL.to_string())?,
        image_base_url: normalize_base_url("tmdb image base url", TMDB_IMAGE_BASE_URL.to_string())?,
    }))
}

#[cfg(test)]
mod tests {
    use super::{metadata_provider_config, TMDB_ACCESS_TOKEN};
    use mova_application::MetadataProviderConfig;

    #[test]
    fn metadata_provider_is_disabled_without_embedded_token() {
        if TMDB_ACCESS_TOKEN.is_some() {
            return;
        }

        let config = metadata_provider_config().unwrap();
        assert!(matches!(config, MetadataProviderConfig::Disabled));
    }
}
