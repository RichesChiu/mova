use super::*;

pub(super) fn normalize_discovered_files_for_local_structure(
    mut discovered_files: Vec<DiscoveredMediaFile>,
) -> Vec<DiscoveredMediaFile> {
    discovered_files.sort_by(|left, right| left.file_path.cmp(&right.file_path));

    let mut groups = HashMap::<String, LocalSeriesGroup>::new();
    for (index, file) in discovered_files.iter().enumerate() {
        if file.season_number.is_none() || file.episode_number.is_none() {
            continue;
        }

        let Some(group_seed) = local_series_group_seed_for_file(file) else {
            continue;
        };

        let group = groups
            .entry(group_seed.item_key.clone())
            .or_insert_with(|| LocalSeriesGroup {
                lookup_title: group_seed.lookup_title.clone(),
                display_title: group_seed.display_title.clone(),
                year: group_seed.year,
                year_priority: group_seed.year_priority,
                identity_from_sidecar: group_seed.identity_from_sidecar,
                identity_season_number: group_seed.season_number,
                has_first_season: group_seed.season_number == 1,
                season_air_year: group_seed.season_air_year,
                file_indexes: Vec::new(),
                classified_episode_count: 0,
            });

        apply_local_series_group_seed(group, &group_seed);
        group.file_indexes.push(index);

        if file.season_number.is_some() && file.episode_number.is_some()
            || classify_media_type(&file.file_path).eq_ignore_ascii_case("episode")
        {
            group.classified_episode_count += 1;
        }
    }

    for mut group in groups.into_values() {
        let should_promote_to_series = should_promote_local_series_group(&group);

        if !should_promote_to_series {
            continue;
        }

        if group.year.is_some() || group.has_first_season {
            group.season_air_year = None;
        }

        assign_local_series_structure(&mut discovered_files, &group);
    }

    discovered_files
}

#[derive(Debug, Clone)]
pub(super) struct LocalSeriesGroupSeed {
    item_key: String,
    lookup_title: String,
    display_title: String,
    year: Option<i32>,
    year_priority: u8,
    identity_from_sidecar: bool,
    season_number: i32,
    season_air_year: Option<MetadataSeasonAirYearHint>,
}

pub(super) fn local_series_group_seed_for_file(
    file: &DiscoveredMediaFile,
) -> Option<LocalSeriesGroupSeed> {
    if file.season_number.is_some() && file.episode_number.is_some() {
        let file_metadata = infer_series_file_metadata(&file.file_path);
        let sidecar_title = file
            .series_sidecar_title
            .as_deref()
            .map(str::trim)
            .filter(|title| !title.is_empty());
        let fallback_title = file_metadata
            .as_ref()
            .map(|metadata| metadata.title.as_str());
        let lookup_title = sidecar_title.or(fallback_title)?.to_string();
        let display_title = sidecar_title
            .map(str::to_string)
            .or_else(|| {
                file_metadata
                    .as_ref()
                    .map(|metadata| metadata.display_title.clone())
            })
            .unwrap_or_else(|| lookup_title.clone());
        let season_number = file_metadata
            .as_ref()
            .map(|metadata| metadata.season_number)
            .or(file.season_number)?;
        let sidecar_year = file.series_sidecar_year;
        let file_first_air_year = file_metadata.as_ref().and_then(|metadata| metadata.year);
        let year = sidecar_year.or(file_first_air_year);
        let year_priority = if sidecar_year.is_some() {
            2
        } else if season_number == 1 && file_first_air_year.is_some() {
            1
        } else {
            0
        };
        let season_air_year = sidecar_year
            .is_none()
            .then(|| {
                file_metadata
                    .as_ref()
                    .and_then(|metadata| metadata.season_air_year)
                    .map(|year| MetadataSeasonAirYearHint {
                        season_number,
                        year,
                    })
            })
            .flatten();

        return Some(LocalSeriesGroupSeed {
            item_key: series_group_item_key(&file.file_path, &lookup_title),
            lookup_title,
            display_title,
            year,
            year_priority,
            identity_from_sidecar: sidecar_title.is_some(),
            season_number,
            season_air_year,
        });
    }

    None
}

