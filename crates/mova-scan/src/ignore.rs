//! Entries that are never media: hidden files, NAS and OS system folders,
//! and the trailers and samples shipped next to a main video.
//!
//! The rules are name-based so discovery can skip an entry before it stats
//! or canonicalizes it, and the folder picker can leave it out of the tree.

use std::{
    ffi::OsStr,
    path::{Component, Path},
};

/// Directory names that hold recycle bins, thumbnails, snapshots or extras.
const IGNORED_DIRECTORY_NAMES: &[&str] = &[
    "#recycle",
    "#snapshot",
    "@eadir",
    "@recycle",
    "@recently-snapshot",
    "$recycle.bin",
    "system volume information",
    "lost+found",
    "extras",
    "trailers",
    "samples",
    "sample",
    "featurettes",
    "behind the scenes",
    "deleted scenes",
    "interviews",
];

const IGNORED_FILE_NAMES: &[&str] = &["thumbs.db", "desktop.ini"];

/// Video stems that mark a trailer or sample rather than the main feature.
const IGNORED_VIDEO_STEMS: &[&str] = &["sample", "trailer"];
const IGNORED_VIDEO_STEM_SUFFIXES: &[&str] = &["-sample", "-trailer", ".sample", ".trailer"];

/// Whether a directory entry is skipped before its type is known: hidden
/// (dot-prefixed, which covers macOS `._*` files), a system or extras
/// directory, or an OS junk file.
pub fn is_ignored_entry_name(name: &OsStr) -> bool {
    let Some(name) = name.to_str() else {
        return false;
    };
    if name.starts_with('.') {
        return true;
    }
    let lowered = name.to_ascii_lowercase();
    IGNORED_DIRECTORY_NAMES.contains(&lowered.as_str())
        || IGNORED_FILE_NAMES.contains(&lowered.as_str())
}

/// Whether a video file is a trailer or sample by its name.
pub fn is_ignored_video_file_name(name: &OsStr) -> bool {
    let Some(stem) = Path::new(name).file_stem().and_then(OsStr::to_str) else {
        return false;
    };
    let stem = stem.to_ascii_lowercase();
    IGNORED_VIDEO_STEMS.contains(&stem.as_str())
        || IGNORED_VIDEO_STEM_SUFFIXES
            .iter()
            .any(|suffix| stem.ends_with(suffix))
}

/// Whether a catalog path under `root` would be skipped by discovery today:
/// its own name is ignored, or any directory between the root and the file
/// is. Used to drop such paths from the catalog even while the file exists.
pub fn is_ignored_media_path(root: &Path, path: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return false;
    };
    let mut components = relative.components().peekable();
    while let Some(component) = components.next() {
        let Component::Normal(name) = component else {
            continue;
        };
        if is_ignored_entry_name(name) {
            return true;
        }
        if components.peek().is_none() && is_ignored_video_file_name(name) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::{is_ignored_entry_name, is_ignored_media_path, is_ignored_video_file_name};
    use std::{ffi::OsStr, path::Path};

    #[test]
    fn hidden_system_and_junk_names_are_ignored() {
        for name in [
            "._Movie.mkv",
            ".DS_Store",
            ".Trash-1000",
            ".@__thumb",
            "#recycle",
            "#Recycle",
            "@eaDir",
            "@Recycle",
            "@Recently-Snapshot",
            "$RECYCLE.BIN",
            "System Volume Information",
            "lost+found",
            "Extras",
            "Trailers",
            "Samples",
            "Thumbs.db",
            "desktop.ini",
        ] {
            assert!(is_ignored_entry_name(OsStr::new(name)), "{name}");
        }
        for name in [
            "Movies",
            "山海情 (2021)",
            "Dune (2021).mkv",
            "Sample Show (2020)",
        ] {
            assert!(!is_ignored_entry_name(OsStr::new(name)), "{name}");
        }
    }

    #[test]
    fn trailers_and_samples_are_ignored_by_stem() {
        for name in [
            "sample.mkv",
            "Trailer.mp4",
            "Dune (2021)-trailer.mkv",
            "Dune.2021.1080p-sample.mkv",
            "Dune.2021.sample.mkv",
            "Dune (2021).trailer.mp4",
        ] {
            assert!(is_ignored_video_file_name(OsStr::new(name)), "{name}");
        }
        for name in [
            "Dune (2021).mkv",
            "The Sample (2019).mkv",
            "Trailer Park Boys S01E01.mkv",
            "Free Samples (2012).mkv",
        ] {
            assert!(!is_ignored_video_file_name(OsStr::new(name)), "{name}");
        }
    }

    #[test]
    fn catalog_paths_are_judged_by_every_component_below_the_root() {
        let root = Path::new("/media/movies");
        for path in [
            "/media/movies/#recycle/Dune (2021)/Dune (2021).mkv",
            "/media/movies/Dune (2021)/@eaDir/Dune (2021).mkv",
            "/media/movies/Dune (2021)/._Dune (2021).mkv",
            "/media/movies/Dune (2021)/Dune (2021)-trailer.mkv",
            "/media/movies/Dune (2021)/Extras/Making of.mkv",
        ] {
            assert!(is_ignored_media_path(root, Path::new(path)), "{path}");
        }
        assert!(!is_ignored_media_path(
            root,
            Path::new("/media/movies/Dune (2021)/Dune (2021).mkv")
        ));
        // The root itself may carry any name.
        assert!(!is_ignored_media_path(
            Path::new("/media/.hidden-library"),
            Path::new("/media/.hidden-library/Dune (2021)/Dune (2021).mkv")
        ));
        // Paths outside the root are not judged here.
        assert!(!is_ignored_media_path(
            root,
            Path::new("/other/#recycle/Dune (2021).mkv")
        ));
    }
}
