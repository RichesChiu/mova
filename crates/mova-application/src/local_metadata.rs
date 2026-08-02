use crate::metadata::TMDB_PROVIDER_NAME;
use mova_db::{CreateLocalMetadataSnapshotParams, ReplaceMediaItemCreditParams};
use mova_domain::{MediaExternalId, MediaRating};
use mova_scan::{DiscoveredMediaFile, LocalNfoKind, LocalNfoMetadata};
use serde_json::json;
use std::{collections::BTreeMap, path::Path};
use time::{Date, Month, OffsetDateTime};

const PROVIDER_KEY_MAX_CHARACTERS: usize = 32;
const PROVIDER_IDENTIFIER_MAX_CHARACTERS: usize = 128;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct LocalMetadataSelection {
    pub tmdb_id_hint: Option<String>,
    pub identity_conflict: bool,
}

pub(crate) fn merge_lookup_hints(
    existing: Option<String>,
    selection: &LocalMetadataSelection,
) -> Option<String> {
    if selection.identity_conflict {
        return None;
    }
    match (existing, selection.tmdb_id_hint.as_deref()) {
        (Some(existing), Some(local)) if existing != local => None,
        (Some(existing), _) => Some(existing),
        (None, Some(local)) => Some(local.to_string()),
        (None, None) => None,
    }
}

pub(crate) fn build_local_metadata_snapshot(
    metadata: &LocalNfoMetadata,
    is_selected: bool,
) -> Option<CreateLocalMetadataSnapshotParams> {
    let source_path = metadata.source_path.to_string_lossy().trim().to_string();
    if source_path.is_empty() {
        return None;
    }

    let metadata = sanitized_snapshot_metadata(metadata);
    let normalized_metadata = serde_json::to_value(&metadata).ok()?;
    let payload = json!({
        "schema_version": 1,
        "tmdb_identity_projection_suppressed": metadata.suppress_tmdb_identity_projection,
        "metadata": normalized_metadata,
    });
    Some(CreateLocalMetadataSnapshotParams {
        source_path,
        document_type: match metadata.kind {
            LocalNfoKind::Movie => "movie",
            LocalNfoKind::TvShow => "tvshow",
            LocalNfoKind::Episode => "episodedetails",
        }
        .to_string(),
        is_locked: metadata.lock_data,
        is_selected,
        external_ids: snapshot_external_ids(&metadata),
        ratings: snapshot_ratings(&metadata),
        credits: snapshot_credits(&metadata),
        payload,
    })
}

pub(crate) fn build_local_metadata_snapshot_for_file(
    metadata: &LocalNfoMetadata,
    is_selected: bool,
    file: &DiscoveredMediaFile,
) -> Option<CreateLocalMetadataSnapshotParams> {
    let mut snapshot = build_local_metadata_snapshot(metadata, is_selected)?;
    let projection = match metadata.kind {
        LocalNfoKind::Movie => json!({
            "title": file.title,
            "source_title": file.source_title,
            "original_title": file.original_title,
            "sort_title": file.sort_title,
            "year": file.year,
            "tagline": file.tagline,
            "content_rating": file.content_rating,
            "country": file.country,
            "genres": file.genres,
            "studio": file.studio,
            "overview": file.overview,
            "poster_path": file.poster_path,
            "backdrop_path": file.backdrop_path,
            "logo_path": file.logo_path,
        }),
        LocalNfoKind::TvShow => json!({
            "title": file.title,
            "source_title": file.source_title,
            "original_title": file.original_title,
            "sort_title": file.sort_title,
            "year": file.year,
            "tagline": file.tagline,
            "content_rating": file.content_rating,
            "country": file.country,
            "genres": file.genres,
            "studio": file.studio,
            "overview": file.overview,
            "poster_path": file.series_poster_path,
            "backdrop_path": file.series_backdrop_path,
            "logo_path": file.series_logo_path,
        }),
        LocalNfoKind::Episode => json!({
            "title": file.episode_title,
            "source_title": file.episode_title,
            "original_title": file.episode_original_title,
            "sort_title": file.episode_sort_title,
            "year": file.episode_year,
            "tagline": file.episode_tagline,
            "content_rating": file.episode_content_rating,
            "overview": file.episode_overview,
            "poster_path": file.poster_path,
            "backdrop_path": file.backdrop_path,
            "logo_path": file.logo_path,
        }),
    };
    snapshot.payload["public_projection"] = projection;
    Some(snapshot)
}

pub(crate) fn parse_nfo_date(value: Option<&str>) -> Option<Date> {
    let value = value?.trim();
    let mut parts = value.split('-');
    let year = parts.next()?.parse::<i32>().ok()?;
    let month = Month::try_from(parts.next()?.parse::<u8>().ok()?).ok()?;
    let day = parts.next()?.parse::<u8>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Date::from_calendar_date(year, month, day).ok()
}

#[derive(Debug, Clone)]
struct ParentNfoCandidate {
    file_index: usize,
    metadata: LocalNfoMetadata,
    tmdb_id: Option<String>,
    has_tmdb_id_conflict: bool,
    is_file_specific: bool,
}

