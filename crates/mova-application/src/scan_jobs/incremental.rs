use super::*;

pub(super) async fn build_incremental_scan_plan(
    pool: &PgPool,
    library_id: i64,
    root_path: &std::path::Path,
    discovered_files: Vec<DiscoveredMediaFileInventory>,
    metadata_provider_enabled: bool,
    metadata_language: &str,
) -> ApplicationResult<IncrementalScanPlan> {
    let file_paths = discovered_files
        .iter()
        .map(|file| file.file_path.to_string_lossy().to_string())
        .collect::<Vec<_>>();
    let discovered_path_set = file_paths
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let memberships = mova_db::list_library_media_file_memberships(pool, library_id)
        .await
        .map_err(ApplicationError::Unexpected)?;
    let owners_with_removed_versions = memberships
        .iter()
        .filter(|membership| !discovered_path_set.contains(membership.file_path.as_str()))
        .map(|membership| membership.logical_metadata_owner_id)
        .collect::<HashSet<_>>();

    let existing_metadata =
        mova_db::list_existing_media_metadata_for_file_paths(pool, library_id, &file_paths)
            .await
            .map_err(ApplicationError::Unexpected)?;
    let container_bindings = build_container_binding_index(existing_metadata.as_slice(), root_path);

    let existing_by_path = existing_metadata
        .into_iter()
        .map(|summary| (summary.file_path.clone(), summary))
        .collect::<HashMap<_, _>>();

    let mut touched_owner_ids = owners_with_removed_versions;
    for inventory in &discovered_files {
        let file_path = inventory.file_path.to_string_lossy();
        let Some(summary) = existing_by_path.get(file_path.as_ref()) else {
            continue;
        };
        let scan_hash = discovered_media_file_inventory_scan_hash(inventory);
        if !can_skip_existing_media_summary(
            summary,
            scan_hash.as_str(),
            metadata_provider_enabled,
            metadata_language,
            &inventory.file_path,
        ) {
            touched_owner_ids.insert(summary.logical_metadata_owner_id);
        }
    }

    let mut scan_files = Vec::with_capacity(discovered_files.len());

    for inventory in discovered_files {
        let file_path = inventory.file_path.to_string_lossy().to_string();
        let scan_hash = discovered_media_file_inventory_scan_hash(&inventory);

        let existing_metadata = existing_by_path.get(file_path.as_str()).cloned();
        let requires_processing = existing_metadata.as_ref().is_none_or(|summary| {
            touched_owner_ids.contains(&summary.logical_metadata_owner_id)
                || !can_skip_existing_media_summary(
                    summary,
                    scan_hash.as_str(),
                    metadata_provider_enabled,
                    metadata_language,
                    &inventory.file_path,
                )
        });
        scan_files.push(IncrementalScanFile {
            inventory,
            existing_metadata,
            requires_processing,
        });
    }

    hydrate_incremental_scan_file_cached_tracks(pool, &mut scan_files).await;

    Ok(IncrementalScanPlan {
        discovered_paths: file_paths,
        scan_files,
        container_bindings,
    })
}

pub(super) fn build_container_binding_index(
    existing_metadata: &[mova_db::ExistingMediaMetadataSummary],
    root_path: &std::path::Path,
) -> HashMap<String, ContainerBindingResolution> {
    let mut ids_by_container = HashMap::<String, HashSet<String>>::new();

    for summary in existing_metadata {
        if summary.metadata_status != METADATA_STATUS_MATCHED
            || !effective_existing_metadata_provider(summary)
                .is_some_and(|provider| provider.eq_ignore_ascii_case(TMDB_PROVIDER_NAME))
        {
            continue;
        }

        let lookup_type = if summary.media_type.eq_ignore_ascii_case("episode") {
            if !summary
                .remote_media_type
                .as_deref()
                .is_some_and(|value| value.eq_ignore_ascii_case(REMOTE_MEDIA_TYPE_SERIES))
            {
                continue;
            }
            "series"
        } else if summary.media_type.eq_ignore_ascii_case("movie") {
            if !summary
                .remote_media_type
                .as_deref()
                .is_some_and(|value| value.eq_ignore_ascii_case(REMOTE_MEDIA_TYPE_MOVIE))
            {
                continue;
            }
            "movie"
        } else {
            continue;
        };

        let Some(provider_item_id) = effective_existing_metadata_provider_item_id(summary)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let Some(container_key) = metadata_container_key_for_path(
            std::path::Path::new(&summary.file_path),
            root_path,
            lookup_type,
        ) else {
            continue;
        };

        ids_by_container
            .entry(container_key)
            .or_default()
            .insert(provider_item_id);
    }

    ids_by_container
        .into_iter()
        .filter_map(|(key, ids)| {
            if ids.is_empty() {
                return None;
            }

            let resolution = if ids.len() == 1 {
                ContainerBindingResolution::Unique(
                    ids.into_iter()
                        .next()
                        .expect("one-element binding set must contain an id"),
                )
            } else {
                ContainerBindingResolution::Conflict
            };
            Some((key, resolution))
        })
        .collect()
}

