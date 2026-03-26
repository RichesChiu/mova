use crate::error::{ApplicationError, ApplicationResult};
use std::path::Path;

pub const LIBRARY_TYPE_MIXED: &str = "mixed";
pub const LIBRARY_TYPE_MOVIE: &str = "movie";
pub const LIBRARY_TYPE_SERIES: &str = "series";

pub fn normalize_library_type(value: String) -> ApplicationResult<String> {
    let normalized = value.trim().to_ascii_lowercase();

    match normalized.as_str() {
        LIBRARY_TYPE_MIXED | LIBRARY_TYPE_MOVIE | LIBRARY_TYPE_SERIES => Ok(normalized),
        _ => Err(ApplicationError::Validation(format!(
            "library type must be one of: {}, {}, {}",
            LIBRARY_TYPE_MIXED, LIBRARY_TYPE_MOVIE, LIBRARY_TYPE_SERIES
        ))),
    }
}

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
        classify_media_type, metadata_lookup_type_for_media_type, normalize_library_type,
        LIBRARY_TYPE_MIXED, LIBRARY_TYPE_MOVIE, LIBRARY_TYPE_SERIES,
    };
    use crate::ApplicationError;
    use std::path::Path;

    #[test]
    fn normalize_library_type_accepts_mixed_movie_and_series() {
        assert_eq!(
            normalize_library_type("mixed".to_string()).unwrap(),
            LIBRARY_TYPE_MIXED
        );
        assert_eq!(
            normalize_library_type("movie".to_string()).unwrap(),
            LIBRARY_TYPE_MOVIE
        );
        assert_eq!(
            normalize_library_type("series".to_string()).unwrap(),
            LIBRARY_TYPE_SERIES
        );
    }

    #[test]
    fn normalize_library_type_rejects_unknown_value() {
        assert!(matches!(
            normalize_library_type("anime".to_string()),
            Err(ApplicationError::Validation(message))
                if message.contains("library type must be one of")
        ));
    }

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
