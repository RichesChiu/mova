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

    let existing_metadata =
        mova_db::list_existing_media_metadata_for_file_paths(pool, library_id, &file_paths)
            .await
            .map_err(ApplicationError::Unexpected)?;
    let container_bindings = build_container_binding_index(existing_metadata.as_slice(), root_path);

    let existing_by_path = existing_metadata
        .into_iter()
        .map(|summary| (summary.file_path.clone(), summary))
        .collect::<HashMap<_, _>>();

    let mut changed_files = Vec::new();

    for inventory in discovered_files {
        let file_path = inventory.file_path.to_string_lossy().to_string();
        let scan_hash = discovered_media_file_inventory_scan_hash(&inventory);

        match existing_by_path.get(file_path.as_str()) {
            Some(summary)
                if can_skip_existing_media_summary(
                    summary,
                    scan_hash.as_str(),
                    metadata_provider_enabled,
                    metadata_language,
                    &inventory.file_path,
                ) =>
            {
                continue;
            }
            existing_metadata => changed_files.push(IncrementalScanFile {
                inventory,
                existing_metadata: existing_metadata.cloned(),
            }),
        }
    }

    hydrate_incremental_scan_file_cached_tracks(pool, &mut changed_files).await;

    Ok(IncrementalScanPlan {
        discovered_paths: file_paths,
        changed_files,
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
    inspect_incremental_scan_files_with_root(changed_files, None, cancellation_flag).await
}

pub(super) async fn inspect_incremental_scan_files_within_root(
    changed_files: Vec<IncrementalScanFile>,
    root_path: PathBuf,
    cancellation_flag: Arc<AtomicBool>,
) -> ApplicationResult<InspectIncrementalScanFilesOutcome> {
    inspect_incremental_scan_files_with_root(changed_files, Some(root_path), cancellation_flag)
        .await
}

async fn inspect_incremental_scan_files_with_root(
    changed_files: Vec<IncrementalScanFile>,
    root_path: Option<PathBuf>,
    cancellation_flag: Arc<AtomicBool>,
) -> ApplicationResult<InspectIncrementalScanFilesOutcome> {
    tokio::task::spawn_blocking(move || {
        let mut discovered_files = Vec::with_capacity(changed_files.len());
        let mut series_sidecars =
            HashMap::<String, Option<mova_scan::SeriesSidecarMetadata>>::new();
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
                    );
                    discovered_files.push(discovered_file);
                    continue;
                }
            }

            let mut discovered_file =
                match mova_scan::inspect_media_file_inventory_with_cancel_and_subtitle_index(
                    changed_file.inventory,
                    &subtitle_index,
                    || is_cancelled(&cancellation_flag),
                ) {
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

            if let Some(existing_metadata) = changed_file.existing_metadata.as_ref() {
                apply_existing_media_metadata(&mut discovered_file, existing_metadata);
            }

            populate_series_sidecar_metadata_with_optional_root(
                &mut discovered_file,
                root_path.as_deref(),
                &mut series_sidecars,
            );
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
    cache: &mut HashMap<String, Option<mova_scan::SeriesSidecarMetadata>>,
) {
    populate_series_sidecar_metadata_with_optional_root(file, None, cache);
}

fn populate_series_sidecar_metadata_with_optional_root(
    file: &mut DiscoveredMediaFile,
    root_path: Option<&std::path::Path>,
    cache: &mut HashMap<String, Option<mova_scan::SeriesSidecarMetadata>>,
) {
    if file.season_number.is_none() || file.episode_number.is_none() {
        return;
    }

    let cache_key = root_path
        .and_then(|root_path| metadata_container_key_for_path(&file.file_path, root_path, "series"))
        .or_else(|| series_container_item_key(&file.file_path))
        .unwrap_or_else(|| {
            file.file_path
                .parent()
                .unwrap_or(file.file_path.as_path())
                .to_string_lossy()
                .to_string()
        });
    let metadata = cache.entry(cache_key).or_insert_with(|| {
        root_path
            .and_then(|root_path| {
                infer_series_sidecar_metadata_within_root(&file.file_path, root_path)
            })
            .or_else(|| {
                root_path
                    .is_none()
                    .then(|| mova_scan::infer_series_sidecar_metadata(&file.file_path))
                    .flatten()
            })
    });

    file.series_sidecar_title = metadata
        .as_ref()
        .and_then(|metadata| metadata.title.clone());
    file.series_sidecar_year = metadata.as_ref().and_then(|metadata| metadata.year);
}

pub(super) async fn inspect_incremental_scan_files_shallow(
    changed_files: Vec<IncrementalScanFile>,
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
        series_sidecar_title: None,
        series_sidecar_year: None,
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