pub(super) async fn hydrate_incremental_scan_file_cached_tracks(
    pool: &PgPool,
    changed_files: &mut [IncrementalScanFile],
) {
    let reusable_media_file_ids = changed_files
        .iter()
        .filter_map(|changed_file| {
            let existing_metadata = changed_file.existing_metadata.as_ref()?;
            let scan_hash = discovered_media_file_inventory_scan_hash(&changed_file.inventory);

            can_reuse_cached_local_analysis(existing_metadata, scan_hash.as_str())
                .then_some(existing_metadata.media_file_id)
        })
        .collect::<Vec<_>>();

    if reusable_media_file_ids.is_empty() {
        return;
    }
    let reusable_media_file_id_set = reusable_media_file_ids
        .iter()
        .copied()
        .collect::<HashSet<_>>();

    let audio_tracks =
        match mova_db::list_audio_tracks_for_media_files(pool, &reusable_media_file_ids).await {
            Ok(audio_tracks) => audio_tracks,
            Err(error) => {
                tracing::warn!(
                    media_file_count = reusable_media_file_ids.len(),
                    error = ?error,
                    "failed to batch-load cached audio tracks; falling back to fresh local analysis"
                );
                invalidate_reusable_local_analysis(changed_files, &reusable_media_file_id_set);
                return;
            }
        };
    let subtitle_tracks = match mova_db::list_subtitle_files_for_media_files(
        pool,
        &reusable_media_file_ids,
    )
    .await
    {
        Ok(subtitle_tracks) => subtitle_tracks,
        Err(error) => {
            tracing::warn!(
                media_file_count = reusable_media_file_ids.len(),
                error = ?error,
                "failed to batch-load cached subtitle tracks; falling back to fresh local analysis"
            );
            invalidate_reusable_local_analysis(changed_files, &reusable_media_file_id_set);
            return;
        }
    };

    let mut audio_tracks_by_media_file = HashMap::new();
    for track in audio_tracks {
        audio_tracks_by_media_file
            .entry(track.media_file_id)
            .or_insert_with(Vec::new)
            .push(track);
    }
    let mut subtitle_tracks_by_media_file = HashMap::new();
    for subtitle in subtitle_tracks {
        subtitle_tracks_by_media_file
            .entry(subtitle.media_file_id)
            .or_insert_with(Vec::new)
            .push(subtitle);
    }

    for changed_file in changed_files {
        let Some(existing_metadata) = changed_file.existing_metadata.as_mut() else {
            continue;
        };
        if !reusable_media_file_id_set.contains(&existing_metadata.media_file_id) {
            continue;
        }

        existing_metadata.audio_tracks = audio_tracks_by_media_file
            .remove(&existing_metadata.media_file_id)
            .unwrap_or_default()
            .into_iter()
            .map(|track| mova_db::CreateAudioTrackParams {
                stream_index: track.stream_index,
                language: track.language,
                audio_codec: track.audio_codec,
                label: track.label,
                channel_layout: track.channel_layout,
                channels: track.channels,
                bitrate: track.bitrate,
                sample_rate: track.sample_rate,
                is_default: track.is_default,
            })
            .collect();
        existing_metadata.subtitle_tracks = subtitle_tracks_by_media_file
            .remove(&existing_metadata.media_file_id)
            .unwrap_or_default()
            .into_iter()
            .map(|subtitle| mova_db::CreateSubtitleTrackParams {
                source_kind: subtitle.source_kind,
                file_path: subtitle.file_path,
                stream_index: subtitle.stream_index,
                language: subtitle.language,
                subtitle_format: subtitle.subtitle_format,
                label: subtitle.label,
                is_default: subtitle.is_default,
                is_forced: subtitle.is_forced,
                is_hearing_impaired: subtitle.is_hearing_impaired,
            })
            .collect();
    }
}

pub(super) fn invalidate_reusable_local_analysis(
    changed_files: &mut [IncrementalScanFile],
    reusable_media_file_ids: &HashSet<i64>,
) {
    for changed_file in changed_files {
        if changed_file
            .existing_metadata
            .as_ref()
            .is_some_and(|metadata| reusable_media_file_ids.contains(&metadata.media_file_id))
        {
            changed_file.existing_metadata = None;
        }
    }
}

pub(super) fn can_skip_existing_media_summary(
    summary: &mova_db::ExistingMediaMetadataSummary,
    scan_hash: &str,
    metadata_provider_enabled: bool,
    metadata_language: &str,
    file_path: &std::path::Path,
) -> bool {
    if summary.scan_hash.as_deref() != Some(scan_hash) {
        return false;
    }

    if summary.local_analysis_version != LOCAL_ANALYSIS_VERSION {
        return false;
    }

    !should_rescan_unchanged_existing_media_summary(
        summary,
        metadata_provider_enabled,
        metadata_language,
        file_path,
    )
}

pub(super) fn should_rescan_unchanged_existing_media_summary(
    summary: &mova_db::ExistingMediaMetadataSummary,
    metadata_provider_enabled: bool,
    _metadata_language: &str,
    file_path: &std::path::Path,
) -> bool {
    if is_existing_summary_in_other_review_section(summary) {
        return true;
    }

    if should_retry_review_metadata_status(summary) {
        return true;
    }

    if metadata_provider_enabled && should_retry_incomplete_remote_match(summary) {
        return true;
    }

    if metadata_provider_enabled && should_retry_local_series_title_override(summary, file_path) {
        return true;
    }

    if metadata_provider_enabled && should_retry_external_cached_artwork(summary) {
        return true;
    }

    false
}

pub(super) fn can_reuse_cached_local_analysis(
    summary: &mova_db::ExistingMediaMetadataSummary,
    scan_hash: &str,
) -> bool {
    summary.scan_hash.as_deref() == Some(scan_hash)
        && summary.local_analysis_version == LOCAL_ANALYSIS_VERSION
        && !summary.has_local_nfo
        && !summary.series_has_local_nfo
}

