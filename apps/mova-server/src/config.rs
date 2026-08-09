use crate::{auth_rate_limit::AuthRateLimitSettings, metadata_provider_config};
use anyhow::{anyhow, Context, Result};
use mova_application::{MetadataProviderConfig, StrmStreamingConfig};
use mova_db::DatabaseSettings;
use std::{env, net::SocketAddr, path::PathBuf};
use time::UtcOffset;

#[derive(Debug, Clone, Copy)]
pub struct ApiTimeSettings {
    pub offset: UtcOffset,
}

#[derive(Debug)]
pub struct AppConfig {
    host: String,
    port: u16,
    pub database: DatabaseSettings,
    pub api_time: ApiTimeSettings,
    pub build_version: String,
    pub cache_dir: PathBuf,
    pub web_dist_dir: Option<PathBuf>,
    pub metadata_provider: MetadataProviderConfig,
    pub session_cookie_secure: bool,
    pub auth_rate_limit: AuthRateLimitSettings,
    pub worker_concurrency: usize,
    pub strm_streaming: StrmStreamingConfig,
}

impl AppConfig {
    /// 从环境变量收集 HTTP 监听配置和数据库配置。
    pub fn from_env() -> Result<Self> {
        let host = env::var("MOVA_HTTP_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
        let port = env::var("MOVA_HTTP_PORT")
            .ok()
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(36080);
        let build_version = option_env!("MOVA_BUILD_VERSION")
            .unwrap_or(env!("CARGO_PKG_VERSION"))
            .to_string();
        let allowed_strm_hosts = env::var("MOVA_STRM_ALLOWED_HOSTS").ok();
        let strm_streaming =
            StrmStreamingConfig::new(&build_version, allowed_strm_hosts.as_deref())
                .map_err(|message| anyhow!(message))?;

        Ok(Self {
            host,
            port,
            database: DatabaseSettings::from_env()?,
            api_time: ApiTimeSettings::from_env()?,
            build_version,
            cache_dir: cache_dir_from_env()?,
            web_dist_dir: web_dist_dir_from_env()?,
            metadata_provider: metadata_provider_config::metadata_provider_config_from_env()?,
            session_cookie_secure: parse_boolean_env("MOVA_SESSION_COOKIE_SECURE", false)?,
            auth_rate_limit: auth_rate_limit_settings_from_env()?,
            worker_concurrency: env::var("MOVA_WORKER_CONCURRENCY")
                .ok()
                .and_then(|value| value.parse::<usize>().ok())
                .filter(|value| *value > 0)
                .unwrap_or(2),
            strm_streaming,
        })
    }

    /// 把配置里的 host/port 组合成 Axum 最终绑定的监听地址。
    pub fn socket_addr(&self) -> Result<SocketAddr> {
        Ok(format!("{}:{}", self.host, self.port).parse()?)
    }
}

fn auth_rate_limit_settings_from_env() -> Result<AuthRateLimitSettings> {
    let defaults = AuthRateLimitSettings::default();

    Ok(AuthRateLimitSettings {
        max_failures: parse_positive_env(
            "MOVA_AUTH_RATE_LIMIT_MAX_FAILURES",
            defaults.max_failures,
        )?,
        window: std::time::Duration::from_secs(parse_positive_env(
            "MOVA_AUTH_RATE_LIMIT_WINDOW_SECONDS",
            defaults.window.as_secs(),
        )?),
        lockout: std::time::Duration::from_secs(parse_positive_env(
            "MOVA_AUTH_RATE_LIMIT_LOCKOUT_SECONDS",
            defaults.lockout.as_secs(),
        )?),
        max_keys: parse_positive_env("MOVA_AUTH_RATE_LIMIT_MAX_KEYS", defaults.max_keys)?,
    })
}

fn parse_positive_env<T>(name: &str, default: T) -> Result<T>
where
    T: std::str::FromStr + Copy + PartialOrd + From<u8>,
    T::Err: std::fmt::Display,
{
    let Some(value) = env::var(name).ok() else {
        return Ok(default);
    };
    let value = value
        .trim()
        .parse::<T>()
        .map_err(|error| anyhow!("invalid {name} value: {error}"))?;
    if value <= T::from(0) {
        return Err(anyhow!("{name} must be greater than zero"));
    }

    Ok(value)
}

fn parse_boolean_env(name: &str, default: bool) -> Result<bool> {
    let Some(value) = env::var(name).ok() else {
        return Ok(default);
    };

    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => Err(anyhow!(
            "{name} must be one of true, false, 1, 0, yes, no, on, or off"
        )),
    }
}

impl ApiTimeSettings {
    /// 读取接口响应使用的时区配置。
    /// 默认使用中国大陆常见的 `Asia/Shanghai`，也支持 `UTC` 和固定偏移如 `+08:00`。
    pub fn from_env() -> Result<Self> {
        let timezone = env::var("MOVA_TIMEZONE").unwrap_or_else(|_| "Asia/Shanghai".to_string());

        Ok(Self {
            offset: parse_timezone_offset(&timezone)
                .with_context(|| format!("invalid MOVA_TIMEZONE value: {}", timezone))?,
        })
    }
}

fn cache_dir_from_env() -> Result<PathBuf> {
    let configured = env::var("MOVA_CACHE_DIR").unwrap_or_else(|_| "./data/cache".to_string());
    let path = PathBuf::from(configured.trim());

    if path.as_os_str().is_empty() {
        return Err(anyhow!("MOVA_CACHE_DIR cannot be empty"));
    }

    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(env::current_dir()
            .context("failed to resolve current working directory for MOVA_CACHE_DIR")?
            .join(path))
    }
}

