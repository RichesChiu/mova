use mova_scan::{
    has_meaningful_file_title, infer_movie_container_identity, infer_series_container_identity,
    infer_series_sidecar_metadata_within_root, DiscoveredMediaFile,
};
use std::path::Path;

pub const LIBRARY_TYPE_MOVIE: &str = "movie";
pub const LIBRARY_TYPE_SERIES: &str = "series";

pub fn classify_media_type(file_path: &Path) -> &'static str {
    if mova_scan::is_likely_episode_path(file_path) {
        "episode"
    } else {
        "movie"
    }
}

pub fn metadata_lookup_type_for_media_type(media_type: &str) -> &'static str {
    if media_type.eq_ignore_ascii_case("episode") || media_type.eq_ignore_ascii_case("series") {
        LIBRARY_TYPE_SERIES
    } else {
        LIBRARY_TYPE_MOVIE
    }
}

pub(crate) fn apply_root_aware_media_identity(
    file: &mut DiscoveredMediaFile,
    root_path: &Path,
) -> Option<String> {
    if file.season_number.is_some() && file.episode_number.is_some() {
        let has_file_title = has_meaningful_file_title(&file.file_path);
        let container_identity = infer_series_container_identity(&file.file_path, root_path);
        let sidecar = infer_series_sidecar_metadata_within_root(&file.file_path, root_path);

        if let Some(title) = sidecar
            .as_ref()
            .and_then(|metadata| metadata.title.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            file.source_title = title.to_string();
            file.title = title.to_string();
        } else if !has_file_title {
            if let Some(identity) = container_identity.as_ref() {
                file.source_title = identity.title.clone();
                file.title = identity.display_title.clone();
            }
        }

        if let Some(year) = sidecar.as_ref().and_then(|metadata| metadata.year) {
            file.year = Some(year);
        } else if !has_file_title && file.year.is_none() {
            file.year = container_identity
                .as_ref()
                .and_then(|identity| identity.year);
        }

        return container_identity.and_then(|identity| identity.tmdb_id);
    }

    let container_identity = infer_movie_container_identity(&file.file_path, root_path);
    apply_movie_container_identity(file, container_identity.as_ref());
    container_identity.and_then(|identity| identity.tmdb_id)
}

pub(crate) fn apply_movie_container_identity_when_title_is_missing(
    file: &mut DiscoveredMediaFile,
    root_path: &Path,
) {
    if file.season_number.is_some()
        || file.episode_number.is_some()
        || file.metadata_provider_item_id.is_some()
    {
        return;
    }

    let identity = infer_movie_container_identity(&file.file_path, root_path);
    apply_movie_container_identity(file, identity.as_ref());
}

fn apply_movie_container_identity(
    file: &mut DiscoveredMediaFile,
    identity: Option<&mova_scan::MediaContainerIdentity>,
) {
    if has_meaningful_file_title(&file.file_path) {
        return;
    }
    let Some(identity) = identity else {
        return;
    };
    let parsed_source_title = file.source_title.clone();
    let has_sidecar_title = !file.title.trim().is_empty()
        && !file
            .title
            .trim()
            .eq_ignore_ascii_case(parsed_source_title.trim());

    file.source_title = identity.title.clone();
    if !has_sidecar_title {
        file.title = identity.display_title.clone();
    }
    if file.year.is_none() {
        file.year = identity.year;
    }
}

#[cfg(test)]
mod tests {
    use super::{
        classify_media_type, metadata_lookup_type_for_media_type, LIBRARY_TYPE_MOVIE,
        LIBRARY_TYPE_SERIES,
    };
    use std::path::Path;

    #[test]
    fn classify_media_type_uses_file_name_heuristics_for_all_libraries() {
        assert_eq!(
            classify_media_type(Path::new("Arcane.S01E01.mkv")),
            "episode"
        );
        assert_eq!(
            classify_media_type(Path::new("The.BeautyS01E01.2026.mkv")),
            "episode"
        );
        assert_eq!(
            classify_media_type(Path::new("Spirited.Away.2001.mkv")),
            "movie"
        );
    }

    #[test]
    fn metadata_lookup_type_maps_episode_like_media_to_series() {
        assert_eq!(
            metadata_lookup_type_for_media_type("episode"),
            LIBRARY_TYPE_SERIES
        );
        assert_eq!(
            metadata_lookup_type_for_media_type("series"),
            LIBRARY_TYPE_SERIES
        );
        assert_eq!(
            metadata_lookup_type_for_media_type("movie"),
            LIBRARY_TYPE_MOVIE
        );
    }
}