pub(super) fn should_retry_incomplete_remote_match(
    summary: &mova_db::ExistingMediaMetadataSummary,
) -> bool {
    if summary.metadata_status != METADATA_STATUS_MATCHED {
        return true;
    }

    effective_existing_metadata_provider_item_id(summary).is_none()
        || !effective_existing_metadata_provider(summary)
            .is_some_and(|value| value.eq_ignore_ascii_case(TMDB_PROVIDER_NAME))
}

pub(super) fn should_retry_review_metadata_status(
    summary: &mova_db::ExistingMediaMetadataSummary,
) -> bool {
    summary.metadata_failure_reason.as_deref() == Some(METADATA_FAILURE_PROVIDER_ERROR)
        || matches!(
            summary.metadata_status.as_str(),
            METADATA_STATUS_PENDING | METADATA_STATUS_UNMATCHED | METADATA_STATUS_FAILED
        )
}

pub(super) fn is_existing_summary_in_other_review_section(
    summary: &mova_db::ExistingMediaMetadataSummary,
) -> bool {
    matches!(
        summary.metadata_status.as_str(),
        METADATA_STATUS_SKIPPED | METADATA_STATUS_UNMATCHED | METADATA_STATUS_FAILED
    ) && !has_existing_remote_enrichment(summary)
}

pub(super) fn has_existing_remote_enrichment(
    summary: &mova_db::ExistingMediaMetadataSummary,
) -> bool {
    effective_existing_metadata_provider_item_id(summary).is_some()
        || existing_text_values(summary).any(has_text)
        || existing_artwork_paths(summary).any(has_text)
}

pub(super) fn existing_text_values(
    summary: &mova_db::ExistingMediaMetadataSummary,
) -> impl Iterator<Item = &str> {
    [
        summary.original_title.as_deref(),
        summary.overview.as_deref(),
        summary.series_original_title.as_deref(),
        summary.series_overview.as_deref(),
    ]
    .into_iter()
    .flatten()
}

pub(super) fn has_text(value: &str) -> bool {
    !value.trim().is_empty()
}

pub(super) fn effective_existing_metadata_provider(
    summary: &mova_db::ExistingMediaMetadataSummary,
) -> Option<&str> {
    if summary.media_type.eq_ignore_ascii_case("episode") {
        return summary
            .series_metadata_provider
            .as_deref()
            .or(summary.metadata_provider.as_deref());
    }

    summary.metadata_provider.as_deref()
}

pub(super) fn effective_existing_metadata_provider_item_id(
    summary: &mova_db::ExistingMediaMetadataSummary,
) -> Option<String> {
    if summary.media_type.eq_ignore_ascii_case("episode") {
        return summary
            .series_metadata_provider_item_id
            .clone()
            .or_else(|| summary.metadata_provider_item_id.clone());
    }

    summary.metadata_provider_item_id.clone()
}

pub(super) fn should_retry_external_cached_artwork(
    summary: &mova_db::ExistingMediaMetadataSummary,
) -> bool {
    if summary.metadata_status != METADATA_STATUS_MATCHED {
        return false;
    }

    existing_artwork_paths(summary).any(is_external_artwork_path)
}

pub(super) fn existing_artwork_paths(
    summary: &mova_db::ExistingMediaMetadataSummary,
) -> impl Iterator<Item = &str> {
    [
        summary.poster_path.as_deref(),
        summary.backdrop_path.as_deref(),
        summary.series_poster_path.as_deref(),
        summary.series_backdrop_path.as_deref(),
        summary.series_logo_path.as_deref(),
        summary.season_poster_path.as_deref(),
        summary.season_backdrop_path.as_deref(),
        summary.logo_path.as_deref(),
    ]
    .into_iter()
    .flatten()
}

pub(super) fn is_external_artwork_path(path: &str) -> bool {
    let path = path.trim();
    path.starts_with("http://") || path.starts_with("https://")
}

pub(super) fn should_retry_local_series_title_override(
    summary: &mova_db::ExistingMediaMetadataSummary,
    file_path: &std::path::Path,
) -> bool {
    if summary.metadata_status != METADATA_STATUS_MATCHED {
        return false;
    }

    if !summary.media_type.eq_ignore_ascii_case("episode")
        && !summary
            .remote_media_type
            .as_deref()
            .is_some_and(|value| value.eq_ignore_ascii_case(REMOTE_MEDIA_TYPE_SERIES))
    {
        return false;
    }

    if effective_existing_metadata_provider_item_id(summary).is_none() {
        return false;
    }

    let Some(file_metadata) = infer_series_file_metadata(file_path) else {
        return false;
    };
    let current_title = summary
        .series_title
        .as_deref()
        .unwrap_or(summary.title.as_str())
        .trim();
    let local_display_title = file_metadata.display_title.trim();
    let local_lookup_title = file_metadata.title.trim();

    !local_display_title.eq_ignore_ascii_case(local_lookup_title)
        && current_title.eq_ignore_ascii_case(local_display_title)
}

#[cfg(test)]
pub(super) async fn inspect_incremental_scan_files(
    changed_files: Vec<IncrementalScanFile>,
    cancellation_flag: Arc<AtomicBool>,
) -> ApplicationResult<InspectIncrementalScanFilesOutcome> {
    inspect_incremental_scan_files_with_root(changed_files, None, None, None, cancellation_flag)
        .await
}