/// Selects one deterministic parent NFO for the shared movie/series projection,
/// then reapplies its user-owned fields. This is intentionally called both
/// before and after remote enrichment: TMDB may fill gaps, but it must not
/// overwrite fields explicitly present in the selected NFO.
pub(crate) fn apply_group_local_metadata(
    files: &mut [DiscoveredMediaFile],
    lookup_type: &str,
) -> LocalMetadataSelection {
    for file in files.iter_mut() {
        file.local_nfo_is_selected = false;
        file.series_local_nfo_is_selected = false;
        if let Some(metadata) = file.local_nfo.as_mut() {
            metadata.suppress_tmdb_identity_projection = false;
        }
        if let Some(metadata) = file.series_local_nfo.as_mut() {
            metadata.suppress_tmdb_identity_projection = false;
        }
    }

    let expected_kind = if lookup_type.eq_ignore_ascii_case("series") {
        LocalNfoKind::TvShow
    } else {
        LocalNfoKind::Movie
    };
    let accepted_tmdb_id = accepted_parent_tmdb_id(files);
    let mut candidates = files
        .iter()
        .enumerate()
        .filter_map(|(file_index, file)| {
            let metadata = match expected_kind {
                LocalNfoKind::TvShow => file.series_local_nfo.as_ref(),
                LocalNfoKind::Movie => file.local_nfo.as_ref(),
                LocalNfoKind::Episode => None,
            }?;
            if metadata.kind != expected_kind {
                return None;
            }
            let (tmdb_id, has_tmdb_id_conflict) = unique_tmdb_id(metadata);
            Some(ParentNfoCandidate {
                file_index,
                metadata: metadata.clone(),
                tmdb_id,
                has_tmdb_id_conflict,
                is_file_specific: is_file_specific_nfo(file, metadata),
            })
        })
        .collect::<Vec<_>>();

    candidates.sort_by(|left, right| {
        right
            .is_file_specific
            .cmp(&left.is_file_specific)
            .then_with(|| left.metadata.source_path.cmp(&right.metadata.source_path))
            .then_with(|| left.file_index.cmp(&right.file_index))
    });
    candidates.dedup_by(|left, right| left.metadata.source_path == right.metadata.source_path);

    let nfo_ids = candidates
        .iter()
        .filter_map(|candidate| candidate.tmdb_id.as_deref())
        .collect::<std::collections::BTreeSet<_>>();
    let has_internal_conflict = candidates
        .iter()
        .any(|candidate| candidate.has_tmdb_id_conflict);
    let has_cross_source_conflict = accepted_tmdb_id.is_none() && nfo_ids.len() > 1;
    let identity_conflict = has_internal_conflict || has_cross_source_conflict;

    let selected = if let Some(accepted_tmdb_id) = accepted_tmdb_id.as_deref() {
        candidates
            .iter()
            .filter(|candidate| !candidate.has_tmdb_id_conflict)
            .find(|candidate| candidate.tmdb_id.as_deref() == Some(accepted_tmdb_id))
            .or_else(|| {
                candidates.iter().find(|candidate| {
                    !candidate.has_tmdb_id_conflict && candidate.tmdb_id.is_none()
                })
            })
    } else if has_cross_source_conflict {
        candidates
            .iter()
            .find(|candidate| !candidate.has_tmdb_id_conflict)
    } else {
        candidates
            .iter()
            .find(|candidate| !candidate.has_tmdb_id_conflict)
    };

    let tmdb_id_hint = (!identity_conflict)
        .then(|| {
            selected
                .as_ref()
                .and_then(|selected| selected.tmdb_id.clone())
        })
        .flatten();
    if let Some(selected) = selected.cloned() {
        for file in files.iter_mut() {
            match expected_kind {
                LocalNfoKind::TvShow => {
                    file.series_local_nfo_is_selected =
                        file.series_local_nfo.as_mut().is_some_and(|metadata| {
                            let is_selected = metadata.source_path == selected.metadata.source_path;
                            metadata.suppress_tmdb_identity_projection =
                                is_selected && identity_conflict;
                            is_selected
                        });
                    apply_parent_projection(file, &selected.metadata, true);
                }
                LocalNfoKind::Movie => {
                    file.local_nfo_is_selected = file.local_nfo.as_mut().is_some_and(|metadata| {
                        let is_selected = metadata.source_path == selected.metadata.source_path;
                        metadata.suppress_tmdb_identity_projection =
                            is_selected && identity_conflict;
                        is_selected
                    });
                    apply_parent_projection(file, &selected.metadata, false);
                }
                LocalNfoKind::Episode => {}
            }
        }
    }
    apply_episode_group_projection(files);

    LocalMetadataSelection {
        tmdb_id_hint,
        identity_conflict,
    }
}

fn accepted_parent_tmdb_id(files: &[DiscoveredMediaFile]) -> Option<String> {
    files.iter().find_map(|file| {
        file.metadata_provider
            .as_deref()
            .filter(|provider| provider.eq_ignore_ascii_case(TMDB_PROVIDER_NAME))?;
        normalize_tmdb_id(file.metadata_provider_item_id.as_deref()?)
    })
}

