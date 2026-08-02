use std::fs::File;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

const MAX_LOCAL_ARTWORK_BYTES: u64 = 20 * 1024 * 1024;

#[derive(Debug)]
pub struct LocalArtwork {
    pub bytes: Vec<u8>,
    pub content_type: &'static str,
}

#[derive(Debug)]
pub enum LocalArtworkError {
    NotFound,
    Untrusted,
    Io(std::io::Error),
}

pub async fn read_trusted_local_artwork(
    artwork_path: &str,
    library_root: &Path,
    library_artwork_cache_root: &Path,
) -> Result<LocalArtwork, LocalArtworkError> {
    let artwork_path = PathBuf::from(artwork_path);
    let allowed_roots = [
        library_root.to_path_buf(),
        library_artwork_cache_root.to_path_buf(),
    ];

    tokio::task::spawn_blocking(move || {
        read_trusted_local_artwork_blocking(&artwork_path, &allowed_roots)
    })
    .await
    .map_err(|error| LocalArtworkError::Io(std::io::Error::other(error)))?
}

fn read_trusted_local_artwork_blocking(
    artwork_path: &Path,
    allowed_roots: &[PathBuf],
) -> Result<LocalArtwork, LocalArtworkError> {
    let mut matched_allowed_root = false;
    let mut last_error = None;

    for root in allowed_roots {
        let Some(relative_path) = trusted_relative_path(artwork_path, root) else {
            continue;
        };
        matched_allowed_root = true;

        match open_artwork_beneath_root(root, &relative_path).and_then(read_artwork_from_file) {
            Ok(artwork) => return Ok(artwork),
            Err(error) => last_error = Some(error),
        }
    }

    if !matched_allowed_root {
        return Err(LocalArtworkError::Untrusted);
    }

    Err(last_error.unwrap_or(LocalArtworkError::Untrusted))
}

fn trusted_relative_path(path: &Path, root: &Path) -> Option<PathBuf> {
    if !path.is_absolute() || !root.is_absolute() {
        return None;
    }

    let relative_path = path.strip_prefix(root).ok()?;
    if relative_path.as_os_str().is_empty()
        || relative_path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return None;
    }

    Some(relative_path.to_path_buf())
}

#[cfg(unix)]
fn open_artwork_beneath_root(root: &Path, relative_path: &Path) -> Result<File, LocalArtworkError> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::OpenOptionsExt;
    use std::os::unix::io::{AsRawFd, FromRawFd};

    let mut directory = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_CLOEXEC | libc::O_DIRECTORY)
        .open(root)
        .map_err(map_artwork_io_error)?;
    let components = relative_path.components().collect::<Vec<_>>();

    for (index, component) in components.iter().enumerate() {
        let Component::Normal(name) = component else {
            return Err(LocalArtworkError::Untrusted);
        };
        let name = CString::new(name.as_bytes()).map_err(|_| LocalArtworkError::Untrusted)?;
        let is_final_component = index + 1 == components.len();
        let flags = if is_final_component {
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK
        } else {
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_DIRECTORY
        };

        // SAFETY: `directory` owns a live descriptor, `name` is NUL-terminated, and no
        // creation flag is used, so `openat` does not require a mode argument.
        let descriptor = unsafe { libc::openat(directory.as_raw_fd(), name.as_ptr(), flags) };
        if descriptor < 0 {
            return Err(map_openat_error(std::io::Error::last_os_error()));
        }

        // SAFETY: a successful `openat` call returns a new owned descriptor. Wrapping it
        // immediately in `File` ensures it is closed on every subsequent return path.
        let opened = unsafe { File::from_raw_fd(descriptor) };
        if is_final_component {
            return Ok(opened);
        }
        directory = opened;
    }

    Err(LocalArtworkError::Untrusted)
}

#[cfg(unix)]
fn map_openat_error(error: std::io::Error) -> LocalArtworkError {
    match error.raw_os_error() {
        Some(libc::ELOOP) | Some(libc::ENOTDIR) => LocalArtworkError::Untrusted,
        _ => map_artwork_io_error(error),
    }
}

