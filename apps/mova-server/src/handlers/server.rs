use crate::auth::require_admin;
use crate::{error::ApiError, state::AppState};
use axum::extract::State;
use axum::Json;
use axum_extra::extract::cookie::CookieJar;
use serde::Serialize;
use std::{collections::HashSet, env, fs, path::PathBuf};

const ROOT_PATH_ENV_KEY: &str = "MOVA_LIBRARY_ROOTS";
const LEGACY_ROOT_PATH_ENV_KEY: &str = "MOVA_MEDIA_ROOT";
const COMPOSE_FILE_ENV_KEY: &str = "MOVA_COMPOSE_FILE";

#[derive(Debug, Clone, Serialize)]
pub struct RootPathOptionResponse {
    pub path: String,
    pub source: String,
}

/// 返回可用于创建媒体库的候选根目录。
/// 读取优先级：环境变量 > docker compose 配置 > 运行时挂载点。
pub async fn list_root_paths(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<Vec<RootPathOptionResponse>>, ApiError> {
    require_admin(&state, &jar).await?;

    Ok(Json(discover_root_path_options()))
}

fn discover_root_path_options() -> Vec<RootPathOptionResponse> {
    let mut options = Vec::new();
    let mut seen = HashSet::new();

    insert_options(
        &mut options,
        &mut seen,
        parse_root_paths_from_env(),
        "env".to_string(),
    );
    insert_options(
        &mut options,
        &mut seen,
        parse_root_paths_from_compose(),
        "compose".to_string(),
    );
    insert_options(
        &mut options,
        &mut seen,
        parse_root_paths_from_mountinfo(),
        "mount".to_string(),
    );

    options
}

fn insert_options(
    options: &mut Vec<RootPathOptionResponse>,
    seen: &mut HashSet<String>,
    paths: Vec<String>,
    source: String,
) {
    for path in paths {
        if !seen.insert(path.clone()) {
            continue;
        }

        options.push(RootPathOptionResponse {
            path,
            source: source.clone(),
        });
    }
}

fn parse_root_paths_from_env() -> Vec<String> {
    let mut paths = Vec::new();

    if let Ok(value) = env::var(ROOT_PATH_ENV_KEY) {
        paths.extend(parse_env_root_path_list(&value));
    }

    if let Ok(value) = env::var(LEGACY_ROOT_PATH_ENV_KEY) {
        paths.extend(parse_env_root_path_list(&value));
    }

    paths
}

fn parse_env_root_path_list(configured: &str) -> Vec<String> {
    configured
        .split(&[',', ';', '\n'][..])
        .filter_map(normalize_container_path)
        .collect()
}

fn parse_root_paths_from_compose() -> Vec<String> {
    let candidates = compose_file_candidates();
    let mut paths = Vec::new();

    for candidate in candidates {
        let content = match fs::read_to_string(&candidate) {
            Ok(content) => content,
            Err(_) => continue,
        };

        paths.extend(parse_compose_root_paths(&content));
    }

    paths
}

fn compose_file_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(configured_path) = env::var(COMPOSE_FILE_ENV_KEY) {
        let trimmed = configured_path.trim();
        if !trimmed.is_empty() {
            let path = PathBuf::from(trimmed);
            if path.is_absolute() {
                candidates.push(path);
            } else if let Ok(cwd) = env::current_dir() {
                candidates.push(cwd.join(path));
            }
        }
    }

    if let Ok(cwd) = env::current_dir() {
        candidates.push(cwd.join("docker-compose.yml"));
        candidates.push(cwd.join("compose.yml"));
    }

    candidates.push(PathBuf::from("/app/docker-compose.yml"));
    candidates.push(PathBuf::from("/app/compose.yml"));

    candidates
}

fn parse_compose_root_paths(content: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let mut in_services = false;
    let mut in_target_service = false;
    let mut in_volumes = false;

    for line in content.lines() {
        let no_comment = line.split('#').next().unwrap_or_default().trim_end();
        if no_comment.trim().is_empty() {
            continue;
        }

        let indent = no_comment
            .chars()
            .take_while(|character| *character == ' ')
            .count();
        let trimmed = no_comment.trim_start();

        if indent == 0 {
            in_services = trimmed == "services:";
            if !in_services {
                in_target_service = false;
                in_volumes = false;
            }
            continue;
        }

        if !in_services {
            continue;
        }

        if indent == 2 && trimmed.ends_with(':') {
            let service_name = trimmed.trim_end_matches(':').trim();
            in_target_service = service_name == "mova-server";
            in_volumes = false;
            continue;
        }

        if !in_target_service {
            continue;
        }

        if indent == 4 && trimmed == "volumes:" {
            in_volumes = true;
            continue;
        }

        if indent == 4 && trimmed.ends_with(':') && trimmed != "volumes:" {
            in_volumes = false;
            continue;
        }

        if !in_volumes {
            continue;
        }

        if indent >= 6 && trimmed.starts_with("- ") {
            let volume_spec = trimmed.trim_start_matches("- ").trim();
            if let Some(path) = parse_compose_short_volume_target(volume_spec) {
                paths.push(path);
            }
            continue;
        }

        if indent >= 8 && trimmed.starts_with("target:") {
            let target = trimmed.trim_start_matches("target:").trim();
            if let Some(path) = normalize_container_path(target) {
                paths.push(path);
            }
        }
    }

    paths
}

