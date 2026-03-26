use super::parse::parse_year_token;
use std::{
    fs,
    path::{Path, PathBuf},
};

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

pub(crate) fn read_sidecar_metadata(path: &Path) -> ParsedSidecarMetadata {
    let Some(nfo_path) = find_sidecar_nfo(path) else {
        return ParsedSidecarMetadata::default();
    };

    let contents = match fs::read_to_string(&nfo_path) {
        Ok(contents) => contents,
        Err(error) => {
            tracing::warn!(
                file_path = %nfo_path.display(),
                error = %error,
                "failed to read sidecar nfo file"
            );
            return ParsedSidecarMetadata::default();
        }
    };

    parse_nfo_metadata(
        &contents,
        nfo_path.parent().unwrap_or_else(|| Path::new("/")),
    )
}

fn find_sidecar_nfo(video_path: &Path) -> Option<PathBuf> {
    let mut candidates = vec![video_path.with_extension("nfo")];

    if let Some(parent) = video_path.parent() {
        candidates.push(parent.join("movie.nfo"));
    }

    candidates.into_iter().find(|candidate| candidate.is_file())
}

pub(crate) fn parse_nfo_metadata(contents: &str, base_dir: &Path) -> ParsedSidecarMetadata {
    let poster_path = extract_xml_tag_value(contents, "thumb")
        .as_deref()
        .and_then(|value| resolve_sidecar_reference(value, base_dir));
    let backdrop_path = extract_fanart_reference(contents)
        .as_deref()
        .and_then(|value| resolve_sidecar_reference(value, base_dir));

    ParsedSidecarMetadata {
        title: extract_xml_tag_value(contents, "title"),
        original_title: extract_xml_tag_value(contents, "originaltitle"),
        sort_title: extract_xml_tag_value(contents, "sorttitle"),
        year: extract_xml_tag_value(contents, "year").and_then(|value| parse_year_token(&value)),
        overview: extract_xml_tag_value(contents, "plot")
            .or_else(|| extract_xml_tag_value(contents, "outline")),
        poster_path,
        backdrop_path,
    }
}

fn extract_fanart_reference(contents: &str) -> Option<String> {
    let fanart = extract_xml_tag_value(contents, "fanart")?;

    extract_xml_tag_value(&fanart, "thumb").or(Some(fanart))
}

fn extract_xml_tag_value(contents: &str, tag: &str) -> Option<String> {
    let lower = contents.to_ascii_lowercase();
    let tag = tag.to_ascii_lowercase();
    let start_tag = format!("<{}", tag);
    let end_tag = format!("</{}>", tag);
    let mut search_from = 0;

    while let Some(relative_start) = lower[search_from..].find(&start_tag) {
        let start = search_from + relative_start;
        let boundary = lower.as_bytes().get(start + start_tag.len()).copied();

        if !matches!(
            boundary,
            Some(b'>') | Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r')
        ) {
            search_from = start + start_tag.len();
            continue;
        }

        let tag_end = lower[start + start_tag.len()..].find('>')? + start + start_tag.len();
        let value_start = tag_end + 1;
        let value_end = lower[value_start..].find(&end_tag)? + value_start;

        return normalize_xml_text(&contents[value_start..value_end]);
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

    let reference = Path::new(value);
    let resolved = if reference.is_absolute() {
        reference.to_path_buf()
    } else {
        base_dir.join(reference)
    };

    resolved
        .is_file()
        .then(|| resolved.to_string_lossy().to_string())
}

pub(crate) fn find_local_artwork(video_path: &Path, kind: ArtworkKind) -> Option<String> {
    const IMAGE_EXTENSIONS: [&str; 5] = ["jpg", "jpeg", "png", "webp", "avif"];

    let parent = video_path.parent()?;
    let stem = video_path.file_stem()?.to_str()?;

    let name_candidates = match kind {
        ArtworkKind::Poster => vec![
            stem.to_string(),
            format!("{stem}-poster"),
            format!("{stem}.poster"),
            "poster".to_string(),
            "folder".to_string(),
            "cover".to_string(),
        ],
        ArtworkKind::Backdrop => vec![
            format!("{stem}-fanart"),
            format!("{stem}-backdrop"),
            format!("{stem}-background"),
            "fanart".to_string(),
            "backdrop".to_string(),
            "background".to_string(),
        ],
    };

    for name in name_candidates {
        for extension in IMAGE_EXTENSIONS {
            let candidate = parent.join(format!("{name}.{extension}"));
            if candidate.is_file() {
                return Some(candidate.to_string_lossy().to_string());
            }
        }
    }

    None
}

fn is_external_url(value: &str) -> bool {
    value.starts_with("http://") || value.starts_with("https://")
}
