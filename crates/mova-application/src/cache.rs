use anyhow::{bail, Context, Result};
use std::{
    collections::HashMap,
    io::ErrorKind,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock, Weak},
};
use tokio::{
    fs,
    io::AsyncWriteExt,
    sync::{Mutex as AsyncMutex, OwnedMutexGuard},
};
use uuid::Uuid;

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

/// Serializes materialization of one cache key inside the current process.
///
/// Atomic renames keep readers from observing partial files. The per-key lock
/// additionally avoids doing the same expensive conversion or download more
/// than once when concurrent requests miss the same cache entry.
pub async fn lock_cache_path(path: &Path) -> OwnedMutexGuard<()> {
    static LOCKS: OnceLock<Mutex<HashMap<PathBuf, Weak<AsyncMutex<()>>>>> = OnceLock::new();

    let lock = {
        let mut locks = LOCKS
            .get_or_init(|| Mutex::new(HashMap::new()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        locks.retain(|_, lock| lock.strong_count() > 0);

        match locks.get(path).and_then(Weak::upgrade) {
            Some(lock) => lock,
            None => {
                let lock = Arc::new(AsyncMutex::new(()));
                locks.insert(path.to_path_buf(), Arc::downgrade(&lock));
                lock
            }
        }
    };

    lock.lock_owned().await
}

/// Returns a unique temporary file in the final file's directory while
/// retaining the final extension so tools such as FFmpeg can infer the format.
pub fn cache_temp_path(final_path: &Path) -> PathBuf {
    let stem = final_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("cache");
    let extension = final_path
        .extension()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty());
    let temporary_name = match extension {
        Some(extension) => format!(".{stem}.{}.tmp.{extension}", Uuid::new_v4()),
        None => format!(".{stem}.{}.tmp", Uuid::new_v4()),
    };

    final_path.with_file_name(temporary_name)
}

/// Owns one unpublished cache file and removes it if the generating future is
/// cancelled, times out, or returns early before the atomic rename.
#[derive(Debug)]
pub struct CacheTempFileGuard {
    path: PathBuf,
    armed: bool,
}

impl CacheTempFileGuard {
    pub fn new(final_path: &Path) -> Self {
        Self {
            path: cache_temp_path(final_path),
            armed: true,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for CacheTempFileGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

pub async fn is_nonempty_cache_file(path: &Path) -> bool {
    fs::metadata(path)
        .await
        .map(|metadata| metadata.is_file() && metadata.len() > 0)
        .unwrap_or(false)
}

/// Writes a complete cache payload beside the destination and publishes it
/// with one same-filesystem rename. Failed writes never leave a final file.
pub async fn write_cache_file_atomically(final_path: &Path, payload: &[u8]) -> Result<()> {
    if payload.is_empty() {
        bail!("cache payload cannot be empty");
    }

    let parent = final_path
        .parent()
        .context("cache file path must have a parent directory")?;
    fs::create_dir_all(parent).await.with_context(|| {
        format!(
            "failed to create cache directory before writing {}",
            final_path.display()
        )
    })?;

    let mut temporary_file = CacheTempFileGuard::new(final_path);
    let temporary_path = temporary_file.path().to_path_buf();
    let result = async {
        let mut file = fs::File::create(&temporary_path).await.with_context(|| {
            format!(
                "failed to create temporary cache file {}",
                temporary_path.display()
            )
        })?;
        file.write_all(payload).await.with_context(|| {
            format!(
                "failed to write temporary cache file {}",
                temporary_path.display()
            )
        })?;
        file.flush().await.with_context(|| {
            format!(
                "failed to flush temporary cache file {}",
                temporary_path.display()
            )
        })?;
        file.sync_all().await.with_context(|| {
            format!(
                "failed to sync temporary cache file {}",
                temporary_path.display()
            )
        })?;
        drop(file);

        commit_cache_file(&temporary_path, final_path).await
    }
    .await;

    if result.is_ok() {
        temporary_file.disarm();
    }

    result
}

/// Publishes a fully generated temporary cache file. Both paths must be on the
/// same filesystem so `rename` is atomic for readers.
pub async fn commit_cache_file(temporary_path: &Path, final_path: &Path) -> Result<()> {
    if !is_nonempty_cache_file(temporary_path).await {
        bail!(
            "temporary cache file is missing or empty: {}",
            temporary_path.display()
        );
    }

    fs::rename(temporary_path, final_path)
        .await
        .with_context(|| {
            format!(
                "failed to publish cache file {} as {}",
                temporary_path.display(),
                final_path.display()
            )
        })
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
        cache_temp_path, library_artwork_cache_dir, library_audio_track_cache_dir,
        library_cache_dir, library_subtitle_cache_path, lock_cache_path, remove_library_cache,
        write_cache_file_atomically, CacheTempFileGuard,
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

    #[test]
    fn temporary_cache_path_stays_beside_destination_and_keeps_extension() {
        let final_path = Path::new("/var/cache/mova/poster.webp");
        let temporary_path = cache_temp_path(final_path);

        assert_eq!(temporary_path.parent(), final_path.parent());
        assert_eq!(
            temporary_path.extension().and_then(|value| value.to_str()),
            Some("webp")
        );
        assert_ne!(temporary_path, final_path);
    }

    #[tokio::test]
    async fn atomic_cache_write_publishes_only_the_complete_payload() {
        let root = std::env::temp_dir().join(format!("mova-cache-test-{}", Uuid::new_v4()));
        let final_path = root.join("artwork").join("poster.jpg");

        write_cache_file_atomically(&final_path, b"complete")
            .await
            .unwrap();

        assert_eq!(tokio::fs::read(&final_path).await.unwrap(), b"complete");
        let entries = std::fs::read_dir(final_path.parent().unwrap())
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(entries.len(), 1);
        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[test]
    fn temporary_cache_guard_removes_an_unpublished_file_on_drop() {
        let root = std::env::temp_dir().join(format!("mova-cache-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let final_path = root.join("poster.jpg");
        let temporary_file = CacheTempFileGuard::new(&final_path);
        std::fs::write(temporary_file.path(), b"partial").unwrap();
        let temporary_path = temporary_file.path().to_path_buf();

        drop(temporary_file);

        assert!(!temporary_path.exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn cache_key_lock_serializes_the_same_path() {
        let path = Path::new("/var/cache/mova/serialized.cache");
        let first = lock_cache_path(path).await;
        let waiting = tokio::spawn(async move {
            let _second = lock_cache_path(path).await;
        });

        tokio::task::yield_now().await;
        assert!(!waiting.is_finished());

        drop(first);
        waiting.await.unwrap();
    }
}