fn unique_tmdb_id(metadata: &LocalNfoMetadata) -> (Option<String>, bool) {
    let ids = metadata
        .unique_ids
        .iter()
        .filter(|id| id.provider.eq_ignore_ascii_case(TMDB_PROVIDER_NAME))
        .filter_map(|id| normalize_tmdb_id(&id.value))
        .collect::<std::collections::BTreeSet<_>>();
    if ids.len() > 1 {
        return (None, true);
    }
    (ids.into_iter().next(), false)
}

fn normalize_tmdb_id(value: &str) -> Option<String> {
    let value = value.trim();
    value
        .parse::<u64>()
        .ok()
        .filter(|id| *id > 0)
        .map(|id| id.to_string())
}

fn sanitized_snapshot_metadata(metadata: &LocalNfoMetadata) -> LocalNfoMetadata {
    let mut metadata = metadata.clone();
    sanitize_snapshot_artwork(&mut metadata.artwork);
    for season in &mut metadata.named_seasons {
        sanitize_snapshot_artwork(&mut season.artwork);
    }
    for actor in &mut metadata.credits.actors {
        if actor
            .thumb_path
            .as_deref()
            .is_some_and(|value| !is_safe_snapshot_artwork(value))
        {
            actor.thumb_path = None;
        }
    }
    metadata
}

fn sanitize_snapshot_artwork(artwork: &mut mova_scan::LocalNfoArtwork) {
    artwork
        .posters
        .retain(|value| is_safe_snapshot_artwork(value));
    artwork
        .backdrops
        .retain(|value| is_safe_snapshot_artwork(value));
    artwork
        .logos
        .retain(|value| is_safe_snapshot_artwork(value));
    artwork
        .thumbnails
        .retain(|value| is_safe_snapshot_artwork(value));
    artwork
        .images
        .retain(|image| is_safe_snapshot_artwork(&image.path));
}

fn is_safe_snapshot_artwork(value: &str) -> bool {
    let value = value.trim();
    if !value.contains("://") {
        return Path::new(value).is_absolute();
    }
    value.starts_with("https://image.tmdb.org/t/p/")
}

fn snapshot_external_ids(metadata: &LocalNfoMetadata) -> Vec<MediaExternalId> {
    let mut values = BTreeMap::<String, Option<String>>::new();
    for item in &metadata.unique_ids {
        let provider = normalize_provider_key(&item.provider);
        let value = item.value.trim();
        if (metadata.suppress_tmdb_identity_projection
            && provider.eq_ignore_ascii_case(TMDB_PROVIDER_NAME))
            || provider.is_empty()
            || !fits_varchar(&provider, PROVIDER_KEY_MAX_CHARACTERS)
            || value.is_empty()
            || !fits_varchar(value, PROVIDER_IDENTIFIER_MAX_CHARACTERS)
        {
            continue;
        }
        values
            .entry(provider)
            .and_modify(|current| {
                if current.as_deref() != Some(value) {
                    *current = None;
                }
            })
            .or_insert_with(|| Some(value.to_string()));
    }
    values
        .into_iter()
        .filter_map(|(provider, external_id)| {
            Some(MediaExternalId {
                provider,
                external_id: external_id?,
            })
        })
        .collect()
}

fn snapshot_ratings(metadata: &LocalNfoMetadata) -> Vec<MediaRating> {
    let fetched_at = OffsetDateTime::now_utc();
    let mut ratings = BTreeMap::<(String, String), MediaRating>::new();
    for rating in &metadata.ratings {
        if !rating.value.is_finite()
            || !rating.scale.is_finite()
            || rating.scale <= 0.0
            || rating.value < 0.0
            || rating.value > rating.scale
        {
            continue;
        }
        let source = normalize_provider_key(&rating.source);
        if source.is_empty() || !fits_varchar(&source, PROVIDER_KEY_MAX_CHARACTERS) {
            continue;
        }
        let kind = match rating.kind {
            mova_scan::LocalNfoRatingKind::Audience => "audience",
            mova_scan::LocalNfoRatingKind::Critic => "critic",
        }
        .to_string();
        ratings
            .entry((source.clone(), kind.clone()))
            .or_insert_with(|| MediaRating {
                source,
                kind,
                score: rating.value,
                scale: rating.scale,
                rating_count: rating.votes.and_then(|votes| i64::try_from(votes).ok()),
                retrieved_via: "nfo".to_string(),
                attributes: json!({ "is_default": rating.is_default }),
                fetched_at,
            });
    }
    ratings.into_values().collect()
}