pub(super) fn apply_local_series_group_seed(
    group: &mut LocalSeriesGroup,
    group_seed: &LocalSeriesGroupSeed,
) {
    group.has_first_season |= group_seed.season_number == 1;

    if group_seed.year_priority > group.year_priority {
        group.year = group_seed.year;
        group.year_priority = group_seed.year_priority;
    }

    if let Some(candidate) = group_seed.season_air_year {
        let should_replace_season_hint = group
            .season_air_year
            .is_none_or(|current| candidate.season_number < current.season_number);
        if should_replace_season_hint {
            group.season_air_year = Some(candidate);
        }
    }

    let should_replace_identity = (group_seed.identity_from_sidecar
        && !group.identity_from_sidecar)
        || (group_seed.identity_from_sidecar == group.identity_from_sidecar
            && group_seed.season_number < group.identity_season_number);

    if should_replace_identity {
        group.lookup_title = group_seed.lookup_title.clone();
        group.display_title = group_seed.display_title.clone();
        group.identity_from_sidecar = group_seed.identity_from_sidecar;
        group.identity_season_number = group_seed.season_number;
    }
}

pub(super) fn series_group_item_key(file_path: &std::path::Path, title: &str) -> String {
    series_container_item_key(file_path).unwrap_or_else(|| series_title_item_key(title))
}

pub(super) fn series_title_item_key(title: &str) -> String {
    let normalized_title = title
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();

    format!("series-title:{normalized_title}")
}

pub(super) fn series_container_item_key(file_path: &std::path::Path) -> Option<String> {
    let parent = file_path.parent()?;
    let mut directories = parent
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .filter(|component| !component.trim().is_empty())
        .collect::<Vec<_>>();

    while directories
        .last()
        .is_some_and(|directory| is_series_variant_directory_name(directory))
    {
        directories.pop();
    }

    let season_directory_index = directories
        .iter()
        .rposition(|directory| is_season_directory_name(directory))?;

    if season_directory_index == 0 {
        return None;
    }

    let container_key = directories[..season_directory_index]
        .iter()
        .map(|component| normalize_series_key_component(component))
        .collect::<Vec<_>>()
        .join("/");

    (!container_key.is_empty()).then(|| format!("series-folder:{container_key}"))
}

