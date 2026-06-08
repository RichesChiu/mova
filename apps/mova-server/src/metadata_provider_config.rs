use anyhow::Result;
use mova_application::{
    normalize_base_url, normalize_optional_value, normalize_required_value, MetadataProviderConfig,
    TmdbMetadataProviderConfig, DEFAULT_OMDB_API_BASE_URL, DEFAULT_TMDB_API_BASE_URL,
    DEFAULT_TMDB_IMAGE_BASE_URL, DEFAULT_TMDB_LANGUAGE,
};
use std::env;

const MOVA_OMDB_API_BASE_URL: &str = "MOVA_OMDB_API_BASE_URL";
const MOVA_OMDB_API_KEY: &str = "MOVA_OMDB_API_KEY";
const MOVA_TMDB_ACCESS_TOKEN: &str = "MOVA_TMDB_ACCESS_TOKEN";
const MOVA_TMDB_LANGUAGE: &str = "MOVA_TMDB_LANGUAGE";
const MOVA_TMDB_API_BASE_URL: &str = "MOVA_TMDB_API_BASE_URL";
const MOVA_TMDB_IMAGE_BASE_URL: &str = "MOVA_TMDB_IMAGE_BASE_URL";

/// TMDB provider 现在完全走运行时环境变量，避免再把真实 token 编译进公开源码或镜像。
/// 如果没有配置 token，会自动把远端 metadata provider 视为 disabled。
pub fn metadata_provider_config_from_env() -> Result<MetadataProviderConfig> {
    let Some(access_token) = normalize_optional_value(env::var(MOVA_TMDB_ACCESS_TOKEN).ok()) else {
        return Ok(MetadataProviderConfig::Disabled);
    };

    Ok(MetadataProviderConfig::Tmdb(TmdbMetadataProviderConfig {
        access_token,
        language: normalize_required_value(
            "tmdb language",
            env::var(MOVA_TMDB_LANGUAGE).unwrap_or_else(|_| DEFAULT_TMDB_LANGUAGE.to_string()),
        )?,
        api_base_url: normalize_base_url(
            "tmdb api base url",
            env::var(MOVA_TMDB_API_BASE_URL)
                .unwrap_or_else(|_| DEFAULT_TMDB_API_BASE_URL.to_string()),
        )?,
        image_base_url: normalize_base_url(
            "tmdb image base url",
            env::var(MOVA_TMDB_IMAGE_BASE_URL)
                .unwrap_or_else(|_| DEFAULT_TMDB_IMAGE_BASE_URL.to_string()),
        )?,
        omdb_api_key: normalize_optional_value(env::var(MOVA_OMDB_API_KEY).ok()),
        omdb_api_base_url: normalize_base_url(
            "omdb api base url",
            env::var(MOVA_OMDB_API_BASE_URL)
                .unwrap_or_else(|_| DEFAULT_OMDB_API_BASE_URL.to_string()),
        )?,
    }))
}

#[cfg(test)]
mod tests {
    use super::{
        metadata_provider_config_from_env, MOVA_OMDB_API_BASE_URL, MOVA_OMDB_API_KEY,
        MOVA_TMDB_ACCESS_TOKEN, MOVA_TMDB_API_BASE_URL, MOVA_TMDB_IMAGE_BASE_URL,
        MOVA_TMDB_LANGUAGE,
    };
    use mova_application::MetadataProviderConfig;
    use std::{
        env,
        sync::{Mutex, OnceLock},
    };

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn metadata_provider_is_disabled_without_tmdb_token() {
        let _guard = env_lock().lock().unwrap();
        unsafe {
            env::remove_var(MOVA_TMDB_ACCESS_TOKEN);
            env::remove_var(MOVA_TMDB_LANGUAGE);
            env::remove_var(MOVA_TMDB_API_BASE_URL);
            env::remove_var(MOVA_TMDB_IMAGE_BASE_URL);
            env::remove_var(MOVA_OMDB_API_KEY);
            env::remove_var(MOVA_OMDB_API_BASE_URL);
        }

        let config = metadata_provider_config_from_env().unwrap();
        assert!(matches!(config, MetadataProviderConfig::Disabled));
    }

    #[test]
    fn metadata_provider_uses_env_tmdb_token() {
        let _guard = env_lock().lock().unwrap();
        unsafe {
            env::set_var(MOVA_TMDB_ACCESS_TOKEN, " token ");
            env::set_var(MOVA_TMDB_LANGUAGE, "en-US");
            env::set_var(MOVA_TMDB_API_BASE_URL, "https://api.themoviedb.org/3/");
            env::set_var(
                MOVA_TMDB_IMAGE_BASE_URL,
                "https://image.tmdb.org/t/p/original/",
            );
            env::set_var(MOVA_OMDB_API_KEY, " omdb-key ");
            env::set_var(MOVA_OMDB_API_BASE_URL, "https://www.omdbapi.com/");
        }

        let config = metadata_provider_config_from_env().unwrap();
        let MetadataProviderConfig::Tmdb(config) = config else {
            panic!("expected tmdb metadata provider config");
        };

        assert_eq!(config.access_token, "token");
        assert_eq!(config.language, "en-US");
        assert_eq!(config.api_base_url, "https://api.themoviedb.org/3");
        assert_eq!(config.image_base_url, "https://image.tmdb.org/t/p/original");
        assert_eq!(config.omdb_api_key.as_deref(), Some("omdb-key"));
        assert_eq!(config.omdb_api_base_url, "https://www.omdbapi.com");
    }
}