fn web_dist_dir_from_env() -> Result<Option<PathBuf>> {
    let configured =
        env::var("MOVA_WEB_DIST_DIR").unwrap_or_else(|_| "./apps/mova-web/dist".to_string());
    let trimmed = configured.trim();

    if trimmed.is_empty() {
        return Ok(None);
    }

    let path = PathBuf::from(trimmed);
    let resolved = if path.is_absolute() {
        path
    } else {
        env::current_dir()
            .context("failed to resolve current working directory for MOVA_WEB_DIST_DIR")?
            .join(path)
    };

    if resolved.is_dir() {
        Ok(Some(resolved))
    } else {
        Ok(None)
    }
}

fn parse_timezone_offset(value: &str) -> Result<UtcOffset> {
    let normalized = value.trim();

    match normalized {
        "UTC" | "Etc/UTC" | "Z" => Ok(UtcOffset::UTC),
        "Asia/Shanghai" | "PRC" => UtcOffset::from_hms(8, 0, 0).map_err(|error| anyhow!(error)),
        _ => parse_fixed_offset(normalized),
    }
}

fn parse_fixed_offset(value: &str) -> Result<UtcOffset> {
    let (sign, digits) = match value.as_bytes().first().copied() {
        Some(b'+') => (1, &value[1..]),
        Some(b'-') => (-1, &value[1..]),
        _ => {
            return Err(anyhow!(
                "expected `Asia/Shanghai`, `UTC`, or a fixed offset such as `+08:00`"
            ));
        }
    };

    let parts = digits.split(':').collect::<Vec<_>>();
    let (hours, minutes, seconds) = match parts.as_slice() {
        [hours] => parse_compact_offset(hours)?,
        [hours, minutes] => (parse_offset_part(hours)?, parse_offset_part(minutes)?, 0),
        [hours, minutes, seconds] => (
            parse_offset_part(hours)?,
            parse_offset_part(minutes)?,
            parse_offset_part(seconds)?,
        ),
        _ => {
            return Err(anyhow!(
                "expected fixed offset in `+HH`, `+HHMM`, `+HH:MM`, or `+HH:MM:SS` format"
            ));
        }
    };

    UtcOffset::from_hms(sign * hours, sign * minutes, sign * seconds)
        .map_err(|error| anyhow!(error))
}

fn parse_compact_offset(value: &str) -> Result<(i8, i8, i8)> {
    match value.len() {
        2 => Ok((parse_offset_part(value)?, 0, 0)),
        4 => Ok((
            parse_offset_part(&value[..2])?,
            parse_offset_part(&value[2..])?,
            0,
        )),
        6 => Ok((
            parse_offset_part(&value[..2])?,
            parse_offset_part(&value[2..4])?,
            parse_offset_part(&value[4..])?,
        )),
        _ => Err(anyhow!(
            "expected compact fixed offset in `+HH`, `+HHMM`, or `+HHMMSS` format"
        )),
    }
}

fn parse_offset_part(value: &str) -> Result<i8> {
    value
        .parse::<i8>()
        .with_context(|| format!("invalid offset component: {}", value))
}

#[cfg(test)]
mod tests {
    use super::{
        cache_dir_from_env, parse_boolean_env, parse_positive_env, parse_timezone_offset,
        web_dist_dir_from_env,
    };
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn parse_timezone_offset_supports_asia_shanghai_alias() {
        let offset = parse_timezone_offset("Asia/Shanghai").unwrap();

        assert_eq!(offset.whole_hours(), 8);
        assert_eq!(offset.minutes_past_hour(), 0);
    }

    #[test]
    fn parse_timezone_offset_supports_utc() {
        let offset = parse_timezone_offset("UTC").unwrap();

        assert_eq!(offset.whole_hours(), 0);
    }

    #[test]
    fn parse_timezone_offset_supports_fixed_offset() {
        let offset = parse_timezone_offset("+08:30").unwrap();

        assert_eq!(offset.whole_hours(), 8);
        assert_eq!(offset.minutes_past_hour(), 30);
    }

    #[test]
    fn cache_dir_from_env_defaults_to_workspace_data_cache_directory() {
        let _guard = env_lock().lock().unwrap();
        unsafe {
            std::env::remove_var("MOVA_CACHE_DIR");
        }

        let path = cache_dir_from_env().unwrap();
        assert!(path.ends_with("data/cache"));
    }

    #[test]
    fn web_dist_dir_from_env_returns_none_when_directory_is_missing() {
        let _guard = env_lock().lock().unwrap();
        unsafe {
            std::env::set_var("MOVA_WEB_DIST_DIR", "./definitely-missing-web-dist");
        }

        let path = web_dist_dir_from_env().unwrap();
        assert!(path.is_none());

        unsafe {
            std::env::remove_var("MOVA_WEB_DIST_DIR");
        }
    }

    #[test]
    fn boolean_environment_values_are_strict() {
        let _guard = env_lock().lock().unwrap();
        unsafe {
            std::env::set_var("MOVA_TEST_BOOLEAN", "yes");
        }
        assert!(parse_boolean_env("MOVA_TEST_BOOLEAN", false).unwrap());

        unsafe {
            std::env::set_var("MOVA_TEST_BOOLEAN", "sometimes");
        }
        assert!(parse_boolean_env("MOVA_TEST_BOOLEAN", false).is_err());

        unsafe {
            std::env::remove_var("MOVA_TEST_BOOLEAN");
        }
    }

    #[test]
    fn positive_environment_values_reject_zero() {
        let _guard = env_lock().lock().unwrap();
        unsafe {
            std::env::set_var("MOVA_TEST_POSITIVE", "0");
        }
        let result = parse_positive_env::<u32>("MOVA_TEST_POSITIVE", 5);
        assert!(result.is_err());

        unsafe {
            std::env::remove_var("MOVA_TEST_POSITIVE");
        }
    }
}
