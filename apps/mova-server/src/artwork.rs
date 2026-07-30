use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::io::AsyncReadExt;

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
    let canonical_path = canonicalize_artwork_path(Path::new(artwork_path)).await?;
    let allowed_roots =
        canonicalize_allowed_roots([library_root, library_artwork_cache_root]).await;

    if !allowed_roots
        .iter()
        .any(|root| canonical_path.starts_with(root))
    {
        return Err(LocalArtworkError::Untrusted);
    }

    let file = fs::File::open(&canonical_path)
        .await
        .map_err(map_artwork_io_error)?;
    let metadata = file.metadata().await.map_err(map_artwork_io_error)?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_LOCAL_ARTWORK_BYTES {
        return Err(LocalArtworkError::Untrusted);
    }

    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_LOCAL_ARTWORK_BYTES + 1)
        .read_to_end(&mut bytes)
        .await
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

async fn canonicalize_artwork_path(path: &Path) -> Result<PathBuf, LocalArtworkError> {
    fs::canonicalize(path).await.map_err(map_artwork_io_error)
}

async fn canonicalize_allowed_roots<'a>(roots: impl IntoIterator<Item = &'a Path>) -> Vec<PathBuf> {
    let mut canonical_roots = Vec::new();
    for root in roots {
        if let Ok(root) = fs::canonicalize(root).await {
            canonical_roots.push(root);
        }
    }
    canonical_roots
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

    #[test]
    fn test_roots_are_absolute() {
        assert!(Path::new(&test_root()).is_absolute());
    }
}