pub(super) async fn inspect_incremental_scan_files_within_root(
    changed_files: Vec<IncrementalScanFile>,
    root_path: PathBuf,
    eligible_generic_movie_nfo_sources: HashSet<PathBuf>,
    eligible_series_nfo_sources: HashSet<PathBuf>,
    cancellation_flag: Arc<AtomicBool>,
) -> ApplicationResult<InspectIncrementalScanFilesOutcome> {
    inspect_incremental_scan_files_with_root(
        changed_files,
        Some(root_path),
        Some(eligible_generic_movie_nfo_sources),
        Some(eligible_series_nfo_sources),
        cancellation_flag,
    )
    .await
}

async fn inspect_incremental_scan_files_with_root(
    changed_files: Vec<IncrementalScanFile>,
    root_path: Option<PathBuf>,
    eligible_generic_movie_nfo_sources: Option<HashSet<PathBuf>>,
    eligible_series_nfo_sources: Option<HashSet<PathBuf>>,
    cancellation_flag: Arc<AtomicBool>,
) -> ApplicationResult<InspectIncrementalScanFilesOutcome> {
    tokio::task::spawn_blocking(move || {
        let mut discovered_files = Vec::with_capacity(changed_files.len());
        let mut series_sidecars = HashMap::<String, mova_scan::LocalNfoObservation>::new();
        let subtitle_index = mova_scan::SubtitleDirectoryIndex::build(
            changed_files
                .iter()
                .map(|file| file.inventory.file_path.as_path()),
        );

        for changed_file in changed_files {
            if is_cancelled(&cancellation_flag) {
                return Ok(InspectIncrementalScanFilesOutcome::Cancelled);
            }

            let file_path = changed_file.inventory.file_path.display().to_string();
            if let Some(existing_metadata) = changed_file.existing_metadata.as_ref() {
                let scan_hash = discovered_media_file_inventory_scan_hash(&changed_file.inventory);
                if can_reuse_cached_local_analysis(existing_metadata, scan_hash.as_str()) {
                    let mut discovered_file = discovered_file_from_existing_local_analysis(
                        &changed_file.inventory,
                        existing_metadata,
                    )?;
                    populate_series_sidecar_metadata_with_optional_root(
                        &mut discovered_file,
                        root_path.as_deref(),
                        &mut series_sidecars,
                        eligible_series_nfo_sources.as_ref(),
                    );
                    discovered_files.push(discovered_file);
                    continue;
                }
            }

            let inspection = match root_path.as_deref() {
                Some(root_path) => {
                    let generic_movie_nfo = normalize_nfo_source_path(
                        &changed_file
                            .inventory
                            .file_path
                            .parent()
                            .unwrap_or(root_path)
                            .join("movie.nfo"),
                    );
                    let allow_generic_movie_nfo = eligible_generic_movie_nfo_sources
                        .as_ref()
                        .is_none_or(|sources| sources.contains(&generic_movie_nfo));
                    mova_scan::inspect_media_file_inventory_within_root_with_cancel_and_subtitle_index_and_nfo_policy(
                        changed_file.inventory,
                        root_path,
                        &subtitle_index,
                        allow_generic_movie_nfo,
                        || is_cancelled(&cancellation_flag),
                    )
                }
                None => mova_scan::inspect_media_file_inventory_with_cancel_and_subtitle_index(
                    changed_file.inventory,
                    &subtitle_index,
                    || is_cancelled(&cancellation_flag),
                ),
            };
            let mut discovered_file = match inspection {
                Ok(file) => file,
                Err(error)
                    if error.kind() == std::io::ErrorKind::Interrupted
                        && is_cancelled(&cancellation_flag) =>
                {
                    return Ok(InspectIncrementalScanFilesOutcome::Cancelled);
                }
                Err(error) => {
                    return Err(ApplicationError::Unexpected(anyhow::anyhow!(
                        "Unable to inspect changed media file {}: {}",
                        file_path,
                        error
                    )));
                }
            };

            populate_series_sidecar_metadata_with_optional_root(
                &mut discovered_file,
                root_path.as_deref(),
                &mut series_sidecars,
                eligible_series_nfo_sources.as_ref(),
            );
            if let Some(existing_metadata) = changed_file.existing_metadata.as_ref() {
                apply_existing_media_metadata(&mut discovered_file, existing_metadata);
            }
            discovered_files.push(discovered_file);
        }

        Ok(InspectIncrementalScanFilesOutcome::Completed(
            discovered_files,
        ))
    })
    .await
    .map_err(|error| {
        ApplicationError::Unexpected(anyhow::anyhow!(
            "The changed media inspection worker exited unexpectedly: {}",
            error
        ))
    })?
}

#[cfg(test)]
pub(super) fn populate_series_sidecar_metadata(
    file: &mut DiscoveredMediaFile,
    cache: &mut HashMap<String, mova_scan::LocalNfoObservation>,
) {
    populate_series_sidecar_metadata_with_optional_root(file, None, cache, None);
}

