use std::path::{Path, PathBuf};
use tokio::fs;

#[derive(Debug)]
pub(crate) enum LibraryMediaPathError {
    LibraryRoot(std::io::Error),
    MediaSource(std::io::Error),
    OutsideLibraryRoot,
    NotRegularFile,
}

/// Resolve an existing regular file while enforcing its owning library boundary.
///
/// The returned path is canonical so callers do not subsequently reopen the
/// untrusted database path through a symbolic link.
pub(crate) async fn resolve_regular_file_within_library(
    file_path: &Path,
    library_root: &Path,
) -> Result<PathBuf, LibraryMediaPathError> {
    let canonical_root = fs::canonicalize(library_root)
        .await
        .map_err(LibraryMediaPathError::LibraryRoot)?;
    let root_metadata = fs::metadata(&canonical_root)
        .await
        .map_err(LibraryMediaPathError::LibraryRoot)?;
    if !root_metadata.is_dir() {
        return Err(LibraryMediaPathError::LibraryRoot(std::io::Error::new(
            std::io::ErrorKind::NotADirectory,
            format!(
                "media library root is not a directory: {}",
                library_root.display()
            ),
        )));
    }

    let canonical_source = fs::canonicalize(file_path)
        .await
        .map_err(LibraryMediaPathError::MediaSource)?;
    if !canonical_source.starts_with(&canonical_root) {
        return Err(LibraryMediaPathError::OutsideLibraryRoot);
    }

    let source_metadata = fs::metadata(&canonical_source)
        .await
        .map_err(LibraryMediaPathError::MediaSource)?;
    if !source_metadata.is_file() {
        return Err(LibraryMediaPathError::NotRegularFile);
    }

    Ok(canonical_source)
}

#[cfg(test)]
mod tests {
    use super::{resolve_regular_file_within_library, LibraryMediaPathError};
    use std::path::PathBuf;
    use uuid::Uuid;

    fn test_root(kind: &str) -> PathBuf {
        std::env::temp_dir().join(format!("mova-media-path-{kind}-{}", Uuid::new_v4()))
    }

    #[tokio::test]
    async fn resolves_regular_files_inside_the_library() {
        let root = test_root("inside");
        let media_file = root.join("nested").join("movie.mkv");
        tokio::fs::create_dir_all(media_file.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(&media_file, b"video").await.unwrap();

        let resolved = resolve_regular_file_within_library(&media_file, &root)
            .await
            .unwrap();

        assert_eq!(
            resolved,
            tokio::fs::canonicalize(&media_file).await.unwrap()
        );
        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn rejects_direct_paths_outside_the_library() {
        let root = test_root("outside-root");
        let outside = test_root("outside-file");
        tokio::fs::create_dir_all(&root).await.unwrap();
        tokio::fs::write(&outside, b"secret").await.unwrap();

        let result = resolve_regular_file_within_library(&outside, &root).await;

        assert!(matches!(
            result,
            Err(LibraryMediaPathError::OutsideLibraryRoot)
        ));
        let _ = tokio::fs::remove_dir_all(root).await;
        let _ = tokio::fs::remove_file(outside).await;
    }

    #[tokio::test]
    async fn rejects_directories_inside_the_library() {
        let root = test_root("directory");
        let directory = root.join("not-a-file");
        tokio::fs::create_dir_all(&directory).await.unwrap();

        let result = resolve_regular_file_within_library(&directory, &root).await;

        assert!(matches!(result, Err(LibraryMediaPathError::NotRegularFile)));
        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn accepts_internal_symlinks_and_rejects_external_symlinks() {
        use std::os::unix::fs::symlink;

        let root = test_root("symlinks");
        let outside = test_root("outside-symlink-target");
        let target = root.join("target.mkv");
        let internal_link = root.join("internal.mkv");
        let external_link = root.join("external.mkv");
        tokio::fs::create_dir_all(&root).await.unwrap();
        tokio::fs::write(&target, b"video").await.unwrap();
        tokio::fs::write(&outside, b"secret").await.unwrap();
        symlink(&target, &internal_link).unwrap();
        symlink(&outside, &external_link).unwrap();

        let internal = resolve_regular_file_within_library(&internal_link, &root)
            .await
            .unwrap();
        let external = resolve_regular_file_within_library(&external_link, &root).await;

        assert_eq!(internal, tokio::fs::canonicalize(&target).await.unwrap());
        assert!(matches!(
            external,
            Err(LibraryMediaPathError::OutsideLibraryRoot)
        ));
        let _ = tokio::fs::remove_dir_all(root).await;
        let _ = tokio::fs::remove_file(outside).await;
    }
}