fn snapshot_credits(metadata: &LocalNfoMetadata) -> Vec<ReplaceMediaItemCreditParams> {
    let mut credits = Vec::new();
    for (index, actor) in metadata.credits.actors.iter().enumerate() {
        let Some(name) = non_empty(actor.name.as_str()) else {
            continue;
        };
        let person_id = actor
            .unique_ids
            .iter()
            .filter(|id| id.provider.eq_ignore_ascii_case(TMDB_PROVIDER_NAME))
            .find_map(|id| {
                non_empty(&id.value)
                    .filter(|value| fits_varchar(value, PROVIDER_IDENTIFIER_MAX_CHARACTERS))
            });
        credits.push(ReplaceMediaItemCreditParams {
            credit_type: "actor".to_string(),
            sort_order: actor
                .order
                .and_then(|order| i32::try_from(order).ok())
                .unwrap_or_else(|| i32::try_from(index).unwrap_or(i32::MAX)),
            person_provider: person_id.as_ref().map(|_| TMDB_PROVIDER_NAME.to_string()),
            person_id,
            name,
            role: actor.role.as_deref().and_then(non_empty),
            profile_path: actor
                .thumb_path
                .as_deref()
                .filter(|value| is_safe_snapshot_artwork(value))
                .and_then(non_empty),
        });
    }
    for (index, name) in metadata.credits.directors.iter().enumerate() {
        if let Some(name) = non_empty(name) {
            credits.push(ReplaceMediaItemCreditParams {
                credit_type: "director".to_string(),
                sort_order: i32::try_from(index).unwrap_or(i32::MAX),
                person_provider: None,
                person_id: None,
                name,
                role: None,
                profile_path: None,
            });
        }
    }
    for (index, name) in metadata.credits.writers.iter().enumerate() {
        if let Some(name) = non_empty(name) {
            credits.push(ReplaceMediaItemCreditParams {
                credit_type: "writer".to_string(),
                sort_order: i32::try_from(index).unwrap_or(i32::MAX),
                person_provider: None,
                person_id: None,
                name,
                role: None,
                profile_path: None,
            });
        }
    }
    credits
}