pub(super) fn normalize_series_key_component(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

pub(super) fn is_series_variant_directory_name(name: &str) -> bool {
    let normalized = name
        .trim()
        .replace(['.', '_', '-', '—', '–'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();

    matches!(
        normalized.as_str(),
        "dv" | "dovi" | "dolby vision" | "hdr" | "hdr10" | "hdr10+" | "sdr"
    ) || normalized.contains("杜比")
}

pub(super) fn is_season_directory_name(name: &str) -> bool {
    let normalized = name.trim().replace(['.', '_', '-', '—', '–'], " ");
    let normalized_lower = normalized.to_ascii_lowercase();
    let has_ascii_digit = normalized_lower.chars().any(|value| value.is_ascii_digit());

    if has_ascii_digit && normalized_lower.contains("season") {
        return true;
    }

    if has_ascii_digit && normalized.contains('季') {
        return true;
    }

    normalized_lower.split_whitespace().any(|token| {
        token.strip_prefix('s').is_some_and(|suffix| {
            !suffix.is_empty()
                && suffix.len() <= 2
                && suffix.chars().all(|value| value.is_ascii_digit())
        })
    })
}

pub(super) fn should_promote_local_series_group(group: &LocalSeriesGroup) -> bool {
    group.classified_episode_count > 0
}

pub(super) fn assign_local_series_structure(
    discovered_files: &mut [DiscoveredMediaFile],
    group: &LocalSeriesGroup,
) {
    let mut season_episode_indexes = HashMap::<i32, Vec<usize>>::new();

    for index in &group.file_indexes {
        let file = &mut discovered_files[*index];
        file.source_title = group.lookup_title.clone();
        if should_use_local_series_display_metadata(file) {
            file.title = group.display_title.clone();
            file.year = group.year;
        } else if file.year.is_none() {
            file.year = group.year;
        }

        let season_number = file.season_number.unwrap_or(1);
        file.season_number = Some(season_number);
        season_episode_indexes
            .entry(season_number)
            .or_default()
            .push(*index);
    }

    for indexes in season_episode_indexes.values_mut() {
        indexes.sort_by(|left, right| {
            discovered_files[*left]
                .file_path
                .cmp(&discovered_files[*right].file_path)
        });

        let mut next_episode_number = 1;
        let mut used_episode_numbers = HashSet::<i32>::new();

        for index in indexes.iter().copied() {
            if let Some(existing) = discovered_files[index].episode_number {
                used_episode_numbers.insert(existing);
                if existing >= next_episode_number {
                    next_episode_number = existing + 1;
                }
            }
        }

        for index in indexes.iter().copied() {
            let file = &mut discovered_files[index];

            if file.episode_number.is_none() {
                while used_episode_numbers.contains(&next_episode_number) {
                    next_episode_number += 1;
                }

                file.episode_number = Some(next_episode_number);
                used_episode_numbers.insert(next_episode_number);
                next_episode_number += 1;
            }

            if file.episode_title.is_none() {
                file.episode_title = Some(
                    file.file_path
                        .file_stem()
                        .and_then(|value| value.to_str())
                        .unwrap_or("Episode")
                        .replace(['.', '_'], " "),
                );
            }
        }
    }
}

pub(super) fn should_use_local_series_display_metadata(file: &DiscoveredMediaFile) -> bool {
    file.metadata_provider_item_id.is_none()
}

pub(super) fn effective_media_type(file: &DiscoveredMediaFile) -> &'static str {
    if file.season_number.is_some() && file.episode_number.is_some() {
        "episode"
    } else {
        classify_media_type(&file.file_path)
    }
}

pub(super) fn apply_existing_media_metadata(
    file: &mut DiscoveredMediaFile,
    summary: &mova_db::ExistingMediaMetadataSummary,
) {
    if summary.metadata_status != METADATA_STATUS_MATCHED
        || effective_existing_metadata_provider_item_id(summary).is_none()
    {
        return;
    }

    if summary.media_type.eq_ignore_ascii_case("episode") {
        replace_option_if_present(
            &mut file.metadata_provider,
            summary.metadata_provider.as_ref(),
        );
        replace_copy_if_present(
            &mut file.metadata_provider_item_id,
            summary.metadata_provider_item_id.clone(),
        );
        replace_string_option_if_present(
            &mut file.metadata_status,
            Some(summary.metadata_status.as_str()),
        );
        replace_string_option_if_present(
            &mut file.metadata_failure_reason,
            summary.metadata_failure_reason.as_deref(),
        );
        replace_string_option_if_present(
            &mut file.remote_media_type,
            summary.remote_media_type.as_deref(),
        );
        replace_string_if_present(&mut file.title, summary.series_title.as_deref());
        fill_string_if_missing(
            &mut file.source_title,
            summary.series_source_title.as_deref(),
        );
        replace_option_if_present(
            &mut file.original_title,
            summary.series_original_title.as_ref(),
        );
        replace_option_if_present(&mut file.sort_title, summary.series_sort_title.as_ref());
        replace_copy_if_present(&mut file.year, summary.series_year);
        replace_option_if_present(&mut file.country, summary.series_country.as_ref());
        replace_option_if_present(&mut file.genres, summary.series_genres.as_ref());
        replace_option_if_present(&mut file.studio, summary.series_studio.as_ref());
        replace_option_if_present(&mut file.overview, summary.series_overview.as_ref());
        fill_option_ref_if_missing(
            &mut file.series_poster_path,
            summary.series_poster_path.as_ref(),
        );
        fill_option_ref_if_missing(
            &mut file.series_backdrop_path,
            summary.series_backdrop_path.as_ref(),
        );
        fill_option_ref_if_missing(
            &mut file.series_logo_path,
            summary.series_logo_path.as_ref(),
        );
        fill_option_ref_if_missing(&mut file.season_title, summary.season_title.as_ref());
        fill_option_ref_if_missing(&mut file.season_overview, summary.season_overview.as_ref());
        fill_option_ref_if_missing(
            &mut file.season_poster_path,
            summary.season_poster_path.as_ref(),
        );
        fill_option_ref_if_missing(
            &mut file.season_backdrop_path,
            summary.season_backdrop_path.as_ref(),
        );
        replace_option_if_present(&mut file.episode_title, summary.episode_title.as_ref());
        fill_option_ref_if_missing(&mut file.poster_path, summary.poster_path.as_ref());
        fill_option_ref_if_missing(&mut file.backdrop_path, summary.backdrop_path.as_ref());
        fill_option_ref_if_missing(&mut file.logo_path, summary.logo_path.as_ref());
        return;
    }

    replace_option_if_present(
        &mut file.metadata_provider,
        summary.metadata_provider.as_ref(),
    );
    replace_copy_if_present(
        &mut file.metadata_provider_item_id,
        summary.metadata_provider_item_id.clone(),
    );
    replace_string_option_if_present(
        &mut file.metadata_status,
        Some(summary.metadata_status.as_str()),
    );
    replace_string_option_if_present(
        &mut file.metadata_failure_reason,
        summary.metadata_failure_reason.as_deref(),
    );
    replace_string_option_if_present(
        &mut file.remote_media_type,
        summary.remote_media_type.as_deref(),
    );
    replace_string_if_present(&mut file.title, Some(summary.title.as_str()));
    fill_string_if_missing(&mut file.source_title, Some(summary.source_title.as_str()));
    replace_option_if_present(&mut file.original_title, summary.original_title.as_ref());
    replace_option_if_present(&mut file.sort_title, summary.sort_title.as_ref());
    replace_copy_if_present(&mut file.year, summary.year);
    replace_option_if_present(&mut file.country, summary.country.as_ref());
    replace_option_if_present(&mut file.genres, summary.genres.as_ref());
    replace_option_if_present(&mut file.studio, summary.studio.as_ref());
    replace_option_if_present(&mut file.overview, summary.overview.as_ref());
    fill_option_ref_if_missing(&mut file.poster_path, summary.poster_path.as_ref());
    fill_option_ref_if_missing(&mut file.backdrop_path, summary.backdrop_path.as_ref());
    fill_option_ref_if_missing(&mut file.logo_path, summary.logo_path.as_ref());
}

pub(super) fn replace_string_if_present(target: &mut String, candidate: Option<&str>) {
    let Some(candidate) = candidate.map(str::trim).filter(|value| !value.is_empty()) else {
        return;
    };

    target.clear();
    target.push_str(candidate);
}

pub(super) fn fill_string_if_missing(target: &mut String, candidate: Option<&str>) {
    if !target.trim().is_empty() {
        return;
    }

    replace_string_if_present(target, candidate);
}

pub(super) fn replace_string_option_if_present(
    target: &mut Option<String>,
    candidate: Option<&str>,
) {
    let Some(candidate) = candidate.map(str::trim).filter(|value| !value.is_empty()) else {
        return;
    };

    *target = Some(candidate.to_string());
}

pub(super) fn fill_option_ref_if_missing<T: Clone>(target: &mut Option<T>, candidate: Option<&T>) {
    if target.is_some() {
        return;
    }

    *target = candidate.cloned();
}

pub(super) fn replace_option_if_present<T: Clone>(target: &mut Option<T>, candidate: Option<&T>) {
    if let Some(candidate) = candidate {
        *target = Some(candidate.clone());
    }
}

pub(super) fn replace_copy_if_present<T>(target: &mut Option<T>, candidate: Option<T>) {
    *target = candidate;
}

pub(super) fn is_cancelled(cancellation_flag: &Arc<AtomicBool>) -> bool {
    cancellation_flag.load(Ordering::SeqCst)
}

pub(super) fn build_scan_job_progress_update(
    scan_job: ScanJob,
    phase: &str,
) -> ScanJobProgressUpdate {
    ScanJobProgressUpdate {
        scan_job,
        phase: Some(phase.to_string()),
    }
}

pub(super) fn format_scan_phase_error(phase: &str, detail: impl AsRef<str>) -> String {
    format!("{}: {}", scan_phase_label(phase), detail.as_ref())
}

pub(super) fn scan_phase_label(phase: &str) -> &'static str {
    match phase {
        SCAN_PHASE_INITIALIZING => "Initialization failed",
        SCAN_PHASE_DISCOVERING => "Directory scan failed",
        SCAN_PHASE_PROCESSING => "Media processing failed",
        SCAN_PHASE_FINALIZING => "Library finalization failed",
        SCAN_PHASE_FINISHED => "Finalization failed",
        _ => "Scan job failed",
    }
}

pub(super) fn build_scan_presentation_group(file: &DiscoveredMediaFile) -> ScanPresentationGroup {
    let media_type = effective_media_type(file);

    if media_type == "episode" {
        if let Some(file_metadata) = infer_series_file_metadata(&file.file_path) {
            let source_title = file.source_title.trim();
            let lookup_title =
                if source_title.is_empty() || is_episode_like_source_title(source_title) {
                    file_metadata.title.clone()
                } else {
                    file.source_title.clone()
                };
            let file_title = file.title.trim();
            let title =
                if file_title.is_empty() || file_title.eq_ignore_ascii_case(&file_metadata.title) {
                    file_metadata.display_title
                } else {
                    file.title.clone()
                };
            let year = file.year.or(file_metadata.year);
            let season_air_year = year
                .is_none()
                .then(|| {
                    file_metadata
                        .season_air_year
                        .map(|year| MetadataSeasonAirYearHint {
                            season_number: file_metadata.season_number,
                            year,
                        })
                })
                .flatten();
            return ScanPresentationGroup {
                item_key: series_group_item_key(&file.file_path, &lookup_title),
                media_type: "series".to_string(),
                title,
                lookup_title,
                year,
                season_air_year,
            };
        }

        return ScanPresentationGroup {
            item_key: series_group_item_key(&file.file_path, &file.source_title),
            media_type: "series".to_string(),
            title: if file.title.trim().is_empty() {
                file.source_title.clone()
            } else {
                file.title.clone()
            },
            lookup_title: file.source_title.clone(),
            year: file.year,
            season_air_year: None,
        };
    }

    ScanPresentationGroup {
        item_key: file.file_path.to_string_lossy().to_string(),
        media_type: "movie".to_string(),
        title: if file.title.trim().is_empty() {
            file.source_title.clone()
        } else {
            file.title.clone()
        },
        lookup_title: file.source_title.clone(),
        year: file.year,
        season_air_year: None,
    }
}

pub(super) fn is_episode_like_source_title(value: &str) -> bool {
    let pseudo_file_name = format!("{value}.mkv");
    infer_series_file_metadata(std::path::Path::new(&pseudo_file_name)).is_some()
}

pub(super) fn group_discovered_files_for_scan(
    discovered_files: Vec<DiscoveredMediaFile>,
) -> Vec<ScanDiscoveredGroup> {
    let discovered_files = normalize_discovered_files_for_local_structure(discovered_files);
    let mut groups = Vec::<ScanDiscoveredGroup>::new();
    let mut group_indexes = HashMap::<String, usize>::new();

    for file in discovered_files {
        let presentation = build_scan_presentation_group(&file);

        if let Some(index) = group_indexes.get(&presentation.item_key).copied() {
            groups[index].files.push(file);
            continue;
        }

        let next_index = groups.len();
        group_indexes.insert(presentation.item_key.clone(), next_index);
        groups.push(ScanDiscoveredGroup {
            presentation,
            files: vec![file],
        });
    }

    groups
}

pub(super) fn build_scan_group_progress_update(
    scan_job_id: i64,
    library_id: i64,
    presentation: &ScanPresentationGroup,
    preview_file: Option<&DiscoveredMediaFile>,
    item_index: i32,
    total_items: i32,
    stage: ScanItemStage,
) -> ScanJobItemProgressUpdate {
    let (stage_name, progress_percent) = match stage {
        ScanItemStage::Analyzed => (SCAN_ITEM_STAGE_ANALYZED, 30),
        ScanItemStage::PendingCommitted => (SCAN_ITEM_STAGE_PENDING_COMMITTED, 40),
        ScanItemStage::Metadata => (SCAN_ITEM_STAGE_METADATA, 60),
        ScanItemStage::Artwork => (SCAN_ITEM_STAGE_ARTWORK, 85),
        ScanItemStage::Completed => (SCAN_ITEM_STAGE_COMPLETED, 100),
    };
    let artwork_preview_file = scan_progress_artwork_preview_file(stage, preview_file);

    ScanJobItemProgressUpdate {
        scan_job_id,
        library_id,
        item_key: presentation.item_key.clone(),
        media_type: presentation.media_type.clone(),
        title: presentation.title.clone(),
        year: preview_file
            .and_then(|file| file.year)
            .or(presentation.year),
        overview: scan_progress_overview(presentation, preview_file),
        poster_path: browser_visible_scan_artwork_path(scan_progress_poster_path(
            presentation,
            artwork_preview_file,
        )),
        backdrop_path: browser_visible_scan_artwork_path(scan_progress_backdrop_path(
            presentation,
            artwork_preview_file,
        )),
        metadata_status: preview_file.and_then(|file| file.metadata_status.clone()),
        remote_media_type: preview_file.and_then(|file| file.remote_media_type.clone()),
        season_number: None,
        episode_number: None,
        item_index,
        total_items,
        stage: stage_name.to_string(),
        progress_percent,
    }
}

pub(super) fn scan_progress_artwork_preview_file(
    stage: ScanItemStage,
    file: Option<&DiscoveredMediaFile>,
) -> Option<&DiscoveredMediaFile> {
    if matches!(stage, ScanItemStage::Completed) {
        file
    } else {
        None
    }
}

pub(super) fn browser_visible_scan_artwork_path(path: Option<String>) -> Option<String> {
    let path = path?;
    let trimmed = path.trim();

    if trimmed.starts_with("http://")
        || trimmed.starts_with("https://")
        || trimmed.starts_with("/api/")
    {
        Some(trimmed.to_string())
    } else {
        None
    }
}

pub(super) fn scan_progress_poster_path(
    presentation: &ScanPresentationGroup,
    file: Option<&DiscoveredMediaFile>,
) -> Option<String> {
    let file = file?;

    if presentation.media_type.eq_ignore_ascii_case("series") {
        return file.series_poster_path.clone();
    }

    file.poster_path.clone()
}

pub(super) fn scan_progress_backdrop_path(
    presentation: &ScanPresentationGroup,
    file: Option<&DiscoveredMediaFile>,
) -> Option<String> {
    let file = file?;

    if presentation.media_type.eq_ignore_ascii_case("series") {
        return file.series_backdrop_path.clone();
    }

    file.backdrop_path.clone()
}

pub(super) fn scan_progress_overview(
    presentation: &ScanPresentationGroup,
    file: Option<&DiscoveredMediaFile>,
) -> Option<String> {
    let file = file?;

    if presentation.media_type.eq_ignore_ascii_case("series") {
        return file
            .season_overview
            .clone()
            .or_else(|| file.overview.clone());
    }

    file.overview.clone()
}

impl From<MetadataEnrichmentStage> for ScanItemStage {
    fn from(value: MetadataEnrichmentStage) -> Self {
        match value {
            MetadataEnrichmentStage::Metadata => Self::Metadata,
            MetadataEnrichmentStage::Artwork => Self::Artwork,
            MetadataEnrichmentStage::Completed => Self::Completed,
        }
    }
}
