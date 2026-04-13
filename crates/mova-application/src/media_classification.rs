use std::path::Path;

pub const LIBRARY_TYPE_MIXED: &str = "mixed";
pub const LIBRARY_TYPE_MOVIE: &str = "movie";
pub const LIBRARY_TYPE_SERIES: &str = "series";

pub fn classify_media_type(library_type: &str, file_path: &Path) -> &'static str {
    if library_type.eq_ignore_ascii_case(LIBRARY_TYPE_SERIES) {
        "episode"
    } else if library_type.eq_ignore_ascii_case(LIBRARY_TYPE_MIXED)
        && mova_scan::is_likely_episode_path(file_path)
    {
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

#[cfg(test)]
mod tests {
    use super::{
        classify_media_type, metadata_lookup_type_for_media_type, LIBRARY_TYPE_MIXED,
        LIBRARY_TYPE_MOVIE, LIBRARY_TYPE_SERIES,
    };
    use std::path::Path;

    #[test]
    fn classify_media_type_uses_file_heuristics_for_mixed_libraries() {
        assert_eq!(
            classify_media_type(LIBRARY_TYPE_MIXED, Path::new("Arcane.S01E01.mkv")),
            "episode"
        );
        assert_eq!(
            classify_media_type(LIBRARY_TYPE_MIXED, Path::new("Spirited.Away.2001.mkv")),
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