fn normalize_provider_key(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|character| {
            if character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}

fn fits_varchar(value: &str, max_characters: usize) -> bool {
    value.chars().take(max_characters + 1).count() <= max_characters
}

fn non_empty(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn is_file_specific_nfo(file: &DiscoveredMediaFile, metadata: &LocalNfoMetadata) -> bool {
    if metadata.kind == LocalNfoKind::TvShow {
        return false;
    }
    let expected = file.file_path.with_extension("nfo");
    same_path(&expected, &metadata.source_path)
}

fn same_path(left: &Path, right: &Path) -> bool {
    let left = left.canonicalize().unwrap_or_else(|_| left.to_path_buf());
    let right = right.canonicalize().unwrap_or_else(|_| right.to_path_buf());
    left == right
}

fn apply_parent_projection(
    file: &mut DiscoveredMediaFile,
    metadata: &LocalNfoMetadata,
    is_series: bool,
) {
    replace_non_empty_string(&mut file.title, metadata.title.as_deref());
    replace_non_empty_string(&mut file.source_title, metadata.title.as_deref());
    replace_optional(&mut file.original_title, metadata.original_title.as_deref());
    replace_optional(&mut file.sort_title, metadata.sort_title.as_deref());
    if metadata.year.is_some() {
        file.year = metadata.year;
    }
    replace_optional(
        &mut file.country,
        joined_values(&metadata.countries).as_deref(),
    );
    replace_optional(&mut file.genres, joined_values(&metadata.genres).as_deref());
    replace_optional(
        &mut file.studio,
        joined_values(&metadata.studios).as_deref(),
    );
    replace_optional(
        &mut file.overview,
        metadata.overview.as_deref().or(metadata.outline.as_deref()),
    );
    replace_optional(&mut file.tagline, metadata.tagline.as_deref());
    replace_optional(
        &mut file.premiere_date,
        metadata.premiered.as_deref().or(metadata.aired.as_deref()),
    );
    replace_optional(&mut file.content_rating, metadata.content_rating.as_deref());

    if is_series {
        if let Some(season_number) = file.season_number {
            if let Some(season) = metadata
                .named_seasons
                .iter()
                .find(|season| season.season_number == season_number)
            {
                replace_optional(&mut file.season_title, season.title.as_deref());
                replace_optional(&mut file.season_overview, season.overview.as_deref());
                replace_optional(
                    &mut file.season_poster_path,
                    season.artwork.posters.first().map(String::as_str),
                );
                replace_optional(
                    &mut file.season_backdrop_path,
                    season.artwork.backdrops.first().map(String::as_str),
                );
            }
        }
        replace_optional(
            &mut file.series_poster_path,
            metadata.artwork.posters.first().map(String::as_str),
        );
        replace_optional(
            &mut file.series_backdrop_path,
            metadata.artwork.backdrops.first().map(String::as_str),
        );
        replace_optional(
            &mut file.series_logo_path,
            metadata.artwork.logos.first().map(String::as_str),
        );
    } else {
        replace_optional(
            &mut file.poster_path,
            metadata.artwork.posters.first().map(String::as_str),
        );
        replace_optional(
            &mut file.backdrop_path,
            metadata.artwork.backdrops.first().map(String::as_str),
        );
        replace_optional(
            &mut file.logo_path,
            metadata.artwork.logos.first().map(String::as_str),
        );
    }
}

fn apply_episode_group_projection(files: &mut [DiscoveredMediaFile]) {
    let mut selections = BTreeMap::<(i32, i32), (LocalNfoMetadata, bool)>::new();
    for file in files.iter() {
        let (Some(season_number), Some(episode_number), Some(metadata)) = (
            file.season_number,
            file.episode_number,
            file.local_nfo
                .as_ref()
                .filter(|metadata| metadata.kind == LocalNfoKind::Episode),
        ) else {
            continue;
        };
        if metadata
            .season_number
            .is_some_and(|value| value != season_number)
            || metadata
                .episode_number
                .is_some_and(|value| value != episode_number)
        {
            tracing::warn!(
                file_path = %file.file_path.display(),
                nfo_path = %metadata.source_path.display(),
                file_season = season_number,
                file_episode = episode_number,
                nfo_season = ?metadata.season_number,
                nfo_episode = ?metadata.episode_number,
                "episode NFO coordinates conflict with the media filename"
            );
            continue;
        }
        let (_, candidate_id_conflict) = unique_tmdb_id(metadata);
        if candidate_id_conflict {
            tracing::warn!(
                file_path = %file.file_path.display(),
                nfo_path = %metadata.source_path.display(),
                "episode NFO declares multiple TMDB identities"
            );
            continue;
        }
        let key = (season_number, episode_number);
        match selections.get_mut(&key) {
            Some((selected, has_conflict)) => {
                let (selected_id, selected_conflict) = unique_tmdb_id(selected);
                let (candidate_id, candidate_conflict) = unique_tmdb_id(metadata);
                *has_conflict |= selected_conflict
                    || candidate_conflict
                    || (selected_id.is_some()
                        && candidate_id.is_some()
                        && selected_id != candidate_id);
                if metadata.source_path < selected.source_path {
                    *selected = metadata.clone();
                }
            }
            None => {
                let (_, has_conflict) = unique_tmdb_id(metadata);
                selections.insert(key, (metadata.clone(), has_conflict));
            }
        }
    }

    for file in files.iter_mut() {
        let Some(key) = file.season_number.zip(file.episode_number) else {
            continue;
        };
        let Some((selected, has_conflict)) = selections.get(&key) else {
            continue;
        };
        file.local_nfo_is_selected = file.local_nfo.as_mut().is_some_and(|metadata| {
            let is_selected = metadata.kind == LocalNfoKind::Episode
                && metadata.source_path == selected.source_path;
            metadata.suppress_tmdb_identity_projection = is_selected && *has_conflict;
            is_selected
        });
        if *has_conflict {
            tracing::warn!(
                season_number = key.0,
                episode_number = key.1,
                selected_nfo_path = %selected.source_path.display(),
                "conflicting episode NFO identities use a stable display source without creating a binding hint"
            );
        }
        apply_episode_projection_from_metadata(file, selected);
    }
}

fn apply_episode_projection_from_metadata(
    file: &mut DiscoveredMediaFile,
    metadata: &LocalNfoMetadata,
) {
    replace_optional(&mut file.episode_title, metadata.title.as_deref());
    replace_optional(
        &mut file.episode_original_title,
        metadata.original_title.as_deref(),
    );
    replace_optional(&mut file.episode_sort_title, metadata.sort_title.as_deref());
    if metadata.year.is_some() {
        file.episode_year = metadata.year;
    }
    replace_optional(
        &mut file.episode_overview,
        metadata.overview.as_deref().or(metadata.outline.as_deref()),
    );
    replace_optional(&mut file.episode_tagline, metadata.tagline.as_deref());
    replace_optional(
        &mut file.episode_premiere_date,
        metadata.aired.as_deref().or(metadata.premiered.as_deref()),
    );
    replace_optional(
        &mut file.episode_content_rating,
        metadata.content_rating.as_deref(),
    );
    replace_optional(
        &mut file.poster_path,
        metadata
            .artwork
            .thumbnails
            .first()
            .or_else(|| metadata.artwork.posters.first())
            .map(String::as_str),
    );
    replace_optional(
        &mut file.backdrop_path,
        metadata.artwork.backdrops.first().map(String::as_str),
    );
}

fn joined_values(values: &[String]) -> Option<String> {
    (!values.is_empty()).then(|| values.join(" · "))
}

fn replace_non_empty_string(target: &mut String, value: Option<&str>) {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return;
    };
    target.clear();
    target.push_str(value);
}

fn replace_optional(target: &mut Option<String>, value: Option<&str>) {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return;
    };
    *target = Some(value.to_string());
}

#[cfg(test)]
mod tests {
    use super::{
        apply_group_local_metadata, build_local_metadata_snapshot, unique_tmdb_id,
        PROVIDER_IDENTIFIER_MAX_CHARACTERS, PROVIDER_KEY_MAX_CHARACTERS,
    };
    use mova_scan::{
        inspect_media_file_inventory_shallow, DiscoveredMediaFile, DiscoveredMediaFileInventory,
        LocalNfoActor, LocalNfoArtwork, LocalNfoCredits, LocalNfoKind, LocalNfoMetadata,
        LocalNfoNamedSeason, LocalNfoRating, LocalNfoRatingKind, LocalNfoUniqueId,
    };
    use std::path::PathBuf;

    fn metadata(ids: &[&str]) -> LocalNfoMetadata {
        metadata_at(LocalNfoKind::Movie, "/media/movie.nfo", None, ids)
    }

