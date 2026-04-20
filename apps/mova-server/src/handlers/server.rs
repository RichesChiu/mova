use crate::auth::require_admin;
use crate::{
    error::ApiError,
    response::{ok, ApiJson},
    state::AppState,
};
use axum::{extract::State, http::HeaderMap};
use axum_extra::extract::cookie::CookieJar;
use serde::Serialize;
use std::{fs, io::ErrorKind, path::Path};

const MEDIA_TREE_ROOT_PATH: &str = "/media";
const MEDIA_TREE_ROOT_NAME: &str = "media";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MediaDirectoryNodeResponse {
    pub name: String,
    pub path: String,
    pub children: Vec<MediaDirectoryNodeResponse>,
}

/// 返回容器内 `/media` 的递归目录树，供前端选择具体库源目录。
pub async fn get_media_tree(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
) -> Result<ApiJson<Option<MediaDirectoryNodeResponse>>, ApiError> {
    require_admin(&state, &headers, &jar).await?;

    let tree = tokio::task::spawn_blocking(discover_media_tree)
        .await
        .map_err(|error| {
            tracing::error!(error = ?error, "failed to join media tree task");
            ApiError::Internal
        })?;

    match tree {
        Ok(tree) => Ok(ok(tree)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(ok(None)),
        Err(error) => {
            tracing::error!(error = ?error, "failed to read media directory tree");
            Err(ApiError::Internal)
        }
    }
}

fn discover_media_tree() -> std::io::Result<Option<MediaDirectoryNodeResponse>> {
    build_media_tree(Path::new(MEDIA_TREE_ROOT_PATH))
}

fn build_media_tree(root: &Path) -> std::io::Result<Option<MediaDirectoryNodeResponse>> {
    let metadata = match fs::metadata(root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };

    if !metadata.is_dir() {
        return Ok(None);
    }

    Ok(Some(build_directory_node(root, true)?))
}

fn build_directory_node(
    directory: &Path,
    is_root: bool,
) -> std::io::Result<MediaDirectoryNodeResponse> {
    let mut children = Vec::new();

    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }

        children.push(build_directory_node(&entry.path(), false)?);
    }

    children.sort_by(|left, right| {
        left.name
            .to_ascii_lowercase()
            .cmp(&right.name.to_ascii_lowercase())
            .then_with(|| left.path.cmp(&right.path))
    });

    Ok(MediaDirectoryNodeResponse {
        name: if is_root {
            MEDIA_TREE_ROOT_NAME.to_string()
        } else {
            directory_name(directory)
        },
        path: normalize_directory_path(directory),
        children,
    })
}

fn directory_name(directory: &Path) -> String {
    directory
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| normalize_directory_path(directory))
}

fn normalize_directory_path(directory: &Path) -> String {
    directory.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::{build_media_tree, MediaDirectoryNodeResponse};
    use std::{
        env, fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    fn unique_temp_path(kind: &str) -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before unix epoch")
            .as_nanos();

        env::temp_dir().join(format!(
            "mova-server-{kind}-{}-{timestamp}",
            std::process::id()
        ))
    }

    fn create_dir(path: &Path) {
        fs::create_dir_all(path).expect("failed to create test directory");
    }

    #[test]
    fn build_media_tree_returns_none_for_missing_root() {
        let root = unique_temp_path("missing-tree");

        let tree = build_media_tree(&root).expect("tree lookup should not fail");

        assert_eq!(tree, None);
    }

    #[test]
    fn build_media_tree_recurses_subdirectories_and_ignores_files() {
        let root = unique_temp_path("media-tree");
        let movies = root.join("Movies");
        let alien = movies.join("Alien");
        let series = root.join("Series");
        let severance = series.join("Severance");
        let season_one = severance.join("Season 1");
        let loose_file = root.join("README.txt");

        let result = (|| {
            create_dir(&alien);
            create_dir(&season_one);
            fs::write(&loose_file, b"not a directory").expect("failed to write loose file");

            build_media_tree(&root).expect("failed to build tree")
        })();

        let _ = fs::remove_dir_all(&root);

        assert_eq!(
            result,
            Some(MediaDirectoryNodeResponse {
                name: "media".to_string(),
                path: root.to_string_lossy().replace('\\', "/"),
                children: vec![
                    MediaDirectoryNodeResponse {
                        name: "Movies".to_string(),
                        path: movies.to_string_lossy().replace('\\', "/"),
                        children: vec![MediaDirectoryNodeResponse {
                            name: "Alien".to_string(),
                            path: alien.to_string_lossy().replace('\\', "/"),
                            children: vec![],
                        }],
                    },
                    MediaDirectoryNodeResponse {
                        name: "Series".to_string(),
                        path: series.to_string_lossy().replace('\\', "/"),
                        children: vec![MediaDirectoryNodeResponse {
                            name: "Severance".to_string(),
                            path: severance.to_string_lossy().replace('\\', "/"),
                            children: vec![MediaDirectoryNodeResponse {
                                name: "Season 1".to_string(),
                                path: season_one.to_string_lossy().replace('\\', "/"),
                                children: vec![],
                            }],
                        }],
                    },
                ],
            })
        );
    }
}
