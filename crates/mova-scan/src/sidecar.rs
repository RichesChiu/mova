use super::parse::parse_year_token;
use std::{
    fs,
    io::Read,
    ops::Range,
    path::{Component, Path, PathBuf},
};

const MAX_MEDIA_NFO_BYTES: usize = 2 * 1024 * 1024;
const MAX_TVSHOW_NFO_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct ParsedSidecarMetadata {
    pub title: Option<String>,
    pub original_title: Option<String>,
    pub sort_title: Option<String>,
    pub year: Option<i32>,
    pub overview: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum ArtworkKind {
    Poster,
    Backdrop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArtworkScope {
    FileSpecific,
    Generic,
}

pub(crate) fn read_sidecar_metadata(path: &Path) -> ParsedSidecarMetadata {
    let Some(nfo_path) = find_sidecar_nfo(path) else {
        return ParsedSidecarMetadata::default();
    };

    let Some(contents) = read_nfo_file(&nfo_path, MAX_MEDIA_NFO_BYTES) else {
        return ParsedSidecarMetadata::default();
    };

    parse_nfo_metadata(
        &contents,
        nfo_path.parent().unwrap_or_else(|| Path::new("/")),
    )
}

pub(crate) fn read_series_sidecar_metadata(path: &Path) -> ParsedSidecarMetadata {
    let nfo_path = path
        .parent()
        .into_iter()
        .flat_map(|parent| parent.ancestors().take(5))
        .map(|directory| directory.join("tvshow.nfo"))
        .find(|candidate| candidate.is_file());

    read_series_sidecar_metadata_from_path(nfo_path)
}

pub(crate) fn read_series_sidecar_metadata_within_root(
    path: &Path,
    root_path: &Path,
) -> ParsedSidecarMetadata {
    let Some(relative_path) = path.strip_prefix(root_path).ok().filter(|relative_path| {
        !relative_path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    }) else {
        return ParsedSidecarMetadata::default();
    };
    let Some(relative_parent) = relative_path.parent() else {
        return ParsedSidecarMetadata::default();
    };
    let nfo_path = relative_parent
        .ancestors()
        .map(|directory| root_path.join(directory).join("tvshow.nfo"))
        .find(|candidate| candidate.is_file());

    read_series_sidecar_metadata_from_path(nfo_path)
}

fn read_series_sidecar_metadata_from_path(nfo_path: Option<PathBuf>) -> ParsedSidecarMetadata {
    let Some(nfo_path) = nfo_path else {
        return ParsedSidecarMetadata::default();
    };

    let Some(contents) = read_nfo_file(&nfo_path, MAX_TVSHOW_NFO_BYTES) else {
        return ParsedSidecarMetadata::default();
    };

    parse_nfo_metadata(
        &contents,
        nfo_path.parent().unwrap_or_else(|| Path::new("/")),
    )
}

fn read_nfo_file(path: &Path, max_bytes: usize) -> Option<String> {
    let mut file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) => {
            tracing::warn!(
                file_path = %path.display(),
                error = %error,
                "failed to open sidecar nfo file"
            );
            return None;
        }
    };
    let metadata = match file.metadata() {
        Ok(metadata) => metadata,
        Err(error) => {
            tracing::warn!(
                file_path = %path.display(),
                error = %error,
                "failed to inspect sidecar nfo file"
            );
            return None;
        }
    };

    if !metadata.is_file() {
        tracing::warn!(
            file_path = %path.display(),
            "sidecar nfo path is not a regular file"
        );
        return None;
    }
    if metadata.len() > max_bytes as u64 {
        tracing::warn!(
            file_path = %path.display(),
            file_bytes = metadata.len(),
            max_bytes,
            "sidecar nfo file exceeds the size limit"
        );
        return None;
    }

    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    let read_result = (&mut file)
        .take(max_bytes as u64 + 1)
        .read_to_end(&mut bytes);
    if let Err(error) = read_result {
        tracing::warn!(
            file_path = %path.display(),
            error = %error,
            "failed to read sidecar nfo file"
        );
        return None;
    }
    if bytes.len() > max_bytes {
        tracing::warn!(
            file_path = %path.display(),
            file_bytes = bytes.len(),
            max_bytes,
            "sidecar nfo file grew beyond the size limit while being read"
        );
        return None;
    }

    match String::from_utf8(bytes) {
        Ok(contents) => Some(contents),
        Err(error) => {
            tracing::warn!(
                file_path = %path.display(),
                error = %error,
                "sidecar nfo file is not valid UTF-8"
            );
            None
        }
    }
}

fn find_sidecar_nfo(video_path: &Path) -> Option<PathBuf> {
    let mut candidates = vec![video_path.with_extension("nfo")];

    if let Some(parent) = video_path.parent() {
        candidates.push(parent.join("movie.nfo"));
    }

    candidates.into_iter().find(|candidate| candidate.is_file())
}