    fn metadata_at(
        kind: LocalNfoKind,
        source_path: &str,
        title: Option<&str>,
        ids: &[&str],
    ) -> LocalNfoMetadata {
        LocalNfoMetadata {
            kind,
            source_path: PathBuf::from(source_path),
            suppress_tmdb_identity_projection: false,
            title: title.map(str::to_string),
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
            unique_ids: ids
                .iter()
                .map(|id| LocalNfoUniqueId {
                    provider: "tmdb".to_string(),
                    value: (*id).to_string(),
                    is_default: false,
                })
                .collect(),
            episode_guide_ids: Vec::new(),
            ratings: Vec::new(),
            credits: LocalNfoCredits::default(),
            artwork: LocalNfoArtwork::default(),
            collection: None,
            lock_data: false,
            locked_fields: Vec::new(),
        }
    }

    fn discovered_file(path: &str) -> DiscoveredMediaFile {
        inspect_media_file_inventory_shallow(DiscoveredMediaFileInventory {
            file_path: PathBuf::from(path),
            file_size: 1,
            file_modified_at_ms: Some(1),
            sidecar_fingerprint: String::new(),
        })
        .expect("shallow discovered media file")
    }

    #[test]
    fn duplicate_equivalent_tmdb_ids_are_one_hint() {
        assert_eq!(
            unique_tmdb_id(&metadata(&["42", "0042"])),
            (Some("42".into()), false)
        );
    }

    #[test]
    fn snapshot_projection_rejects_overlong_varchar_values_but_keeps_normalized_payload() {
        let overlong_provider = "p".repeat(PROVIDER_KEY_MAX_CHARACTERS + 1);
        let overlong_external_id = "e".repeat(PROVIDER_IDENTIFIER_MAX_CHARACTERS + 1);
        let overlong_rating_source = "s".repeat(PROVIDER_KEY_MAX_CHARACTERS + 1);
        let overlong_person_id = "a".repeat(PROVIDER_IDENTIFIER_MAX_CHARACTERS + 1);
        let mut nfo = metadata(&[]);
        nfo.unique_ids = vec![
            LocalNfoUniqueId {
                provider: "tmdb".to_string(),
                value: "42".to_string(),
                is_default: true,
            },
            LocalNfoUniqueId {
                provider: overlong_provider.clone(),
                value: "provider-too-long".to_string(),
                is_default: false,
            },
            LocalNfoUniqueId {
                provider: "imdb".to_string(),
                value: overlong_external_id.clone(),
                is_default: false,
            },
        ];
        nfo.ratings = vec![
            LocalNfoRating {
                source: "tmdb".to_string(),
                kind: LocalNfoRatingKind::Audience,
                value: 8.5,
                scale: 10.0,
                votes: Some(10),
                is_default: true,
            },
            LocalNfoRating {
                source: overlong_rating_source.clone(),
                kind: LocalNfoRatingKind::Audience,
                value: 7.0,
                scale: 10.0,
                votes: None,
                is_default: false,
            },
        ];
        nfo.credits.actors = vec![
            LocalNfoActor {
                name: "Safe Actor".to_string(),
                role: None,
                order: Some(0),
                thumb_path: None,
                person_type: None,
                unique_ids: vec![LocalNfoUniqueId {
                    provider: "tmdb".to_string(),
                    value: "100".to_string(),
                    is_default: true,
                }],
                profile: None,
            },
            LocalNfoActor {
                name: "Actor With Overlong ID".to_string(),
                role: None,
                order: Some(1),
                thumb_path: None,
                person_type: None,
                unique_ids: vec![LocalNfoUniqueId {
                    provider: "tmdb".to_string(),
                    value: overlong_person_id.clone(),
                    is_default: true,
                }],
                profile: None,
            },
        ];

        let snapshot = build_local_metadata_snapshot(&nfo, true)
            .expect("overlong projected fields must not reject the whole NFO snapshot");

        assert_eq!(snapshot.external_ids.len(), 1);
        assert_eq!(snapshot.external_ids[0].provider, "tmdb");
        assert_eq!(snapshot.external_ids[0].external_id, "42");
        assert_eq!(snapshot.ratings.len(), 1);
        assert_eq!(snapshot.ratings[0].source, "tmdb");
        assert_eq!(snapshot.credits.len(), 2);
        assert_eq!(snapshot.credits[0].person_id.as_deref(), Some("100"));
        assert_eq!(snapshot.credits[1].person_id, None);

        let payload_metadata = &snapshot.payload["metadata"];
        assert!(payload_metadata["unique_ids"]
            .as_array()
            .is_some_and(|ids| ids.iter().any(|id| id["provider"] == overlong_provider)));
        assert!(payload_metadata["unique_ids"]
            .as_array()
            .is_some_and(|ids| { ids.iter().any(|id| id["value"] == overlong_external_id) }));
        assert!(payload_metadata["ratings"]
            .as_array()
            .is_some_and(|ratings| ratings
                .iter()
                .any(|rating| rating["source"] == overlong_rating_source)));
        assert!(payload_metadata["credits"]["actors"]
            .as_array()
            .is_some_and(|actors| actors.iter().any(|actor| actor["unique_ids"]
                .as_array()
                .is_some_and(|ids| ids.iter().any(|id| id["value"] == overlong_person_id)))));
    }

