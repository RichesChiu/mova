use super::*;
use mova_scan::{
    has_meaningful_file_title, infer_movie_container_identity, infer_series_container_identity,
    LocalNfoMetadata, MediaContainerIdentity,
};
use std::path::{Path, PathBuf};

pub(super) fn normalize_discovered_files_for_local_structure(
    discovered_files: Vec<DiscoveredMediaFile>,
) -> Vec<DiscoveredMediaFile> {
    normalize_discovered_files_for_local_structure_with_optional_root(discovered_files, None)
}

pub(super) fn normalize_discovered_files_for_local_structure_with_root(
    discovered_files: Vec<DiscoveredMediaFile>,
    root_path: &Path,
) -> Vec<DiscoveredMediaFile> {
    normalize_discovered_files_for_local_structure_with_optional_root(
        discovered_files,
        Some(root_path),
    )
}

fn normalize_discovered_files_for_local_structure_with_optional_root(
    mut discovered_files: Vec<DiscoveredMediaFile>,
    root_path: Option<&Path>,
) -> Vec<DiscoveredMediaFile> {
    discovered_files.sort_by(|left, right| left.file_path.cmp(&right.file_path));

    let mut groups = HashMap::<String, LocalSeriesGroup>::new();
    for (index, file) in discovered_files.iter().enumerate() {
        if file.season_number.is_none() || file.episode_number.is_none() {
            continue;
        }

        let group_seed = match root_path {
            Some(root_path) => local_series_group_seed_for_file_with_root(file, root_path),
            None => local_series_group_seed_for_file(file),
        };
        let Some(group_seed) = group_seed else {
            continue;
        };

        let group = groups
            .entry(group_seed.item_key.clone())
            .or_insert_with(|| LocalSeriesGroup {
                lookup_title: group_seed.lookup_title.clone(),
                display_title: group_seed.display_title.clone(),
                year: group_seed.year,
                year_priority: group_seed.year_priority,
                identity_priority: group_seed.identity_priority,
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

    if let Some(root_path) = root_path {
        for file in &mut discovered_files {
            apply_movie_container_identity_when_title_is_missing(file, root_path);
        }
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
    identity_priority: u8,
    season_number: i32,
    season_air_year: Option<MetadataSeasonAirYearHint>,
}

pub(super) fn local_series_group_seed_for_file(
    file: &DiscoveredMediaFile,
) -> Option<LocalSeriesGroupSeed> {
    local_series_group_seed_for_file_with_optional_root(file, None)
}

pub(super) fn local_series_group_seed_for_file_with_root(
    file: &DiscoveredMediaFile,
    root_path: &Path,
) -> Option<LocalSeriesGroupSeed> {
    local_series_group_seed_for_file_with_optional_root(file, Some(root_path))
}

fn local_series_group_seed_for_file_with_optional_root(
    file: &DiscoveredMediaFile,
    root_path: Option<&Path>,
) -> Option<LocalSeriesGroupSeed> {
    if file.season_number.is_some() && file.episode_number.is_some() {
        let file_metadata = infer_series_file_metadata(&file.file_path);
        let container_identity = root_path
            .and_then(|root_path| infer_series_container_identity(&file.file_path, root_path));
        let sidecar_title = file
            .series_sidecar_title
            .as_deref()
            .map(str::trim)
            .filter(|title| !title.is_empty());
        let fallback_title = file_metadata
            .as_ref()
            .map(|metadata| metadata.title.as_str())
            .or_else(|| {
                container_identity
                    .as_ref()
                    .map(|identity| identity.title.as_str())
            });
        let lookup_title = sidecar_title.or(fallback_title)?.to_string();
        let display_title = sidecar_title
            .map(str::to_string)
            .or_else(|| {
                file_metadata
                    .as_ref()
                    .map(|metadata| metadata.display_title.clone())
            })
            .or_else(|| {
                container_identity
                    .as_ref()
                    .map(|identity| identity.display_title.clone())
            })
            .unwrap_or_else(|| lookup_title.clone());
        let season_number = file_metadata
            .as_ref()
            .map(|metadata| metadata.season_number)
            .or(file.season_number)?;
        let filename_has_series_identity = file_metadata.is_some();
        let sidecar_year = file.series_sidecar_year;
        let file_first_air_year = file_metadata.as_ref().and_then(|metadata| metadata.year);
        let container_year = if filename_has_series_identity {
            None
        } else {
            container_identity
                .as_ref()
                .and_then(|identity| identity.year)
        };
        let year = sidecar_year.or(file_first_air_year).or(container_year);
        let year_priority = if sidecar_year.is_some() {
            3
        } else if season_number == 1 && file_first_air_year.is_some() {
            2
        } else if container_year.is_some() {
            1
        } else {
            0
        };
        let season_air_year = (sidecar_year.is_none() && container_year.is_none())
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
        let identity_priority = if sidecar_title.is_some() {
            3
        } else if filename_has_series_identity {
            2
        } else {
            1
        };

        return Some(LocalSeriesGroupSeed {
            item_key: series_group_item_key_with_optional_root(
                &file.file_path,
                &lookup_title,
                root_path,
                container_identity.as_ref(),
            ),
            lookup_title,
            display_title,
            year,
            year_priority,
            identity_priority,
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

    let should_replace_identity = (group_seed.identity_priority > group.identity_priority)
        || (group_seed.identity_priority == group.identity_priority
            && group_seed.season_number < group.identity_season_number);

    if should_replace_identity {
        group.lookup_title = group_seed.lookup_title.clone();
        group.display_title = group_seed.display_title.clone();
        group.identity_priority = group_seed.identity_priority;
        group.identity_season_number = group_seed.season_number;
    }
}

pub(super) fn series_group_item_key(file_path: &std::path::Path, title: &str) -> String {
    series_container_item_key(file_path).unwrap_or_else(|| series_title_item_key(title))
}

fn series_group_item_key_with_optional_root(
    file_path: &Path,
    title: &str,
    root_path: Option<&Path>,
    container_identity: Option<&MediaContainerIdentity>,
) -> String {
    let Some(root_path) = root_path else {
        return series_group_item_key(file_path, title);
    };

    if let Some(container_key) = series_container_item_key_within_root(file_path, root_path) {
        return container_key;
    }

    if let Some(identity) = container_identity {
        return container_item_key("series-folder", &identity.container_path);
    }

    series_title_item_key(title)
}

pub(super) fn series_title_item_key(title: &str) -> String {
    let normalized_title = title
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();

    format!("series-title:{normalized_title}")
}

pub(super) fn series_container_item_key_within_root(
    file_path: &Path,
    root_path: &Path,
) -> Option<String> {
    let relative_path = file_path.strip_prefix(root_path).ok()?;
    let parent = relative_path.parent()?;
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

    let container_path = directories[..season_directory_index]
        .iter()
        .collect::<std::path::PathBuf>();
    Some(container_item_key("series-folder", &container_path))
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

fn container_item_key(prefix: &str, container_path: &Path) -> String {
    let container_key = container_path
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .map(normalize_series_key_component)
        .filter(|component| !component.is_empty())
        .collect::<Vec<_>>()
        .join("/");

    format!("{prefix}:{container_key}")
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

#[derive(Debug, Clone, Copy)]
struct ExistingNfoProjection<'a> {
    restore_all: bool,
    previous_payload: Option<&'a serde_json::Value>,
    episode_scope: bool,
}

impl<'a> ExistingNfoProjection<'a> {
    fn for_refresh(
        has_local_nfo: bool,
        source_path: Option<&str>,
        previous_payload: Option<&'a serde_json::Value>,
        current_nfo: Option<&LocalNfoMetadata>,
        invalid_candidate_path: Option<&Path>,
        episode_scope: bool,
    ) -> Self {
        let restore_all = should_restore_last_known_good(
            has_local_nfo,
            source_path,
            current_nfo,
            invalid_candidate_path,
        );
        Self {
            restore_all,
            previous_payload,
            episode_scope,
        }
    }

    fn allows(self, field: &str) -> bool {
        self.restore_all
            || !previous_nfo_owns_field(self.previous_payload, field, self.episode_scope)
    }
}

fn previous_nfo_owns_field(
    payload: Option<&serde_json::Value>,
    field: &str,
    episode_scope: bool,
) -> bool {
    let Some(metadata) = payload.and_then(|payload| payload.get("metadata")) else {
        return false;
    };
    let has_text = |name: &str| {
        metadata
            .get(name)
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| !value.trim().is_empty())
    };
    let has_values = |name: &str| {
        metadata
            .get(name)
            .and_then(serde_json::Value::as_array)
            .is_some_and(|values| {
                values
                    .iter()
                    .any(|value| value.as_str().is_some_and(|value| !value.trim().is_empty()))
            })
    };
    let has_artwork = |name: &str| {
        metadata
            .get("artwork")
            .and_then(|artwork| artwork.get(name))
            .and_then(serde_json::Value::as_array)
            .is_some_and(|values| !values.is_empty())
    };

    match field {
        "title" | "source_title" => has_text("title"),
        "original_title" => has_text("original_title"),
        "sort_title" => has_text("sort_title"),
        "year" => metadata.get("year").is_some_and(|value| !value.is_null()),
        "tagline" => has_text("tagline"),
        "premiere_date" => has_text("premiered") || has_text("aired"),
        "content_rating" => has_text("content_rating"),
        "country" => has_values("countries"),
        "genres" => has_values("genres"),
        "studio" => has_values("studios"),
        "overview" => has_text("overview") || has_text("outline"),
        "poster" if episode_scope => has_artwork("thumbnails") || has_artwork("posters"),
        "poster" => has_artwork("posters"),
        "backdrop" => has_artwork("backdrops"),
        "logo" => has_artwork("logos"),
        _ => false,
    }
}

fn previous_nfo_owns_named_season(
    payload: Option<&serde_json::Value>,
    season_number: Option<i32>,
) -> bool {
    previous_nfo_owns_season_field(payload, season_number, "title")
}

fn previous_nfo_owns_season_field(
    payload: Option<&serde_json::Value>,
    season_number: Option<i32>,
    field: &str,
) -> bool {
    let Some(season_number) = season_number else {
        return false;
    };
    payload
        .and_then(|payload| payload.get("metadata"))
        .and_then(|metadata| metadata.get("named_seasons"))
        .and_then(serde_json::Value::as_array)
        .is_some_and(|seasons| {
            seasons.iter().any(|season| {
                if season
                    .get("season_number")
                    .and_then(serde_json::Value::as_i64)
                    != Some(i64::from(season_number))
                {
                    return false;
                }
                match field {
                    "title" | "overview" => season
                        .get(field)
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|value| !value.trim().is_empty()),
                    "poster" => season
                        .get("artwork")
                        .and_then(|artwork| artwork.get("posters"))
                        .and_then(serde_json::Value::as_array)
                        .is_some_and(|values| !values.is_empty()),
                    "backdrop" => season
                        .get("artwork")
                        .and_then(|artwork| artwork.get("backdrops"))
                        .and_then(serde_json::Value::as_array)
                        .is_some_and(|values| !values.is_empty()),
                    _ => false,
                }
            })
        })
}

pub(super) fn apply_existing_media_metadata(
    file: &mut DiscoveredMediaFile,
    summary: &mova_db::ExistingMediaMetadataSummary,
) {
    restore_last_known_good_nfo(
        &mut file.local_nfo,
        summary.has_local_nfo,
        summary.local_nfo_source_path.as_deref(),
        summary.local_nfo_payload.as_ref(),
        file.invalid_local_nfo_source_path.as_deref(),
    );
    restore_last_known_good_nfo(
        &mut file.series_local_nfo,
        summary.series_has_local_nfo,
        summary.series_local_nfo_source_path.as_deref(),
        summary.series_local_nfo_payload.as_ref(),
        file.invalid_series_local_nfo_source_path.as_deref(),
    );
    let local_baseline = file.clone();
    file.removed_local_nfo_source_path = removed_local_source_path(
        summary.has_local_nfo,
        summary.local_nfo_source_path.as_deref(),
        file.local_nfo.as_ref(),
        file.invalid_local_nfo_source_path.as_deref(),
    );
    file.removed_series_local_nfo_source_path = removed_local_source_path(
        summary.series_has_local_nfo,
        summary.series_local_nfo_source_path.as_deref(),
        file.series_local_nfo.as_ref(),
        file.invalid_series_local_nfo_source_path.as_deref(),
    );

    if summary.metadata_status != METADATA_STATUS_MATCHED
        || effective_existing_metadata_provider_item_id(summary).is_none()
    {
        return;
    }

    if summary.media_type.eq_ignore_ascii_case("episode") {
        let series_remote_snapshot = crate::tmdb_revalidation::parse_tmdb_remote_snapshot(
            summary.series_tmdb_remote_snapshot.as_ref(),
        );
        let remote_season = series_remote_snapshot.as_ref().and_then(|snapshot| {
            snapshot
                .series_outline
                .as_ref()?
                .seasons
                .iter()
                .find(|season| Some(season.season_number) == file.season_number)
        });
        let remote_episode = remote_season.and_then(|season| {
            season
                .episodes
                .iter()
                .find(|episode| Some(episode.episode_number) == file.episode_number)
        });
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
        let series_projection = ExistingNfoProjection::for_refresh(
            summary.series_has_local_nfo,
            summary.series_local_nfo_source_path.as_deref(),
            summary.series_local_nfo_payload.as_ref(),
            file.series_local_nfo.as_ref(),
            file.invalid_series_local_nfo_source_path.as_deref(),
            false,
        );
        {
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
            replace_option_if_present(&mut file.tagline, summary.series_tagline.as_ref());
            replace_string_option_if_present(
                &mut file.premiere_date,
                summary
                    .series_premiere_date
                    .map(|value| value.to_string())
                    .as_deref(),
            );
            replace_option_if_present(
                &mut file.content_rating,
                summary.series_content_rating.as_ref(),
            );
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
        }
        restore_parent_local_values(
            file,
            &local_baseline,
            series_projection,
            true,
            series_remote_snapshot.as_ref(),
        );
        if previous_nfo_owns_named_season(
            summary.series_local_nfo_payload.as_ref(),
            file.season_number,
        ) && !series_projection.restore_all
        {
            file.season_title = remote_season
                .and_then(|season| season.title.clone())
                .or_else(|| local_baseline.season_title.clone());
        } else {
            fill_option_ref_if_missing(&mut file.season_title, summary.season_title.as_ref());
        }
        if previous_nfo_owns_season_field(
            summary.series_local_nfo_payload.as_ref(),
            file.season_number,
            "overview",
        ) && !series_projection.restore_all
        {
            file.season_overview = remote_season
                .and_then(|season| season.overview.clone())
                .or_else(|| local_baseline.season_overview.clone());
        } else {
            fill_option_ref_if_missing(&mut file.season_overview, summary.season_overview.as_ref());
        }
        if previous_nfo_owns_season_field(
            summary.series_local_nfo_payload.as_ref(),
            file.season_number,
            "poster",
        ) && !series_projection.restore_all
        {
            file.season_poster_path = remote_season
                .and_then(|season| season.poster_path.clone())
                .or_else(|| local_baseline.season_poster_path.clone());
        } else {
            fill_option_ref_if_missing(
                &mut file.season_poster_path,
                summary.season_poster_path.as_ref(),
            );
        }
        if previous_nfo_owns_season_field(
            summary.series_local_nfo_payload.as_ref(),
            file.season_number,
            "backdrop",
        ) && !series_projection.restore_all
        {
            file.season_backdrop_path = remote_season
                .and_then(|season| season.backdrop_path.clone())
                .or_else(|| local_baseline.season_backdrop_path.clone());
        } else {
            fill_option_ref_if_missing(
                &mut file.season_backdrop_path,
                summary.season_backdrop_path.as_ref(),
            );
        }
        let episode_projection = ExistingNfoProjection::for_refresh(
            summary.has_local_nfo,
            summary.local_nfo_source_path.as_deref(),
            summary.local_nfo_payload.as_ref(),
            file.local_nfo.as_ref(),
            file.invalid_local_nfo_source_path.as_deref(),
            true,
        );
        {
            replace_option_if_present(&mut file.episode_title, summary.episode_title.as_ref());
            replace_option_if_present(
                &mut file.episode_original_title,
                summary.original_title.as_ref(),
            );
            replace_option_if_present(&mut file.episode_sort_title, summary.sort_title.as_ref());
            replace_copy_if_present(&mut file.episode_year, summary.year);
            replace_option_if_present(&mut file.episode_overview, summary.overview.as_ref());
            replace_option_if_present(&mut file.episode_tagline, summary.tagline.as_ref());
            replace_string_option_if_present(
                &mut file.episode_premiere_date,
                summary
                    .premiere_date
                    .map(|value| value.to_string())
                    .as_deref(),
            );
            replace_option_if_present(
                &mut file.episode_content_rating,
                summary.content_rating.as_ref(),
            );
            fill_option_ref_if_missing(&mut file.poster_path, summary.poster_path.as_ref());
            fill_option_ref_if_missing(&mut file.backdrop_path, summary.backdrop_path.as_ref());
            fill_option_ref_if_missing(&mut file.logo_path, summary.logo_path.as_ref());
        }
        restore_episode_local_values(file, &local_baseline, episode_projection, remote_episode);
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
    let movie_projection = ExistingNfoProjection::for_refresh(
        summary.has_local_nfo,
        summary.local_nfo_source_path.as_deref(),
        summary.local_nfo_payload.as_ref(),
        file.local_nfo.as_ref(),
        file.invalid_local_nfo_source_path.as_deref(),
        false,
    );
    {
        replace_string_if_present(&mut file.title, Some(summary.title.as_str()));
        fill_string_if_missing(&mut file.source_title, Some(summary.source_title.as_str()));
        replace_option_if_present(&mut file.original_title, summary.original_title.as_ref());
        replace_option_if_present(&mut file.sort_title, summary.sort_title.as_ref());
        replace_copy_if_present(&mut file.year, summary.year);
        replace_option_if_present(&mut file.tagline, summary.tagline.as_ref());
        replace_string_option_if_present(
            &mut file.premiere_date,
            summary
                .premiere_date
                .map(|value| value.to_string())
                .as_deref(),
        );
        replace_option_if_present(&mut file.content_rating, summary.content_rating.as_ref());
        replace_option_if_present(&mut file.country, summary.country.as_ref());
        replace_option_if_present(&mut file.genres, summary.genres.as_ref());
        replace_option_if_present(&mut file.studio, summary.studio.as_ref());
        replace_option_if_present(&mut file.overview, summary.overview.as_ref());
        fill_option_ref_if_missing(&mut file.poster_path, summary.poster_path.as_ref());
        fill_option_ref_if_missing(&mut file.backdrop_path, summary.backdrop_path.as_ref());
        fill_option_ref_if_missing(&mut file.logo_path, summary.logo_path.as_ref());
    }
    let remote_snapshot =
        crate::tmdb_revalidation::parse_tmdb_remote_snapshot(summary.tmdb_remote_snapshot.as_ref());
    restore_parent_local_values(
        file,
        &local_baseline,
        movie_projection,
        false,
        remote_snapshot.as_ref(),
    );
}

fn restore_last_known_good_nfo(
    current: &mut Option<LocalNfoMetadata>,
    has_local_nfo: bool,
    source_path: Option<&str>,
    payload: Option<&serde_json::Value>,
    invalid_candidate_path: Option<&Path>,
) {
    if current.is_some() || !has_local_nfo {
        return;
    }
    let Some(source_path) = source_path
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .filter(|value| {
            invalid_candidate_path.is_some_and(|candidate| {
                same_local_metadata_source_path(Path::new(value), candidate)
            })
        })
    else {
        return;
    };
    let Some(metadata) = payload.and_then(|payload| payload.get("metadata")) else {
        return;
    };
    let Ok(mut metadata) = serde_json::from_value::<LocalNfoMetadata>(metadata.clone()) else {
        return;
    };
    metadata.source_path = PathBuf::from(source_path);
    *current = Some(metadata);
}

fn restore_parent_local_values(
    file: &mut DiscoveredMediaFile,
    baseline: &DiscoveredMediaFile,
    projection: ExistingNfoProjection<'_>,
    is_series: bool,
    remote: Option<&crate::tmdb_revalidation::TmdbRemoteMetadataSnapshot>,
) {
    if !projection.allows("title") {
        file.title = remote
            .and_then(|snapshot| snapshot.title.clone())
            .unwrap_or_else(|| baseline.title.clone());
    }
    if !projection.allows("source_title") {
        file.source_title.clone_from(&baseline.source_title);
    }
    if !projection.allows("original_title") {
        file.original_title = remote
            .and_then(|snapshot| snapshot.original_title.clone())
            .or_else(|| baseline.original_title.clone());
    }
    if !projection.allows("sort_title") {
        file.sort_title.clone_from(&baseline.sort_title);
    }
    if !projection.allows("year") {
        file.year = remote.and_then(|snapshot| snapshot.year).or(baseline.year);
    }
    if !projection.allows("tagline") {
        file.tagline.clone_from(&baseline.tagline);
    }
    if !projection.allows("premiere_date") {
        file.premiere_date.clone_from(&baseline.premiere_date);
    }
    if !projection.allows("content_rating") {
        file.content_rating.clone_from(&baseline.content_rating);
    }
    if !projection.allows("country") {
        file.country = remote
            .and_then(|snapshot| snapshot.country.clone())
            .or_else(|| baseline.country.clone());
    }
    if !projection.allows("genres") {
        file.genres = remote
            .and_then(|snapshot| snapshot.genres.clone())
            .or_else(|| baseline.genres.clone());
    }
    if !projection.allows("studio") {
        file.studio = remote
            .and_then(|snapshot| snapshot.studio.clone())
            .or_else(|| baseline.studio.clone());
    }
    if !projection.allows("overview") {
        file.overview = remote
            .and_then(|snapshot| snapshot.overview.clone())
            .or_else(|| baseline.overview.clone());
    }

    if is_series {
        if !projection.allows("poster") {
            file.series_poster_path = remote
                .and_then(|snapshot| snapshot.poster_path.clone())
                .or_else(|| baseline.series_poster_path.clone());
        }
        if !projection.allows("backdrop") {
            file.series_backdrop_path = remote
                .and_then(|snapshot| snapshot.backdrop_path.clone())
                .or_else(|| baseline.series_backdrop_path.clone());
        }
        if !projection.allows("logo") {
            file.series_logo_path = remote
                .and_then(|snapshot| snapshot.logo_path.clone())
                .or_else(|| baseline.series_logo_path.clone());
        }
    } else {
        if !projection.allows("poster") {
            file.poster_path = remote
                .and_then(|snapshot| snapshot.poster_path.clone())
                .or_else(|| baseline.poster_path.clone());
        }
        if !projection.allows("backdrop") {
            file.backdrop_path = remote
                .and_then(|snapshot| snapshot.backdrop_path.clone())
                .or_else(|| baseline.backdrop_path.clone());
        }
        if !projection.allows("logo") {
            file.logo_path = remote
                .and_then(|snapshot| snapshot.logo_path.clone())
                .or_else(|| baseline.logo_path.clone());
        }
    }
}

fn restore_episode_local_values(
    file: &mut DiscoveredMediaFile,
    baseline: &DiscoveredMediaFile,
    projection: ExistingNfoProjection<'_>,
    remote: Option<&crate::metadata::RemoteSeriesEpisode>,
) {
    if !projection.allows("title") {
        file.episode_title = remote
            .and_then(|episode| episode.title.clone())
            .or_else(|| baseline.episode_title.clone());
    }
    if !projection.allows("original_title") {
        file.episode_original_title
            .clone_from(&baseline.episode_original_title);
    }
    if !projection.allows("sort_title") {
        file.episode_sort_title
            .clone_from(&baseline.episode_sort_title);
    }
    if !projection.allows("year") {
        file.episode_year = baseline.episode_year;
    }
    if !projection.allows("overview") {
        file.episode_overview = remote
            .and_then(|episode| episode.overview.clone())
            .or_else(|| baseline.episode_overview.clone());
    }
    if !projection.allows("tagline") {
        file.episode_tagline.clone_from(&baseline.episode_tagline);
    }
    if !projection.allows("premiere_date") {
        file.episode_premiere_date
            .clone_from(&baseline.episode_premiere_date);
    }
    if !projection.allows("content_rating") {
        file.episode_content_rating
            .clone_from(&baseline.episode_content_rating);
    }
    if !projection.allows("poster") {
        file.poster_path = remote
            .and_then(|episode| episode.poster_path.clone())
            .or_else(|| baseline.poster_path.clone());
    }
    if !projection.allows("backdrop") {
        file.backdrop_path = remote
            .and_then(|episode| episode.backdrop_path.clone())
            .or_else(|| baseline.backdrop_path.clone());
    }
    if !projection.allows("logo") {
        file.logo_path.clone_from(&baseline.logo_path);
    }
}

fn should_restore_last_known_good(
    has_local_nfo: bool,
    source_path: Option<&str>,
    current_nfo: Option<&LocalNfoMetadata>,
    invalid_candidate_path: Option<&Path>,
) -> bool {
    if current_nfo.is_some() {
        return false;
    }
    if !has_local_nfo {
        return true;
    }

    source_path
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_some_and(|value| {
            invalid_candidate_path.is_some_and(|candidate| {
                same_local_metadata_source_path(Path::new(value), candidate)
            })
        })
}

fn same_local_metadata_source_path(left: &Path, right: &Path) -> bool {
    left == right
        || match (std::fs::canonicalize(left), std::fs::canonicalize(right)) {
            (Ok(left), Ok(right)) => left == right,
            _ => false,
        }
}

fn removed_local_source_path(
    has_local_nfo: bool,
    source_path: Option<&str>,
    current_nfo: Option<&LocalNfoMetadata>,
    invalid_candidate_path: Option<&Path>,
) -> Option<String> {
    if !has_local_nfo {
        return None;
    }
    let source_path = source_path
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let source_path_ref = Path::new(source_path);
    let remains_current = current_nfo.is_some_and(|metadata| {
        same_local_metadata_source_path(source_path_ref, &metadata.source_path)
    }) || invalid_candidate_path
        .is_some_and(|candidate| same_local_metadata_source_path(source_path_ref, candidate));
    (!remains_current).then(|| source_path.to_string())
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
    build_scan_presentation_group_with_optional_root(file, None)
}

pub(super) fn build_scan_presentation_group_with_root(
    file: &DiscoveredMediaFile,
    root_path: &Path,
) -> ScanPresentationGroup {
    build_scan_presentation_group_with_optional_root(file, Some(root_path))
}

fn build_scan_presentation_group_with_optional_root(
    file: &DiscoveredMediaFile,
    root_path: Option<&Path>,
) -> ScanPresentationGroup {
    let media_type = effective_media_type(file);

    if media_type == "episode" {
        let container_identity = root_path
            .and_then(|root_path| infer_series_container_identity(&file.file_path, root_path));
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
                item_key: series_group_item_key_with_optional_root(
                    &file.file_path,
                    &lookup_title,
                    root_path,
                    container_identity.as_ref(),
                ),
                media_type: "series".to_string(),
                title,
                lookup_title,
                year,
                season_air_year,
            };
        }

        return ScanPresentationGroup {
            item_key: series_group_item_key_with_optional_root(
                &file.file_path,
                &file.source_title,
                root_path,
                container_identity.as_ref(),
            ),
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

    let container_identity = root_path
        .filter(|_| !has_meaningful_file_title(&file.file_path))
        .and_then(|root_path| infer_movie_container_identity(&file.file_path, root_path));

    ScanPresentationGroup {
        item_key: container_identity
            .as_ref()
            .map(|identity| container_item_key("movie-folder", &identity.container_path))
            .unwrap_or_else(|| file.file_path.to_string_lossy().to_string()),
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

#[cfg_attr(not(test), allow(dead_code))]
pub(super) fn group_discovered_files_for_scan(
    discovered_files: Vec<DiscoveredMediaFile>,
) -> Vec<ScanDiscoveredGroup> {
    group_discovered_files_for_scan_with_optional_root(discovered_files, None)
}

pub(super) fn group_discovered_files_for_scan_with_root(
    discovered_files: Vec<DiscoveredMediaFile>,
    root_path: &Path,
) -> Vec<ScanDiscoveredGroup> {
    group_discovered_files_for_scan_with_optional_root(discovered_files, Some(root_path))
}

fn group_discovered_files_for_scan_with_optional_root(
    discovered_files: Vec<DiscoveredMediaFile>,
    root_path: Option<&Path>,
) -> Vec<ScanDiscoveredGroup> {
    let discovered_files = match root_path {
        Some(root_path) => {
            normalize_discovered_files_for_local_structure_with_root(discovered_files, root_path)
        }
        None => normalize_discovered_files_for_local_structure(discovered_files),
    };
    let mut groups = Vec::<ScanDiscoveredGroup>::new();
    let mut group_indexes = HashMap::<String, usize>::new();

    for file in discovered_files {
        let mut presentation = match root_path {
            Some(root_path) => build_scan_presentation_group_with_root(&file, root_path),
            None => build_scan_presentation_group(&file),
        };
        if let Some(item_key) = local_nfo_container_group_key(&file) {
            presentation.item_key = item_key;
        }
        let explicit_tmdb_id =
            root_path.and_then(|root_path| explicit_container_tmdb_id(&file, root_path));

        if let Some(index) = group_indexes.get(&presentation.item_key).copied() {
            merge_metadata_lookup_hint(&mut groups[index], explicit_tmdb_id);
            groups[index].files.push(file);
            continue;
        }

        let next_index = groups.len();
        group_indexes.insert(presentation.item_key.clone(), next_index);
        groups.push(ScanDiscoveredGroup {
            presentation,
            files: vec![file],
            metadata_lookup_hint: explicit_tmdb_id,
            metadata_binding_conflict: false,
        });
    }

    groups
}

fn local_nfo_container_group_key(file: &DiscoveredMediaFile) -> Option<String> {
    if file.season_number.is_some() && file.episode_number.is_some() {
        if let Some(metadata) = file
            .series_local_nfo
            .as_ref()
            .filter(|metadata| metadata.kind == mova_scan::LocalNfoKind::TvShow)
        {
            return Some(format!(
                "series:nfo:{}",
                metadata.source_path.to_string_lossy()
            ));
        }
    }

    file.local_nfo
        .as_ref()
        .filter(|metadata| metadata.kind == mova_scan::LocalNfoKind::Movie)
        .filter(|metadata| {
            metadata
                .source_path
                .file_name()
                .is_some_and(|name| name.eq_ignore_ascii_case("movie.nfo"))
        })
        .map(|metadata| format!("movie:nfo:{}", metadata.source_path.to_string_lossy()))
}

pub(super) fn metadata_container_key_for_path(
    file_path: &Path,
    root_path: &Path,
    lookup_type: &str,
) -> Option<String> {
    let (normalized_type, identity) = if lookup_type.eq_ignore_ascii_case("series")
        || lookup_type.eq_ignore_ascii_case("episode")
    {
        (
            "series",
            infer_series_container_identity(file_path, root_path),
        )
    } else if lookup_type.eq_ignore_ascii_case("movie") {
        (
            "movie",
            infer_movie_container_identity(file_path, root_path),
        )
    } else {
        return None;
    };
    let identity = identity?;

    Some(format!(
        "{normalized_type}:{}",
        identity.container_path.to_string_lossy()
    ))
}

fn explicit_container_tmdb_id(file: &DiscoveredMediaFile, root_path: &Path) -> Option<String> {
    let identity = if file.season_number.is_some() && file.episode_number.is_some() {
        infer_series_container_identity(&file.file_path, root_path)
    } else {
        infer_movie_container_identity(&file.file_path, root_path)
    };

    identity.and_then(|identity| identity.tmdb_id)
}

pub(super) fn merge_metadata_lookup_hint(
    group: &mut ScanDiscoveredGroup,
    candidate: Option<String>,
) {
    let Some(candidate) = candidate else {
        return;
    };
    if group.metadata_binding_conflict {
        return;
    }

    match group.metadata_lookup_hint.as_deref() {
        None => group.metadata_lookup_hint = Some(candidate),
        Some(current) if current == candidate => {}
        Some(_) => {
            group.metadata_lookup_hint = None;
            group.metadata_binding_conflict = true;
        }
    }
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

#[cfg(test)]
mod last_known_good_tests {
    use super::should_restore_last_known_good;
    use mova_scan::{LocalNfoArtwork, LocalNfoCredits, LocalNfoKind, LocalNfoMetadata};
    use std::{env, fs, path::PathBuf};
    use uuid::Uuid;

    fn temporary_nfo_path(case: &str) -> PathBuf {
        env::temp_dir().join(format!("mova-{case}-{}.nfo", Uuid::new_v4()))
    }

    fn parsed_nfo(source_path: PathBuf) -> LocalNfoMetadata {
        LocalNfoMetadata {
            kind: LocalNfoKind::Movie,
            source_path,
            suppress_tmdb_identity_projection: false,
            title: Some("Current title".to_string()),
            original_title: None,
            sort_title: None,
            year: None,
            overview: None,
            outline: None,
            tagline: None,
            status: None,
            premiered: None,
            aired: None,
            date_added: None,
            runtime_minutes: None,
            content_rating: None,
            original_language: None,
            preferred_metadata_language: None,
            preferred_metadata_country_code: None,
            show_title: None,
            end_date: None,
            display_order: None,
            air_days: Vec::new(),
            air_time: None,
            custom_rating: None,
            trailers: Vec::new(),
            aspect_ratio: None,
            top_250: None,
            season_number: None,
            episode_number: None,
            season_count: None,
            episode_count: None,
            display_episode_number: None,
            display_season_number: None,
            display_after_season_number: None,
            show_link: None,
            genres: Vec::new(),
            countries: Vec::new(),
            studios: Vec::new(),
            tags: Vec::new(),
            styles: Vec::new(),
            named_seasons: Vec::new(),
            unique_ids: Vec::new(),
            episode_guide_ids: Vec::new(),
            ratings: Vec::new(),
            credits: LocalNfoCredits::default(),
            artwork: LocalNfoArtwork::default(),
            collection: None,
            lock_data: false,
            locked_fields: Vec::new(),
        }
    }

    #[test]
    fn restores_remote_projection_when_the_item_never_had_an_nfo() {
        assert!(should_restore_last_known_good(false, None, None, None));
    }

    #[test]
    fn restores_nfo_projection_only_when_the_old_file_still_exists_but_parsing_failed() {
        let source_path = temporary_nfo_path("nfo-parse-failure");
        fs::write(&source_path, "<movie>").expect("invalid NFO fixture should be written");

        assert!(should_restore_last_known_good(
            true,
            source_path.to_str(),
            None,
            Some(&source_path),
        ));

        let _ = fs::remove_file(source_path);
    }

    #[test]
    fn does_not_restore_old_projection_when_current_nfo_parsed_successfully() {
        let source_path = temporary_nfo_path("nfo-reparsed");
        fs::write(&source_path, "<movie />").expect("valid NFO fixture should be written");
        let current = parsed_nfo(source_path.clone());

        assert!(!should_restore_last_known_good(
            true,
            source_path.to_str(),
            Some(&current),
            None,
        ));

        let _ = fs::remove_file(source_path);
    }

    #[test]
    fn does_not_restore_old_projection_after_the_nfo_was_deleted() {
        let source_path = temporary_nfo_path("nfo-deleted");

        assert!(!should_restore_last_known_good(
            true,
            source_path.to_str(),
            None,
            None,
        ));
    }

    #[test]
    fn does_not_fall_back_past_a_new_invalid_higher_priority_nfo() {
        let persisted_source = temporary_nfo_path("movie-generic");
        let invalid_higher_priority = temporary_nfo_path("movie-specific-invalid");

        assert!(!should_restore_last_known_good(
            true,
            persisted_source.to_str(),
            None,
            Some(&invalid_higher_priority),
        ));
    }
}