pub(crate) fn parse_nfo_metadata(contents: &str, base_dir: &Path) -> ParsedSidecarMetadata {
    let document = NfoDocument::new(contents);
    let poster_path = document
        .extract("thumb")
        .as_deref()
        .and_then(|value| resolve_sidecar_reference(value, base_dir));
    let backdrop_path = extract_fanart_reference(&document)
        .as_deref()
        .and_then(|value| resolve_sidecar_reference(value, base_dir));

    ParsedSidecarMetadata {
        title: document.extract("title"),
        original_title: document.extract("originaltitle"),
        sort_title: document.extract("sorttitle"),
        year: document
            .extract("year")
            .and_then(|value| parse_year_token(&value)),
        overview: document
            .extract("plot")
            .or_else(|| document.extract("outline")),
        poster_path,
        backdrop_path,
    }
}

struct NfoDocument<'a> {
    contents: &'a str,
    lowercase: String,
}

impl<'a> NfoDocument<'a> {
    fn new(contents: &'a str) -> Self {
        Self {
            contents,
            lowercase: contents.to_ascii_lowercase(),
        }
    }

    fn extract(&self, tag: &str) -> Option<String> {
        extract_xml_tag_value(self.contents, &self.lowercase, tag)
    }

    fn extract_nested(&self, outer_tag: &str, inner_tag: &str) -> Option<String> {
        let range = find_xml_tag_value_range(&self.lowercase, outer_tag)?;
        extract_xml_tag_value(
            &self.contents[range.clone()],
            &self.lowercase[range],
            inner_tag,
        )
    }
}

fn extract_fanart_reference(document: &NfoDocument<'_>) -> Option<String> {
    document
        .extract_nested("fanart", "thumb")
        .or_else(|| document.extract("fanart"))
}

fn extract_xml_tag_value(contents: &str, lowercase: &str, tag: &str) -> Option<String> {
    let range = find_xml_tag_value_range(lowercase, tag)?;
    normalize_xml_text(&contents[range])
}

fn find_xml_tag_value_range(lowercase: &str, tag: &str) -> Option<Range<usize>> {
    let start_tag = format!("<{}", tag);
    let end_tag = format!("</{}>", tag);
    let mut search_from = 0;

    while let Some(relative_start) = lowercase[search_from..].find(&start_tag) {
        let start = search_from + relative_start;
        let boundary = lowercase.as_bytes().get(start + start_tag.len()).copied();

        if !matches!(
            boundary,
            Some(b'>') | Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r')
        ) {
            search_from = start + start_tag.len();
            continue;
        }

        let tag_end = lowercase[start + start_tag.len()..].find('>')? + start + start_tag.len();
        let value_start = tag_end + 1;
        let value_end = lowercase[value_start..].find(&end_tag)? + value_start;

        return Some(value_start..value_end);
    }

    None
}

fn normalize_xml_text(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    let without_cdata = trimmed
        .strip_prefix("<![CDATA[")
        .and_then(|inner| inner.strip_suffix("]]>"))
        .unwrap_or(trimmed);

    let normalized = without_cdata
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'");
    let normalized = normalized.trim();

    if normalized.is_empty() {
        None
    } else {
        Some(normalized.to_string())
    }
}

fn resolve_sidecar_reference(value: &str, base_dir: &Path) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }

    if is_external_url(value) {
        return Some(value.to_string());
    }

    let canonical_base = fs::canonicalize(base_dir).ok()?;
    let reference = Path::new(value);
    let resolved = if reference.is_absolute() {
        reference.to_path_buf()
    } else {
        base_dir.join(reference)
    };
    let canonical_reference = fs::canonicalize(resolved).ok()?;

    if !canonical_reference.starts_with(&canonical_base)
        || !is_supported_sidecar_image(&canonical_reference)
    {
        return None;
    }

    Some(canonical_reference.to_string_lossy().to_string())
}

fn is_supported_sidecar_image(path: &Path) -> bool {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase());
    if !matches!(
        extension.as_deref(),
        Some("jpg" | "jpeg" | "png" | "webp" | "gif" | "avif")
    ) {
        return false;
    }

    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() || metadata.len() == 0 {
        return false;
    }

    let Ok(mut file) = fs::File::open(path) else {
        return false;
    };
    let mut header = [0_u8; 16];
    let Ok(read) = file.read(&mut header) else {
        return false;
    };
    let header = &header[..read];

    match extension.as_deref() {
        Some("jpg" | "jpeg") => header.starts_with(&[0xff, 0xd8, 0xff]),
        Some("png") => header.starts_with(b"\x89PNG\r\n\x1a\n"),
        Some("webp") => {
            header.len() >= 12 && header.starts_with(b"RIFF") && &header[8..12] == b"WEBP"
        }
        Some("gif") => header.starts_with(b"GIF87a") || header.starts_with(b"GIF89a"),
        Some("avif") => {
            header.len() >= 12
                && &header[4..8] == b"ftyp"
                && matches!(&header[8..12], b"avif" | b"avis")
        }
        _ => false,
    }
}

