use anyhow::{bail, Context, Result};
use std::{
    io::ErrorKind,
    path::{Path, PathBuf},
};

const LIBRARY_CACHE_ROOT: &str = "libraries";

pub fn library_cache_dir(cache_dir: &Path, library_id: i64) -> PathBuf {
    cache_dir
        .join(LIBRARY_CACHE_ROOT)
        .join(library_id.to_string())
}

pub fn library_artwork_cache_dir(cache_dir: &Path, library_id: i64) -> PathBuf {
    library_cache_dir(cache_dir, library_id).join("artwork")
}

pub fn library_subtitle_cache_path(
    cache_dir: &Path,
    library_id: i64,
    subtitle_file_id: i64,
) -> PathBuf {
    library_cache_dir(cache_dir, library_id)
        .join("subtitles")
        .join(format!("subtitle-{subtitle_file_id}.vtt"))
}

pub fn library_audio_track_cache_dir(cache_dir: &Path, library_id: i64) -> PathBuf {
    library_cache_dir(cache_dir, library_id).join("audio-tracks")
}

pub async fn remove_library_cache(cache_dir: &Path, library_id: i64) -> Result<()> {
    if library_id <= 0 {
        bail!("library cache cleanup requires a positive library id");
    }

    let library_dir = library_cache_dir(cache_dir, library_id);
    match tokio::fs::remove_dir_all(&library_dir).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| {
            format!(
                "failed to remove library cache directory {}",
                library_dir.display()
            )
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        library_artwork_cache_dir, library_audio_track_cache_dir, library_cache_dir,
        library_subtitle_cache_path, remove_library_cache,
    };
    use std::path::Path;
    use uuid::Uuid;

    #[test]
    fn cache_paths_are_scoped_to_one_library() {
        let root = Path::new("/var/cache/mova");

        assert_eq!(
            library_cache_dir(root, 17),
            Path::new("/var/cache/mova/libraries/17")
        );
        assert_eq!(
            library_artwork_cache_dir(root, 17),
            Path::new("/var/cache/mova/libraries/17/artwork")
        );
        assert_eq!(
            library_subtitle_cache_path(root, 17, 29),
            Path::new("/var/cache/mova/libraries/17/subtitles/subtitle-29.vtt")
        );
        assert_eq!(
            library_audio_track_cache_dir(root, 17),
            Path::new("/var/cache/mova/libraries/17/audio-tracks")
        );
    }

    #[tokio::test]
    async fn cleanup_removes_only_the_requested_library_namespace() {
        let root = std::env::temp_dir().join(format!("mova-cache-test-{}", Uuid::new_v4()));
        let first = library_artwork_cache_dir(&root, 1)
            .join("poster")
            .join("one.jpg");
        let second = library_artwork_cache_dir(&root, 2)
            .join("poster")
            .join("two.jpg");
        tokio::fs::create_dir_all(first.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::create_dir_all(second.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(&first, b"one").await.unwrap();
        tokio::fs::write(&second, b"two").await.unwrap();

        remove_library_cache(&root, 1).await.unwrap();

        assert!(tokio::fs::metadata(library_cache_dir(&root, 1))
            .await
            .is_err());
        assert!(tokio::fs::metadata(&second).await.is_ok());

        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn cleanup_is_idempotent() {
        let root = std::env::temp_dir().join(format!("mova-cache-test-{}", Uuid::new_v4()));

        remove_library_cache(&root, 9).await.unwrap();
        remove_library_cache(&root, 9).await.unwrap();
    }
}