#[cfg(not(unix))]
fn open_artwork_beneath_root(root: &Path, relative_path: &Path) -> Result<File, LocalArtworkError> {
    use std::sync::Once;

    static WARNED_ABOUT_PATH_FALLBACK: Once = Once::new();
    WARNED_ABOUT_PATH_FALLBACK.call_once(|| {
        tracing::warn!(
            "local artwork path validation uses canonical paths on this platform; handle-relative no-follow traversal is unavailable"
        );
    });

    // Non-Unix targets do not expose the openat/O_NOFOLLOW primitives used above. Keep
    // the fallback conservative by resolving both paths immediately before opening and
    // rejecting anything that no longer resolves beneath the configured root. There is
    // still a small platform-level race between canonicalization and opening, which is
    // why the limitation is logged once at runtime.
    let canonical_root = std::fs::canonicalize(root).map_err(map_artwork_io_error)?;
    let canonical_path =
        std::fs::canonicalize(root.join(relative_path)).map_err(map_artwork_io_error)?;
    if !canonical_path.starts_with(&canonical_root) {
        return Err(LocalArtworkError::Untrusted);
    }

    File::open(canonical_path).map_err(map_artwork_io_error)
}

fn read_artwork_from_file(mut file: File) -> Result<LocalArtwork, LocalArtworkError> {
    let metadata = file.metadata().map_err(map_artwork_io_error)?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_LOCAL_ARTWORK_BYTES {
        return Err(LocalArtworkError::Untrusted);
    }

    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.by_ref()
        .take(MAX_LOCAL_ARTWORK_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(map_artwork_io_error)?;
    if bytes.is_empty() || bytes.len() as u64 > MAX_LOCAL_ARTWORK_BYTES {
        return Err(LocalArtworkError::Untrusted);
    }
    let content_type = artwork_content_type(&bytes).ok_or(LocalArtworkError::Untrusted)?;

    Ok(LocalArtwork {
        bytes,
        content_type,
    })
}

fn map_artwork_io_error(error: std::io::Error) -> LocalArtworkError {
    if error.kind() == std::io::ErrorKind::NotFound {
        LocalArtworkError::NotFound
    } else {
        LocalArtworkError::Io(error)
    }
}

fn artwork_content_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.len() >= 12
        && &bytes[4..8] == b"ftyp"
        && matches!(&bytes[8..12], b"avif" | b"avis")
    {
        Some("image/avif")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{read_trusted_local_artwork, LocalArtworkError};
    use std::path::Path;
    use uuid::Uuid;

    fn test_root() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("mova-artwork-test-{}", Uuid::new_v4()))
    }

    #[tokio::test]
    async fn serves_valid_images_from_the_library_or_its_cache() {
        let root = test_root();
        let library_root = root.join("library");
        let cache_root = root.join("cache");
        tokio::fs::create_dir_all(&library_root).await.unwrap();
        tokio::fs::create_dir_all(&cache_root).await.unwrap();
        let sidecar = library_root.join("poster.jpg");
        let cached = cache_root.join("poster.png");
        tokio::fs::write(&sidecar, b"\xff\xd8\xffposter")
            .await
            .unwrap();
        tokio::fs::write(&cached, b"\x89PNG\r\n\x1a\nposter")
            .await
            .unwrap();

        let sidecar_result =
            read_trusted_local_artwork(sidecar.to_str().unwrap(), &library_root, &cache_root)
                .await
                .unwrap();
        let cached_result =
            read_trusted_local_artwork(cached.to_str().unwrap(), &library_root, &cache_root)
                .await
                .unwrap();

        assert_eq!(sidecar_result.content_type, "image/jpeg");
        assert_eq!(cached_result.content_type, "image/png");
        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn rejects_preexisting_paths_outside_the_library_and_cache() {
        let root = test_root();
        let library_root = root.join("library");
        let cache_root = root.join("cache");
        let outside = root.join("outside.jpg");
        tokio::fs::create_dir_all(&library_root).await.unwrap();
        tokio::fs::create_dir_all(&cache_root).await.unwrap();
        tokio::fs::write(&outside, b"\xff\xd8\xffoutside")
            .await
            .unwrap();

        let result =
            read_trusted_local_artwork(outside.to_str().unwrap(), &library_root, &cache_root).await;

        assert!(matches!(result, Err(LocalArtworkError::Untrusted)));
        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn rejects_non_image_payloads_inside_an_allowed_root() {
        let root = test_root();
        let library_root = root.join("library");
        let cache_root = root.join("cache");
        tokio::fs::create_dir_all(&library_root).await.unwrap();
        tokio::fs::create_dir_all(&cache_root).await.unwrap();
        let fake_image = library_root.join("poster.jpg");
        tokio::fs::write(&fake_image, b"not an image")
            .await
            .unwrap();

        let result =
            read_trusted_local_artwork(fake_image.to_str().unwrap(), &library_root, &cache_root)
                .await;

        assert!(matches!(result, Err(LocalArtworkError::Untrusted)));
        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn rejects_parent_directory_components_even_when_they_resolve_inside_the_root() {
        let root = test_root();
        let library_root = root.join("library");
        let cache_root = root.join("cache");
        tokio::fs::create_dir_all(library_root.join("nested"))
            .await
            .unwrap();
        tokio::fs::create_dir_all(&cache_root).await.unwrap();
        tokio::fs::write(library_root.join("poster.jpg"), b"\xff\xd8\xffposter")
            .await
            .unwrap();
        let non_normalized_path = library_root.join("nested").join("..").join("poster.jpg");

        let result = read_trusted_local_artwork(
            non_normalized_path.to_str().unwrap(),
            &library_root,
            &cache_root,
        )
        .await;

        assert!(matches!(result, Err(LocalArtworkError::Untrusted)));
        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn rejects_symlinks_that_escape_an_allowed_root() {
        let root = test_root();
        let library_root = root.join("library");
        let cache_root = root.join("cache");
        let outside = root.join("outside.jpg");
        tokio::fs::create_dir_all(&library_root).await.unwrap();
        tokio::fs::create_dir_all(&cache_root).await.unwrap();
        tokio::fs::write(&outside, b"\xff\xd8\xffoutside")
            .await
            .unwrap();
        std::os::unix::fs::symlink(&outside, library_root.join("poster.jpg")).unwrap();

        let result = read_trusted_local_artwork(
            library_root.join("poster.jpg").to_str().unwrap(),
            &library_root,
            &cache_root,
        )
        .await;

        assert!(matches!(result, Err(LocalArtworkError::Untrusted)));
        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn rejects_symlinked_directories_even_when_the_target_stays_inside_the_root() {
        let root = test_root();
        let library_root = root.join("library");
        let cache_root = root.join("cache");
        let real_directory = library_root.join("real");
        tokio::fs::create_dir_all(&real_directory).await.unwrap();
        tokio::fs::create_dir_all(&cache_root).await.unwrap();
        tokio::fs::write(real_directory.join("poster.jpg"), b"\xff\xd8\xffposter")
            .await
            .unwrap();
        std::os::unix::fs::symlink(&real_directory, library_root.join("alias")).unwrap();

        let result = read_trusted_local_artwork(
            library_root.join("alias/poster.jpg").to_str().unwrap(),
            &library_root,
            &cache_root,
        )
        .await;

        assert!(matches!(result, Err(LocalArtworkError::Untrusted)));
        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn rejects_symlinked_files_even_when_the_target_stays_inside_the_root() {
        let root = test_root();
        let library_root = root.join("library");
        let cache_root = root.join("cache");
        tokio::fs::create_dir_all(&library_root).await.unwrap();
        tokio::fs::create_dir_all(&cache_root).await.unwrap();
        let real_artwork = library_root.join("real.jpg");
        tokio::fs::write(&real_artwork, b"\xff\xd8\xffposter")
            .await
            .unwrap();
        std::os::unix::fs::symlink(&real_artwork, library_root.join("poster.jpg")).unwrap();

        let result = read_trusted_local_artwork(
            library_root.join("poster.jpg").to_str().unwrap(),
            &library_root,
            &cache_root,
        )
        .await;

        assert!(matches!(result, Err(LocalArtworkError::Untrusted)));
        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[test]
    fn test_roots_are_absolute() {
        assert!(Path::new(&test_root()).is_absolute());
    }
}