    #[test]
    fn different_tmdb_ids_are_an_identity_conflict() {
        assert_eq!(unique_tmdb_id(&metadata(&["42", "43"])), (None, true));
    }

    #[test]
    fn movie_nfo_overrides_owned_fields_and_preserves_missing_remote_fields() {
        let mut file = discovered_file("/media/Example.Movie.2026.mkv");
        file.title = "Remote title".to_string();
        file.source_title = "Remote title".to_string();
        file.original_title = Some("Remote original title".to_string());
        file.genres = Some("Remote genre".to_string());
        file.overview = Some("Remote overview".to_string());
        file.tagline = Some("Remote tagline".to_string());
        file.poster_path = Some("/cache/remote-poster.jpg".to_string());

        let mut nfo = metadata_at(
            LocalNfoKind::Movie,
            "/media/Example.Movie.2026.nfo",
            Some("Local title"),
            &[],
        );
        nfo.genres = vec!["Drama".to_string(), "Mystery".to_string()];
        nfo.artwork.posters = vec!["/media/local-poster.jpg".to_string()];
        file.local_nfo = Some(nfo);

        let selection = apply_group_local_metadata(std::slice::from_mut(&mut file), "movie");

        assert_eq!(selection.tmdb_id_hint, None);
        assert!(!selection.identity_conflict);
        assert!(file.local_nfo_is_selected);
        assert_eq!(file.title, "Local title");
        assert_eq!(file.source_title, "Local title");
        assert_eq!(file.genres.as_deref(), Some("Drama · Mystery"));
        assert_eq!(file.poster_path.as_deref(), Some("/media/local-poster.jpg"));
        assert_eq!(
            file.original_title.as_deref(),
            Some("Remote original title")
        );
        assert_eq!(file.overview.as_deref(), Some("Remote overview"));
        assert_eq!(file.tagline.as_deref(), Some("Remote tagline"));
    }

    #[test]
    fn series_nfo_projects_structured_season_metadata() {
        let mut file = discovered_file("/media/Show/Season 02/Show.S02E01.mkv");
        file.season_number = Some(2);
        let mut nfo = metadata_at(
            LocalNfoKind::TvShow,
            "/media/Show/tvshow.nfo",
            Some("Local show"),
            &[],
        );
        nfo.named_seasons = vec![LocalNfoNamedSeason {
            season_number: 2,
            title: Some("The Second Chapter".to_string()),
            overview: Some("Local season overview".to_string()),
            artwork: LocalNfoArtwork {
                posters: vec!["/media/Show/season02-poster.jpg".to_string()],
                backdrops: vec!["/media/Show/season02-fanart.jpg".to_string()],
                ..LocalNfoArtwork::default()
            },
        }];
        file.series_local_nfo = Some(nfo);

        apply_group_local_metadata(std::slice::from_mut(&mut file), "series");

        assert_eq!(file.season_title.as_deref(), Some("The Second Chapter"));
        assert_eq!(
            file.season_overview.as_deref(),
            Some("Local season overview")
        );
        assert_eq!(
            file.season_poster_path.as_deref(),
            Some("/media/Show/season02-poster.jpg")
        );
        assert_eq!(
            file.season_backdrop_path.as_deref(),
            Some("/media/Show/season02-fanart.jpg")
        );
    }

    #[test]
    fn episode_nfo_tmdb_id_never_becomes_the_parent_series_hint() {
        let mut file = discovered_file("/media/Show/Show.S01E01.mkv");
        file.local_nfo = Some(metadata_at(
            LocalNfoKind::Episode,
            "/media/Show/Show.S01E01.nfo",
            Some("Local episode"),
            &["999"],
        ));

        let selection = apply_group_local_metadata(std::slice::from_mut(&mut file), "series");

        assert_eq!(selection.tmdb_id_hint, None);
        assert!(!selection.identity_conflict);
        assert_eq!(file.episode_title.as_deref(), Some("Local episode"));
        assert!(file.local_nfo_is_selected);
    }

    #[test]
    fn episode_nfo_identity_is_not_compared_with_the_parent_series_binding() {
        let mut file = discovered_file("/media/Show/Show.S01E01.mkv");
        file.metadata_provider = Some("tmdb".to_string());
        file.metadata_provider_item_id = Some("100".to_string());
        file.local_nfo = Some(metadata_at(
            LocalNfoKind::Episode,
            "/media/Show/Show.S01E01.nfo",
            Some("Episode-owned title"),
            &["9001"],
        ));

        let selection = apply_group_local_metadata(std::slice::from_mut(&mut file), "series");

        assert_eq!(selection.tmdb_id_hint, None);
        assert_eq!(file.metadata_provider_item_id.as_deref(), Some("100"));
        assert_eq!(file.episode_title.as_deref(), Some("Episode-owned title"));
        assert!(file.local_nfo_is_selected);
    }