pub(crate) fn find_local_artwork(video_path: &Path, kind: ArtworkKind) -> Option<String> {
    find_local_artwork_with_scope(video_path, kind, ArtworkScope::FileSpecific)
        .or_else(|| find_local_artwork_with_scope(video_path, kind, ArtworkScope::Generic))
}

pub(crate) fn find_local_artwork_with_scope(
    video_path: &Path,
    kind: ArtworkKind,
    scope: ArtworkScope,
) -> Option<String> {
    const IMAGE_EXTENSIONS: [&str; 5] = ["jpg", "jpeg", "png", "webp", "avif"];

    let parent = video_path.parent()?;
    let stem = video_path.file_stem()?.to_str()?;

    let name_candidates = match (kind, scope) {
        (ArtworkKind::Poster, ArtworkScope::FileSpecific) => {
            vec![
                stem.to_string(),
                format!("{stem}-poster"),
                format!("{stem}.poster"),
            ]
        }
        (ArtworkKind::Poster, ArtworkScope::Generic) => {
            vec![
                "poster".to_string(),
                "folder".to_string(),
                "cover".to_string(),
            ]
        }
        (ArtworkKind::Backdrop, ArtworkScope::FileSpecific) => vec![
            format!("{stem}-fanart"),
            format!("{stem}-backdrop"),
            format!("{stem}-background"),
        ],
        (ArtworkKind::Backdrop, ArtworkScope::Generic) => vec![
            "fanart".to_string(),
            "backdrop".to_string(),
            "background".to_string(),
        ],
    };

    for name in name_candidates {
        for extension in IMAGE_EXTENSIONS {
            let candidate = parent.join(format!("{name}.{extension}"));
            if is_non_empty_file(&candidate) {
                return Some(candidate.to_string_lossy().to_string());
            }
        }
    }

    None
}

fn is_non_empty_file(path: &Path) -> bool {
    fs::metadata(path)
        .map(|metadata| metadata.is_file() && metadata.len() > 0)
        .unwrap_or(false)
}

fn is_external_url(value: &str) -> bool {
    value.starts_with("http://") || value.starts_with("https://")
}

#[cfg(test)]
mod tests {
    use super::{
        read_series_sidecar_metadata, read_sidecar_metadata, ParsedSidecarMetadata,
        MAX_MEDIA_NFO_BYTES, MAX_TVSHOW_NFO_BYTES,
    };
    use std::{env, fs, path::PathBuf};
    use uuid::Uuid;

    fn unique_temp_path(kind: &str) -> PathBuf {
        env::temp_dir().join(format!("mova-scan-sidecar-{kind}-{}", Uuid::new_v4()))
    }

    fn create_sparse_file(path: &std::path::Path, bytes: usize) {
        let file = fs::File::create(path).expect("oversized nfo should be created");
        file.set_len(bytes as u64)
            .expect("oversized nfo length should be set");
    }

    #[test]
    fn oversized_movie_nfo_safely_falls_back_to_empty_metadata() {
        let root = unique_temp_path("oversized-movie");
        let video_path = root.join("Movie.2026.mkv");
        fs::create_dir_all(&root).unwrap();
        create_sparse_file(&root.join("movie.nfo"), MAX_MEDIA_NFO_BYTES + 1);

        let metadata = read_sidecar_metadata(&video_path);

        let _ = fs::remove_dir_all(&root);
        assert_eq!(metadata, ParsedSidecarMetadata::default());
    }

    #[test]
    fn oversized_episode_nfo_safely_falls_back_to_empty_metadata() {
        let root = unique_temp_path("oversized-episode");
        let video_path = root.join("Series.S01E01.mkv");
        fs::create_dir_all(&root).unwrap();
        create_sparse_file(&video_path.with_extension("nfo"), MAX_MEDIA_NFO_BYTES + 1);

        let metadata = read_sidecar_metadata(&video_path);

        let _ = fs::remove_dir_all(&root);
        assert_eq!(metadata, ParsedSidecarMetadata::default());
    }

    #[test]
    fn oversized_tvshow_nfo_safely_falls_back_to_empty_metadata() {
        let root = unique_temp_path("oversized-tvshow");
        let video_path = root.join("Season 01").join("Series.S01E01.mkv");
        fs::create_dir_all(video_path.parent().unwrap()).unwrap();
        create_sparse_file(&root.join("tvshow.nfo"), MAX_TVSHOW_NFO_BYTES + 1);

        let metadata = read_series_sidecar_metadata(&video_path);

        let _ = fs::remove_dir_all(&root);
        assert_eq!(metadata, ParsedSidecarMetadata::default());
    }

    #[test]
    fn invalid_utf8_nfo_safely_falls_back_to_empty_metadata() {
        let root = unique_temp_path("invalid-utf8");
        let video_path = root.join("Movie.2026.mkv");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("movie.nfo"), [0xff, 0xfe, 0xfd]).unwrap();

        let metadata = read_sidecar_metadata(&video_path);

        let _ = fs::remove_dir_all(&root);
        assert_eq!(metadata, ParsedSidecarMetadata::default());
    }
}