fn populate_series_sidecar_metadata_with_optional_root(
    file: &mut DiscoveredMediaFile,
    root_path: Option<&std::path::Path>,
    cache: &mut HashMap<String, mova_scan::LocalNfoObservation>,
    eligible_sources: Option<&HashSet<PathBuf>>,
) {
    if file.season_number.is_none() || file.episode_number.is_none() {
        return;
    }

    let cache_key = file
        .file_path
        .parent()
        .unwrap_or(file.file_path.as_path())
        .to_string_lossy()
        .to_string();
    let observation = cache.entry(cache_key).or_insert_with(|| match root_path {
        Some(root_path) => mova_scan::observe_series_nfo_within_root(&file.file_path, root_path),
        None => mova_scan::observe_series_nfo(&file.file_path),
    });

    if eligible_sources.is_some_and(|sources| {
        nfo_observation_source_path(observation)
            .is_some_and(|path| !sources.contains(&normalize_nfo_source_path(path)))
    }) {
        file.series_sidecar_title = None;
        file.series_sidecar_year = None;
        file.series_local_nfo = None;
        file.invalid_series_local_nfo_source_path = None;
        return;
    }

    match observation {
        mova_scan::LocalNfoObservation::Valid(metadata) => {
            file.series_sidecar_title = metadata.title.clone();
            file.series_sidecar_year = metadata.year;
            file.series_local_nfo = Some((**metadata).clone());
            file.invalid_series_local_nfo_source_path = None;
        }
        mova_scan::LocalNfoObservation::Invalid { candidate_path, .. } => {
            file.series_sidecar_title = None;
            file.series_sidecar_year = None;
            file.series_local_nfo = None;
            file.invalid_series_local_nfo_source_path = Some(candidate_path.clone());
        }
        mova_scan::LocalNfoObservation::Absent { .. } => {
            file.series_sidecar_title = None;
            file.series_sidecar_year = None;
            file.series_local_nfo = None;
            file.invalid_series_local_nfo_source_path = None;
        }
    }
}

pub(super) async fn inspect_incremental_scan_files_shallow(
    changed_files: Vec<IncrementalScanFile>,
    root_path: PathBuf,
) -> ApplicationResult<Vec<PendingScanFile>> {
    tokio::task::spawn_blocking(move || {
        let mut pending_files = Vec::with_capacity(changed_files.len());

        for changed_file in changed_files {
            let file_path = changed_file.inventory.file_path.display().to_string();
            let file =
                mova_scan::inspect_media_file_inventory_shallow(changed_file.inventory.clone())
                    .map_err(|error| {
                        ApplicationError::Unexpected(anyhow::anyhow!(
                            "Unable to inspect changed media file {}: {}",
                            file_path,
                            error
                        ))
                    })?;
            pending_files.push(PendingScanFile { changed_file, file });
        }

        let shallow_files = pending_files
            .iter()
            .map(|pending| pending.file.clone())
            .collect::<Vec<_>>();
        let observations = eligible_local_nfo_observations(&shallow_files, &root_path);
        for (pending_file, (media_observation, series_observation)) in
            pending_files.iter_mut().zip(observations)
        {
            apply_media_nfo_observation(&mut pending_file.file, media_observation);
            apply_series_nfo_observation(&mut pending_file.file, series_observation);
        }
        force_reconcile_owners_with_ineligible_selected_nfo(&mut pending_files);

        Ok(pending_files)
    })
    .await
    .map_err(|error| {
        ApplicationError::Unexpected(anyhow::anyhow!(
            "The shallow media inspection worker exited unexpectedly: {}",
            error
        ))
    })?
}

/// Eligibility for a directory-level sidecar depends on the complete library
/// inventory, not only on the carrier that originally selected it. When a new
/// work makes a formerly valid `movie.nfo` or shared `tvshow.nfo` ambiguous,
/// that original carrier's scan hash can remain unchanged. Force every live
/// carrier of the persisted owner through the normal remove-and-reproject path
/// before the final library-wide reconciliation runs.
pub(super) fn force_reconcile_owners_with_ineligible_selected_nfo(files: &mut [PendingScanFile]) {
    let mut eligible_media_sources = HashSet::<PathBuf>::new();
    let mut eligible_series_sources = HashSet::<PathBuf>::new();

    for pending in files.iter() {
        if let Some(source_path) = pending
            .file
            .local_nfo
            .as_ref()
            .map(|metadata| metadata.source_path.as_path())
            .or(pending.file.invalid_local_nfo_source_path.as_deref())
        {
            eligible_media_sources.insert(normalize_nfo_source_path(source_path));
        }
        if let Some(source_path) = pending
            .file
            .series_local_nfo
            .as_ref()
            .map(|metadata| metadata.source_path.as_path())
            .or(pending.file.invalid_series_local_nfo_source_path.as_deref())
        {
            eligible_series_sources.insert(normalize_nfo_source_path(source_path));
        }
    }

    let owners_with_ineligible_source = files
        .iter()
        .filter_map(|pending| pending.changed_file.existing_metadata.as_ref())
        .filter(|summary| {
            summary
                .local_nfo_source_path
                .as_deref()
                .is_some_and(|path| {
                    !eligible_media_sources.contains(&normalize_nfo_source_path(Path::new(path)))
                })
                || summary
                    .series_local_nfo_source_path
                    .as_deref()
                    .is_some_and(|path| {
                        !eligible_series_sources
                            .contains(&normalize_nfo_source_path(Path::new(path)))
                    })
        })
        .map(|summary| summary.logical_metadata_owner_id)
        .collect::<HashSet<_>>();

    if owners_with_ineligible_source.is_empty() {
        return;
    }

    for pending in files {
        if pending
            .changed_file
            .existing_metadata
            .as_ref()
            .is_some_and(|summary| {
                owners_with_ineligible_source.contains(&summary.logical_metadata_owner_id)
            })
        {
            pending.changed_file.requires_processing = true;
        }
    }
}