fn parse_compose_short_volume_target(spec: &str) -> Option<String> {
    let normalized = trim_wrapping_quotes(spec.trim());
    if normalized.is_empty() {
        return None;
    }

    if !normalized.contains(':') {
        return normalize_container_path(normalized);
    }

    for segment in normalized.split(':') {
        if let Some(path) = normalize_container_path(segment) {
            return Some(path);
        }
    }

    None
}

fn parse_root_paths_from_mountinfo() -> Vec<String> {
    let content = match fs::read_to_string("/proc/self/mountinfo") {
        Ok(content) => content,
        Err(_) => return Vec::new(),
    };

    let mut paths = Vec::new();

    for line in content.lines() {
        let left_side = match line.split_once(" - ") {
            Some((left, _)) => left,
            None => line,
        };
        let fields = left_side.split_whitespace().collect::<Vec<_>>();
        if fields.len() < 5 {
            continue;
        }

        let mount_point = decode_mountinfo_path(fields[4]);
        if !is_likely_library_mount(&mount_point) {
            continue;
        }

        if let Some(path) = normalize_container_path(&mount_point) {
            paths.push(path);
        }
    }

    paths
}

fn decode_mountinfo_path(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'\\' && index + 3 < bytes.len() {
            let a = bytes[index + 1];
            let b = bytes[index + 2];
            let c = bytes[index + 3];

            if is_octal_digit(a) && is_octal_digit(b) && is_octal_digit(c) {
                let value = (a - b'0') * 64 + (b - b'0') * 8 + (c - b'0');
                decoded.push(value);
                index += 4;
                continue;
            }
        }

        decoded.push(bytes[index]);
        index += 1;
    }

    String::from_utf8_lossy(&decoded).into_owned()
}

fn is_octal_digit(value: u8) -> bool {
    (b'0'..=b'7').contains(&value)
}

fn is_likely_library_mount(path: &str) -> bool {
    let normalized = path.trim();
    normalized == "/media"
        || normalized.starts_with("/media/")
        || normalized == "/mnt"
        || normalized.starts_with("/mnt/")
        || normalized == "/srv/media"
        || normalized.starts_with("/srv/media/")
        || normalized == "/data/media"
        || normalized.starts_with("/data/media/")
        || normalized == "/libraries"
        || normalized.starts_with("/libraries/")
}

fn normalize_container_path(raw: &str) -> Option<String> {
    let trimmed = trim_wrapping_quotes(raw.trim());
    if !trimmed.starts_with('/') {
        return None;
    }

    let mut normalized = trimmed.trim_end_matches('/').to_string();
    if normalized.is_empty() {
        normalized = "/".to_string();
    }

    if !is_allowed_library_path(&normalized) {
        return None;
    }

    Some(normalized)
}

fn is_allowed_library_path(path: &str) -> bool {
    if path == "/" {
        return false;
    }

    !(path == "/app/data/cache" || path.starts_with("/app/data/cache/"))
}

fn trim_wrapping_quotes(raw: &str) -> &str {
    raw.trim_matches(|character| character == '"' || character == '\'')
}

#[cfg(test)]
mod tests {
    use super::{
        decode_mountinfo_path, parse_compose_root_paths, parse_root_paths_from_env,
        LEGACY_ROOT_PATH_ENV_KEY, ROOT_PATH_ENV_KEY,
    };
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn parse_compose_short_volume_syntax() {
        let content = r#"
services:
  mova-server:
    volumes:
      - ./data/cache:/app/data/cache
      - ./dev-media:/media/dev-media:ro
"#;

        let paths = parse_compose_root_paths(content);

        assert_eq!(paths, vec!["/media/dev-media"]);
    }

    #[test]
    fn parse_compose_long_volume_syntax() {
        let content = r#"
services:
  mova-server:
    volumes:
      - type: bind
        source: ./dev-media
        target: /media/dev-media
"#;

        let paths = parse_compose_root_paths(content);

        assert_eq!(paths, vec!["/media/dev-media"]);
    }

    #[test]
    fn decode_mountinfo_path_supports_octal_escapes() {
        let decoded = decode_mountinfo_path("/media/dev\\040media");
        assert_eq!(decoded, "/media/dev media");
    }

    #[test]
    fn parse_root_paths_from_env_supports_multi_path_configuration() {
        let _guard = env_lock().lock().unwrap();
        unsafe {
            std::env::set_var(
                ROOT_PATH_ENV_KEY,
                "/media/movies;/media/anime\n/libraries/tv",
            );
            std::env::remove_var(LEGACY_ROOT_PATH_ENV_KEY);
        }

        let paths = parse_root_paths_from_env();

        assert_eq!(
            paths,
            vec!["/media/movies", "/media/anime", "/libraries/tv",]
        );

        unsafe {
            std::env::remove_var(ROOT_PATH_ENV_KEY);
            std::env::remove_var(LEGACY_ROOT_PATH_ENV_KEY);
        }
    }

    #[test]
    fn parse_root_paths_from_env_supports_legacy_single_path_key() {
        let _guard = env_lock().lock().unwrap();
        unsafe {
            std::env::remove_var(ROOT_PATH_ENV_KEY);
            std::env::set_var(LEGACY_ROOT_PATH_ENV_KEY, "/media/legacy");
        }

        let paths = parse_root_paths_from_env();

        assert_eq!(paths, vec!["/media/legacy"]);

        unsafe {
            std::env::remove_var(ROOT_PATH_ENV_KEY);
            std::env::remove_var(LEGACY_ROOT_PATH_ENV_KEY);
        }
    }
}