    #[test]
    fn conflicting_episode_nfo_ids_keep_one_stable_display_source_selected() {
        let mut first = discovered_file("/media/Show/Show.S01E01.1080p.mkv");
        first.local_nfo = Some(metadata_at(
            LocalNfoKind::Episode,
            "/media/Show/Show.S01E01.1080p.nfo",
            Some("Stable episode title"),
            &["42"],
        ));
        let mut second = discovered_file("/media/Show/Show.S01E01.2160p.mkv");
        second.local_nfo = Some(metadata_at(
            LocalNfoKind::Episode,
            "/media/Show/Show.S01E01.2160p.nfo",
            Some("Conflicting episode title"),
            &["43"],
        ));
        let mut files = vec![first, second];

        let selection = apply_group_local_metadata(&mut files, "series");

        assert_eq!(selection.tmdb_id_hint, None);
        assert_eq!(
            files[0].episode_title.as_deref(),
            Some("Stable episode title")
        );
        assert_eq!(
            files[1].episode_title.as_deref(),
            Some("Stable episode title")
        );
        assert!(files[0].local_nfo_is_selected);
        assert!(!files[1].local_nfo_is_selected);
        let snapshot = build_local_metadata_snapshot(
            files[0].local_nfo.as_ref().expect("selected episode NFO"),
            true,
        )
        .expect("conflicting episode NFO snapshot");
        assert!(snapshot.external_ids.is_empty());
        assert_eq!(snapshot.payload["metadata"]["unique_ids"][0]["value"], "42");
    }

    #[test]
    fn different_parent_nfo_tmdb_ids_in_one_group_report_identity_conflict() {
        let mut first = discovered_file("/media/Movie/Movie.1080p.mkv");
        first.local_nfo = Some(metadata_at(
            LocalNfoKind::Movie,
            "/media/Movie/Movie.1080p.nfo",
            Some("Stable first display"),
            &["42"],
        ));
        let mut second = discovered_file("/media/Movie/Movie.2160p.mkv");
        second.local_nfo = Some(metadata_at(
            LocalNfoKind::Movie,
            "/media/Movie/Movie.2160p.nfo",
            Some("Conflicting second display"),
            &["43"],
        ));
        let mut files = vec![first, second];

        let selection = apply_group_local_metadata(&mut files, "movie");

        assert!(selection.identity_conflict);
        assert_eq!(selection.tmdb_id_hint, None);
        assert_eq!(files[0].title, "Stable first display");
        assert_eq!(files[1].title, "Stable first display");
        assert!(files[0].local_nfo_is_selected);
        assert!(!files[1].local_nfo_is_selected);
        let snapshot = build_local_metadata_snapshot(
            files[0].local_nfo.as_ref().expect("selected movie NFO"),
            true,
        )
        .expect("conflicting movie NFO snapshot");
        assert!(snapshot.external_ids.is_empty());
        assert_eq!(snapshot.payload["metadata"]["unique_ids"][0]["value"], "42");
    }

    #[test]
    fn existing_tmdb_binding_selects_the_nfo_with_the_same_id() {
        let mut first = discovered_file("/media/Movie/Movie.1080p.mkv");
        first.metadata_provider = Some("tmdb".to_string());
        first.metadata_provider_item_id = Some("43".to_string());
        first.local_nfo = Some(metadata_at(
            LocalNfoKind::Movie,
            "/media/Movie/Movie.1080p.nfo",
            Some("Wrong identity"),
            &["42"],
        ));
        let mut second = discovered_file("/media/Movie/Movie.2160p.mkv");
        second.local_nfo = Some(metadata_at(
            LocalNfoKind::Movie,
            "/media/Movie/Movie.2160p.nfo",
            Some("Accepted identity"),
            &["43"],
        ));
        let mut files = vec![first, second];

        let selection = apply_group_local_metadata(&mut files, "movie");

        assert_eq!(selection.tmdb_id_hint.as_deref(), Some("43"));
        assert!(!selection.identity_conflict);
        assert_eq!(files[0].title, "Accepted identity");
        assert_eq!(files[1].title, "Accepted identity");
        assert!(!files[0].local_nfo_is_selected);
        assert!(files[1].local_nfo_is_selected);
    }

    #[test]
    fn same_title_nfos_select_lexical_source_stably_and_mark_only_that_source() {
        let mut later = discovered_file("/media/Movie/Movie.B.mkv");
        let mut later_nfo = metadata_at(
            LocalNfoKind::Movie,
            "/media/Movie/Movie.B.nfo",
            Some("Same title"),
            &[],
        );
        later_nfo.overview = Some("Later source".to_string());
        later.local_nfo = Some(later_nfo);

        let mut earlier = discovered_file("/media/Movie/Movie.A.mkv");
        let mut earlier_nfo = metadata_at(
            LocalNfoKind::Movie,
            "/media/Movie/Movie.A.nfo",
            Some("Same title"),
            &[],
        );
        earlier_nfo.overview = Some("Earlier source".to_string());
        earlier.local_nfo = Some(earlier_nfo);
        let mut files = vec![later, earlier];

        let selection = apply_group_local_metadata(&mut files, "movie");

        assert!(!selection.identity_conflict);
        assert_eq!(files[0].overview.as_deref(), Some("Earlier source"));
        assert_eq!(files[1].overview.as_deref(), Some("Earlier source"));
        assert!(!files[0].local_nfo_is_selected);
        assert!(files[1].local_nfo_is_selected);
    }
}