/// Observes NFO files only after the complete shallow inventory can prove the
/// ownership boundary of directory-level metadata.
///
/// A file-specific NFO is always scoped to its carrier. A `movie.nfo` is
/// eligible only when its whole directory has one parsed movie title/year
/// identity. A `tvshow.nfo` is eligible only when all episode carriers below
/// its directory that can reach it belong to one pre-NFO series group. This
/// keeps a common library root from becoming shared metadata for unrelated
/// works while still allowing multi-version movies and multi-season series.
pub(super) fn eligible_local_nfo_observations(
    files: &[DiscoveredMediaFile],
    root_path: &Path,
) -> Vec<(
    Option<mova_scan::LocalNfoObservation>,
    Option<mova_scan::LocalNfoObservation>,
)> {
    let mut media_observations = Vec::with_capacity(files.len());
    let mut series_observations = Vec::with_capacity(files.len());
    let mut movie_identities_by_directory = HashMap::<PathBuf, HashSet<String>>::new();
    let mut series_identities_by_directory = HashMap::<PathBuf, HashSet<String>>::new();

    for file in files {
        let is_episode = file.season_number.is_some() && file.episode_number.is_some();
        if is_episode {
            let series_identity = build_scan_presentation_group_with_root(file, root_path).item_key;
            for directory in nfo_ancestor_directories_within_root(&file.file_path, root_path) {
                series_identities_by_directory
                    .entry(normalize_nfo_source_path(&directory))
                    .or_default()
                    .insert(series_identity.clone());
            }
        } else if let Some(parent) = file.file_path.parent() {
            movie_identities_by_directory
                .entry(normalize_nfo_source_path(parent))
                .or_default()
                .insert(shallow_movie_identity(file));
        }

        let media_kind = if is_episode {
            mova_scan::MediaNfoKind::Episode
        } else {
            mova_scan::MediaNfoKind::Movie
        };
        let media_observation = mova_scan::observe_media_nfo_for_kind_within_root(
            &file.file_path,
            media_kind,
            root_path,
        );
        media_observations.push(media_observation);

        let series_observation = is_episode
            .then(|| mova_scan::observe_series_nfo_within_root(&file.file_path, root_path));
        series_observations.push(series_observation);
    }

    media_observations
        .into_iter()
        .zip(series_observations)
        .map(|(media, series)| {
            let media = if nfo_observation_source_path(&media)
                .filter(|path| is_generic_movie_nfo(path))
                .is_some_and(|path| {
                    path.parent()
                        .map(normalize_nfo_source_path)
                        .and_then(|directory| movie_identities_by_directory.get(&directory))
                        .is_none_or(|identities| identities.len() != 1)
                }) {
                None
            } else {
                Some(media)
            };
            let series = series.filter(|observation| {
                nfo_observation_source_path(observation).is_none_or(|path| {
                    path.parent()
                        .map(normalize_nfo_source_path)
                        .and_then(|directory| series_identities_by_directory.get(&directory))
                        .is_some_and(|identities| identities.len() == 1)
                })
            });
            (media, series)
        })
        .collect()
}

fn nfo_ancestor_directories_within_root(file_path: &Path, root_path: &Path) -> Vec<PathBuf> {
    let Some(relative_parent) = file_path
        .strip_prefix(root_path)
        .ok()
        .and_then(Path::parent)
    else {
        return Vec::new();
    };

    relative_parent
        .ancestors()
        .take(5)
        .map(|directory| root_path.join(directory))
        .collect()
}

pub(super) fn nfo_observation_source_path(
    observation: &mova_scan::LocalNfoObservation,
) -> Option<&Path> {
    match observation {
        mova_scan::LocalNfoObservation::Valid(metadata) => Some(metadata.source_path.as_path()),
        mova_scan::LocalNfoObservation::Invalid { candidate_path, .. } => {
            Some(candidate_path.as_path())
        }
        mova_scan::LocalNfoObservation::Absent { .. } => None,
    }
}

pub(super) fn normalize_nfo_source_path(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

pub(super) fn is_generic_movie_nfo(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case("movie.nfo"))
}

fn shallow_movie_identity(file: &DiscoveredMediaFile) -> String {
    let normalized_title = file
        .source_title
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    if normalized_title.is_empty() {
        return format!("path:{}", file.file_path.to_string_lossy());
    }

    format!("title:{normalized_title}:year:{:?}", file.year)
}

fn apply_media_nfo_observation(
    file: &mut DiscoveredMediaFile,
    observation: Option<mova_scan::LocalNfoObservation>,
) {
    match observation {
        Some(mova_scan::LocalNfoObservation::Valid(metadata)) => file.local_nfo = Some(*metadata),
        Some(mova_scan::LocalNfoObservation::Invalid { candidate_path, .. }) => {
            file.invalid_local_nfo_source_path = Some(candidate_path);
        }
        Some(mova_scan::LocalNfoObservation::Absent { .. }) | None => {}
    }
}

fn apply_series_nfo_observation(
    file: &mut DiscoveredMediaFile,
    observation: Option<mova_scan::LocalNfoObservation>,
) {
    match observation {
        Some(mova_scan::LocalNfoObservation::Valid(metadata)) => {
            file.series_sidecar_title = metadata.title.clone();
            file.series_sidecar_year = metadata.year;
            file.series_local_nfo = Some(*metadata);
        }
        Some(mova_scan::LocalNfoObservation::Invalid { candidate_path, .. }) => {
            file.invalid_series_local_nfo_source_path = Some(candidate_path);
        }
        Some(mova_scan::LocalNfoObservation::Absent { .. }) | None => {}
    }
}

pub(super) fn discovered_file_from_existing_local_analysis(
    inventory: &DiscoveredMediaFileInventory,
    summary: &mova_db::ExistingMediaMetadataSummary,
) -> ApplicationResult<DiscoveredMediaFile> {
    let file_size = u64::try_from(summary.file_size).map_err(|_| {
        ApplicationError::Unexpected(anyhow::anyhow!(
            "stored media file size is invalid: {}",
            summary.file_path
        ))
    })?;
    let (
        title,
        source_title,
        original_title,
        sort_title,
        year,
        country,
        genres,
        studio,
        overview,
        poster_path,
        backdrop_path,
    ) = if summary.media_type.eq_ignore_ascii_case("episode") {
        (
            summary
                .series_title
                .as_ref()
                .cloned()
                .unwrap_or_else(|| summary.title.clone()),
            summary
                .series_source_title
                .as_ref()
                .cloned()
                .unwrap_or_else(|| summary.source_title.clone()),
            summary.series_original_title.clone(),
            summary.series_sort_title.clone(),
            summary.series_year,
            summary.series_country.clone(),
            summary.series_genres.clone(),
            summary.series_studio.clone(),
            summary
                .series_overview
                .clone()
                .or_else(|| summary.overview.clone()),
            summary.poster_path.clone(),
            summary.backdrop_path.clone(),
        )
    } else {
        (
            summary.title.clone(),
            summary.source_title.clone(),
            summary.original_title.clone(),
            summary.sort_title.clone(),
            summary.year,
            summary.country.clone(),
            summary.genres.clone(),
            summary.studio.clone(),
            summary.overview.clone(),
            summary.poster_path.clone(),
            summary.backdrop_path.clone(),
        )
    };
    let accepted_metadata_provider = (summary.metadata_status == METADATA_STATUS_MATCHED)
        .then(|| effective_existing_metadata_provider(summary))
        .flatten()
        .map(str::to_string);
    let accepted_metadata_provider_item_id = (summary.metadata_status == METADATA_STATUS_MATCHED)
        .then(|| effective_existing_metadata_provider_item_id(summary))
        .flatten();

    Ok(DiscoveredMediaFile {
        file_path: inventory.file_path.clone(),
        file_modified_at_ms: inventory.file_modified_at_ms,
        sidecar_fingerprint: inventory.sidecar_fingerprint.clone(),
        probe_error: None,
        metadata_provider: accepted_metadata_provider,
        metadata_provider_item_id: accepted_metadata_provider_item_id,
        title,
        source_title,
        original_title,
        sort_title,
        tagline: None,
        premiere_date: None,
        content_rating: None,
        series_sidecar_title: None,
        series_sidecar_year: None,
        local_nfo: None,
        series_local_nfo: None,
        invalid_local_nfo_source_path: None,
        invalid_series_local_nfo_source_path: None,
        local_nfo_is_selected: false,
        series_local_nfo_is_selected: false,
        removed_local_nfo_source_path: None,
        removed_series_local_nfo_source_path: None,
        year,
        external_ids: Vec::new(),
        ratings: Vec::new(),
        metadata_status: Some(summary.metadata_status.clone()),
        metadata_failure_reason: summary.metadata_failure_reason.clone(),
        remote_media_type: summary.remote_media_type.clone(),
        country,
        genres,
        studio,
        season_number: summary.season_number,
        season_title: summary.season_title.clone(),
        season_overview: summary.season_overview.clone(),
        season_poster_path: summary.season_poster_path.clone(),
        season_backdrop_path: summary.season_backdrop_path.clone(),
        episode_number: summary.episode_number,
        episode_title: summary.episode_title.clone(),
        episode_original_title: None,
        episode_sort_title: None,
        episode_year: None,
        episode_overview: None,
        episode_tagline: None,
        episode_premiere_date: None,
        episode_content_rating: None,
        overview,
        series_poster_path: summary.series_poster_path.clone(),
        series_backdrop_path: summary.series_backdrop_path.clone(),
        series_logo_path: summary.series_logo_path.clone(),
        poster_path,
        backdrop_path,
        logo_path: summary.logo_path.clone(),
        file_size: inventory.file_size.max(file_size),
        container: summary.container.clone(),
        duration_seconds: summary.duration_seconds,
        video_title: summary.video_title.clone(),
        video_codec: summary.video_codec.clone(),
        video_profile: summary.video_profile.clone(),
        video_level: summary.video_level.clone(),
        audio_codec: summary.audio_codec.clone(),
        width: summary.width,
        height: summary.height,
        bitrate: summary.bitrate,
        video_bitrate: summary.video_bitrate,
        video_frame_rate: summary.video_frame_rate,
        video_aspect_ratio: summary.video_aspect_ratio.clone(),
        video_scan_type: summary.video_scan_type.clone(),
        video_color_primaries: summary.video_color_primaries.clone(),
        video_color_space: summary.video_color_space.clone(),
        video_color_transfer: summary.video_color_transfer.clone(),
        video_bit_depth: summary.video_bit_depth,
        video_pixel_format: summary.video_pixel_format.clone(),
        video_reference_frames: summary.video_reference_frames,
        technical_tags: summary.technical_tags.clone(),
        audio_tracks: summary
            .audio_tracks
            .iter()
            .map(|track| DiscoveredAudioTrack {
                stream_index: track.stream_index,
                language: track.language.clone(),
                audio_codec: track.audio_codec.clone(),
                label: track.label.clone(),
                channel_layout: track.channel_layout.clone(),
                channels: track.channels,
                bitrate: track.bitrate,
                sample_rate: track.sample_rate,
                is_default: track.is_default,
            })
            .collect(),
        subtitle_tracks: summary
            .subtitle_tracks
            .iter()
            .map(|subtitle| DiscoveredSubtitleTrack {
                source_kind: subtitle.source_kind.clone(),
                file_path: subtitle.file_path.as_ref().map(PathBuf::from),
                stream_index: subtitle.stream_index,
                language: subtitle.language.clone(),
                subtitle_format: subtitle.subtitle_format.clone(),
                label: subtitle.label.clone(),
                is_default: subtitle.is_default,
                is_forced: subtitle.is_forced,
                is_hearing_impaired: subtitle.is_hearing_impaired,
            })
            .collect(),
    })
}

pub(super) async fn discover_media_files(
    pool: &PgPool,
    scan_job_id: i64,
    library: &Library,
    fence: &BackgroundJobFence,
    cancellation_flag: Arc<AtomicBool>,
    event_listener: Arc<dyn Fn(ScanJobEvent) + Send + Sync>,
) -> ApplicationResult<DiscoverMediaFilesOutcome> {
    let root_path = library.root_path.as_str();
    let root_path_string = root_path.to_string();
    let root_path_for_task = root_path_string.clone();
    let (progress_tx, mut progress_rx) = tokio::sync::mpsc::channel::<()>(1);
    let progress_pool = pool.clone();
    let last_progress = Arc::new(AtomicI32::new(0));
    let last_progress_for_task = last_progress.clone();
    let latest_discovered = Arc::new(AtomicI32::new(0));
    let latest_discovered_for_task = latest_discovered.clone();
    let progress_event_listener = event_listener.clone();
    let progress_fence = fence.clone();
    let progress_cancellation_flag = cancellation_flag.clone();

    let progress_task = tokio::spawn(async move {
        let mut persisted_progress = 0;
        let mut pending_progress = 0;
        let mut last_flush_at: Option<Instant> = None;

        while progress_rx.recv().await.is_some() {
            let scanned_files = latest_discovered_for_task.load(Ordering::SeqCst);
            if scanned_files <= pending_progress {
                continue;
            }

            pending_progress = scanned_files;
            let now = Instant::now();

            if !should_flush_discovery_progress(
                persisted_progress,
                pending_progress,
                last_flush_at,
                now,
            ) {
                continue;
            }

            match flush_discovery_progress(
                &progress_pool,
                scan_job_id,
                pending_progress,
                &progress_fence,
                &progress_event_listener,
            )
            .await
            {
                Ok(Some(flushed_progress)) => {
                    persisted_progress = flushed_progress;
                    last_flush_at = Some(now);
                    last_progress_for_task.store(flushed_progress, Ordering::SeqCst);
                }
                Ok(None) => {}
                Err(error) => {
                    progress_cancellation_flag.store(true, Ordering::SeqCst);
                    return Err(error);
                }
            }
        }

        if pending_progress > persisted_progress {
            match flush_discovery_progress(
                &progress_pool,
                scan_job_id,
                pending_progress,
                &progress_fence,
                &progress_event_listener,
            )
            .await
            {
                Ok(Some(flushed_progress)) => {
                    last_progress_for_task.store(flushed_progress, Ordering::SeqCst);
                }
                Ok(None) => {}
                Err(error) => {
                    progress_cancellation_flag.store(true, Ordering::SeqCst);
                    return Err(error);
                }
            }
        }

        Ok::<(), ApplicationError>(())
    });

    let cancellation_for_task = cancellation_flag.clone();
    let result = tokio::task::spawn_blocking(move || {
        mova_scan::discover_media_file_inventory_with_progress_and_cancel(
            std::path::Path::new(&root_path_for_task),
            |count| {
                publish_discovery_progress(&latest_discovered, &progress_tx, count);
            },
            || cancellation_for_task.load(Ordering::SeqCst),
        )
    })
    .await
    .map_err(|error| {
        ApplicationError::Unexpected(anyhow::anyhow!(
            "The file discovery worker exited unexpectedly ({}): {}",
            root_path_string,
            error
        ))
    })?;

    progress_task.await.map_err(|error| {
        ApplicationError::Unexpected(anyhow::anyhow!(
            "The scan progress worker exited unexpectedly: {}",
            error
        ))
    })??;

    match result {
        Ok(files) => Ok(DiscoverMediaFilesOutcome::Completed(files)),
        Err(error) if error.kind() == std::io::ErrorKind::Interrupted => Ok(
            DiscoverMediaFilesOutcome::Cancelled(last_progress.load(Ordering::SeqCst)),
        ),
        Err(error) => Err(ApplicationError::Unexpected(anyhow::anyhow!(
            "Unable to read library directory {}: {}",
            root_path,
            error
        ))),
    }
}

pub(super) fn publish_discovery_progress(
    latest_discovered: &AtomicI32,
    progress_signal: &tokio::sync::mpsc::Sender<()>,
    count: usize,
) {
    latest_discovered.store(i32::try_from(count).unwrap_or(i32::MAX), Ordering::SeqCst);
    let _ = progress_signal.try_send(());
}
