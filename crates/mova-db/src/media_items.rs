mod query;
mod ratings;
mod series;
mod sync;

use anyhow::Result;
use mova_domain::{MediaExternalId, MediaItem, MediaRating, MediaSourceKind, ScanJob};
pub use query::{
    count_media_items_for_library, delete_series_episode_outline_cache, get_audio_track,
    get_library_media_type_counts, get_media_file, get_media_file_with_library_visibility,
    get_media_item, get_media_item_playback_header, get_media_item_with_library_visibility,
    get_season, get_season_with_library_visibility, get_series_episode_outline_cache,
    get_subtitle_file, global_search, list_audio_tracks_for_media_file,
    list_audio_tracks_for_media_files, list_episodes_for_season, list_episodes_for_series,
    list_existing_media_metadata_for_file_paths, list_library_media_file_memberships,
    list_library_media_file_paths, list_media_files_for_media_item,
    list_media_item_metadata_refresh_source_files, list_media_item_previews_by_library,
    list_media_items_for_library, list_recently_added_media_items_by_library,
    list_seasons_for_series, list_series_media_item_ids_for_library,
    list_subtitle_files_for_media_file, list_subtitle_files_for_media_files,
    replace_audio_tracks_for_media_file, replace_subtitle_files_for_media_file,
    update_media_file_metadata, update_media_item_metadata, update_series_episode_metadata,
    update_series_season_metadata, upsert_series_episode_outline_cache,
};
pub use ratings::list_media_item_ratings;
pub(crate) use ratings::replace_media_item_remote_data;
use serde_json::Value;
use sqlx::{Postgres, Row, Transaction};
pub use sync::{
    cleanup_library_orphan_series_after_scan, delete_library_media_by_file_path,
    delete_library_media_by_path_prefix, patch_library_media_entries_remote_by_file_path,
    patch_library_media_entries_remote_by_file_path_with_progress, sync_library_media,
    sync_library_media_best_effort, sync_library_media_changes,
    upsert_library_media_entries_by_file_path,
    upsert_library_media_entries_by_file_path_with_progress,
    upsert_library_media_entry_by_file_path, ScanGroupCommitOutcome, ScanGroupCommitStage,
    SyncLibraryMediaBestEffortOutcome,
};
use time::OffsetDateTime;

use crate::local_metadata::{
    remove_media_local_metadata_source_tx, replace_media_local_metadata_source_tx,
    MediaLocalMetadataTarget, ReplaceMediaItemCreditParams, ReplaceMediaLocalMetadataSourceParams,
};

#[derive(Debug, Clone)]
pub struct CreateAudioTrackParams {
    pub stream_index: i32,
    pub language: Option<String>,
    pub audio_codec: Option<String>,
    pub label: Option<String>,
    pub channel_layout: Option<String>,
    pub channels: Option<i32>,
    pub bitrate: Option<i64>,
    pub sample_rate: Option<i32>,
    pub is_default: bool,
}

#[derive(Debug, Clone)]
pub struct CreateSubtitleTrackParams {
    pub source_kind: String,
    pub file_path: Option<String>,
    pub stream_index: Option<i32>,
    pub language: Option<String>,
    pub subtitle_format: String,
    pub label: Option<String>,
    pub is_default: bool,
    pub is_forced: bool,
    pub is_hearing_impaired: bool,
}

#[derive(Debug, Clone)]
pub struct CreateLocalMetadataSnapshotParams {
    pub source_path: String,
    pub document_type: String,
    pub is_locked: bool,
    pub is_selected: bool,
    pub payload: Value,
    pub external_ids: Vec<MediaExternalId>,
    pub ratings: Vec<MediaRating>,
    pub credits: Vec<ReplaceMediaItemCreditParams>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryMediaFileMembership {
    pub media_item_id: i64,
    /// The shared metadata owner for this physical file. Movies own their
    /// metadata directly; episode files inherit their series item as owner.
    pub logical_metadata_owner_id: i64,
    pub file_path: String,
}

/// 重建某个媒体库内容时，每个视频文件对应的一组入库参数。
#[derive(Debug, Clone)]
pub struct CreateMediaEntryParams {
    pub library_id: i64,
    pub media_type: String,
    pub metadata_provider: Option<String>,
    pub metadata_provider_item_id: Option<String>,
    pub metadata_status: String,
    pub metadata_failure_reason: Option<String>,
    pub allow_artwork_clear: bool,
    pub replace_remote_data: bool,
    pub tmdb_remote_snapshot_json: Option<String>,
    pub tmdb_remote_snapshot_renews_retention: bool,
    pub remote_media_type: Option<String>,
    pub title: String,
    pub source_title: String,
    pub original_title: Option<String>,
    pub sort_title: Option<String>,
    pub year: Option<i32>,
    pub tagline: Option<String>,
    pub premiere_date: Option<time::Date>,
    pub content_rating: Option<String>,
    pub external_ids: Vec<MediaExternalId>,
    pub ratings: Vec<MediaRating>,
    pub country: Option<String>,
    pub genres: Option<String>,
    pub studio: Option<String>,
    pub season_number: Option<i32>,
    pub season_title: Option<String>,
    pub season_overview: Option<String>,
    pub season_poster_path: Option<String>,
    pub season_backdrop_path: Option<String>,
    pub episode_number: Option<i32>,
    pub episode_title: Option<String>,
    pub episode_original_title: Option<String>,
    pub episode_sort_title: Option<String>,
    pub episode_year: Option<i32>,
    pub episode_overview: Option<String>,
    pub episode_tagline: Option<String>,
    pub episode_premiere_date: Option<time::Date>,
    pub episode_content_rating: Option<String>,
    pub overview: Option<String>,
    pub series_poster_path: Option<String>,
    pub series_backdrop_path: Option<String>,
    pub series_logo_path: Option<String>,
    pub local_nfo: Option<CreateLocalMetadataSnapshotParams>,
    pub series_local_nfo: Option<CreateLocalMetadataSnapshotParams>,
    pub removed_local_nfo_source_path: Option<String>,
    pub removed_series_local_nfo_source_path: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub logo_path: Option<String>,
    pub file_path: String,
    pub source_kind: MediaSourceKind,
    pub stream_reference_hash: Option<String>,
    pub container: Option<String>,
    pub file_size: i64,
    pub duration_seconds: Option<i32>,
    pub video_title: Option<String>,
    pub video_codec: Option<String>,
    pub video_profile: Option<String>,
    pub video_level: Option<String>,
    pub audio_codec: Option<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub bitrate: Option<i64>,
    pub video_bitrate: Option<i64>,
    pub video_frame_rate: Option<f64>,
    pub video_aspect_ratio: Option<String>,
    pub video_scan_type: Option<String>,
    pub video_color_primaries: Option<String>,
    pub video_color_space: Option<String>,
    pub video_color_transfer: Option<String>,
    pub video_bit_depth: Option<i32>,
    pub video_pixel_format: Option<String>,
    pub video_reference_frames: Option<i32>,
    pub technical_tags: Vec<String>,
    pub audio_tracks: Vec<CreateAudioTrackParams>,
    pub subtitle_tracks: Vec<CreateSubtitleTrackParams>,
    pub local_analysis_version: i32,
    pub scan_hash: Option<String>,
}

/// A season projection committed together with a parent-series metadata refresh.
#[derive(Debug, Clone)]
pub struct UpdateSeasonMetadataParams {
    pub season_number: i32,
    pub title: String,
    pub overview: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
}

/// 手动刷新单个媒体条目时允许更新的 metadata 字段。
#[derive(Debug, Clone)]
pub struct UpdateMediaItemMetadataParams {
    pub expected_updated_at: OffsetDateTime,
    pub title: String,
    pub source_title: String,
    pub original_title: Option<String>,
    pub sort_title: Option<String>,
    pub metadata_provider: Option<String>,
    pub metadata_provider_item_id: Option<String>,
    pub metadata_status: String,
    pub metadata_failure_reason: Option<String>,
    pub replace_remote_data: bool,
    pub tmdb_remote_snapshot_json: Option<String>,
    pub tmdb_remote_snapshot_renews_retention: bool,
    pub remote_media_type: Option<String>,
    pub year: Option<i32>,
    pub tagline: Option<String>,
    pub premiere_date: Option<time::Date>,
    pub content_rating: Option<String>,
    pub seasons: Vec<UpdateSeasonMetadataParams>,
    pub local_nfos: Vec<CreateLocalMetadataSnapshotParams>,
    pub removed_local_nfo_source_paths: Vec<String>,
    pub external_ids: Vec<MediaExternalId>,
    pub ratings: Vec<MediaRating>,
    pub country: Option<String>,
    pub genres: Option<String>,
    pub studio: Option<String>,
    pub overview: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub logo_path: Option<String>,
}

/// Result of an administrator-initiated metadata mutation.
///
/// The database is authoritative for active-scan exclusion so separate server instances cannot
/// write manual metadata while a worker is reconciling the same library.
#[derive(Debug)]
pub enum UpdateMediaItemMetadataOutcome {
    Updated(Box<MediaItem>),
    Missing,
    Stale,
    ActiveScan(ScanJob),
}

pub(super) async fn persist_local_metadata_snapshot_tx(
    tx: &mut Transaction<'_, Postgres>,
    library_id: i64,
    target: MediaLocalMetadataTarget,
    snapshot: &CreateLocalMetadataSnapshotParams,
) -> Result<()> {
    replace_media_local_metadata_source_tx(
        tx,
        ReplaceMediaLocalMetadataSourceParams {
            library_id,
            target,
            source_path: snapshot.source_path.clone(),
            document_type: snapshot.document_type.clone(),
            is_locked: snapshot.is_locked,
            is_selected: snapshot.is_selected,
            payload: snapshot.payload.clone(),
            external_ids: snapshot.external_ids.clone(),
            ratings: snapshot.ratings.clone(),
            credits: snapshot.credits.clone(),
        },
    )
    .await?;
    Ok(())
}

pub(super) async fn reconcile_local_metadata_snapshot_tx(
    tx: &mut Transaction<'_, Postgres>,
    library_id: i64,
    target: MediaLocalMetadataTarget,
    removed_source_path: Option<&str>,
    snapshot: Option<&CreateLocalMetadataSnapshotParams>,
) -> Result<()> {
    if let Some(source_path) = removed_source_path {
        remove_media_local_metadata_source_tx(tx, library_id, source_path).await?;
    }
    if let Some(snapshot) = snapshot {
        persist_local_metadata_snapshot_tx(tx, library_id, target, snapshot).await?;
    }
    Ok(())
}

pub(super) async fn reconcile_local_metadata_snapshots_tx(
    tx: &mut Transaction<'_, Postgres>,
    library_id: i64,
    target: MediaLocalMetadataTarget,
    removed_source_paths: &[String],
    snapshots: &[CreateLocalMetadataSnapshotParams],
) -> Result<()> {
    let mut snapshots_by_path = std::collections::BTreeMap::new();
    for snapshot in snapshots {
        match snapshots_by_path.entry(snapshot.source_path.as_str()) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(snapshot);
            }
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                let existing = *entry.get();
                if existing.document_type != snapshot.document_type
                    || existing.payload != snapshot.payload
                {
                    anyhow::bail!(
                        "conflicting local metadata snapshots share source path {}",
                        snapshot.source_path
                    );
                }
                if snapshot.is_selected && !existing.is_selected {
                    entry.insert(snapshot);
                }
            }
        }
    }
    let selected_count = snapshots_by_path
        .values()
        .filter(|snapshot| snapshot.is_selected)
        .count();
    if selected_count > 1 {
        anyhow::bail!("multiple local metadata snapshots were selected for one target");
    }

    for source_path in removed_source_paths {
        remove_media_local_metadata_source_tx(tx, library_id, source_path).await?;
    }

    let mut ordered = snapshots_by_path.into_values().collect::<Vec<_>>();
    ordered.sort_by(|left, right| {
        left.is_selected
            .cmp(&right.is_selected)
            .then_with(|| left.source_path.cmp(&right.source_path))
    });
    for snapshot in ordered {
        persist_local_metadata_snapshot_tx(tx, library_id, target, snapshot).await?;
    }

    Ok(())
}

#[derive(Debug, Clone, Copy)]
pub(super) enum LocalMetadataProjectionScope {
    Movie,
    Series,
    Episode,
}

#[derive(Debug, Clone, Copy, Default)]
struct LocalMetadataProjectionFields {
    title: bool,
    original_title: bool,
    sort_title: bool,
    year: bool,
    tagline: bool,
    premiere_date: bool,
    content_rating: bool,
    country: bool,
    genres: bool,
    studio: bool,
    overview: bool,
    poster: bool,
    backdrop: bool,
    logo: bool,
}

#[derive(Debug, Clone)]
pub(super) struct LocalMetadataProjectionCheckpoint {
    selected_source_path: Option<String>,
    title: String,
    source_title: String,
    original_title: Option<String>,
    sort_title: Option<String>,
    year: Option<i32>,
    tagline: Option<String>,
    premiere_date: Option<time::Date>,
    content_rating: Option<String>,
    country: Option<String>,
    genres: Option<String>,
    studio: Option<String>,
    overview: Option<String>,
    poster_path: Option<String>,
    backdrop_path: Option<String>,
    logo_path: Option<String>,
}

impl LocalMetadataProjectionCheckpoint {
    fn apply_selected_payload(&mut self, payload: &Value) -> bool {
        let Some(projection) = payload.get("public_projection").and_then(Value::as_object) else {
            return false;
        };
        if let Some(value) = projection.get("title").and_then(Value::as_str) {
            self.title = value.to_string();
        }
        if let Some(value) = projection.get("source_title").and_then(Value::as_str) {
            self.source_title = value.to_string();
        }
        self.original_title = json_optional_string(projection, "original_title");
        self.sort_title = json_optional_string(projection, "sort_title");
        self.year = projection
            .get("year")
            .and_then(Value::as_i64)
            .and_then(|value| i32::try_from(value).ok());
        self.tagline = json_optional_string(projection, "tagline");
        self.content_rating = json_optional_string(projection, "content_rating");
        self.country = json_optional_string(projection, "country");
        self.genres = json_optional_string(projection, "genres");
        self.studio = json_optional_string(projection, "studio");
        self.overview = json_optional_string(projection, "overview");
        self.poster_path = json_optional_string(projection, "poster_path");
        self.backdrop_path = json_optional_string(projection, "backdrop_path");
        self.logo_path = json_optional_string(projection, "logo_path");
        true
    }
}

fn json_optional_string(projection: &serde_json::Map<String, Value>, name: &str) -> Option<String> {
    projection
        .get(name)
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// Capture the last authoritative public projection before a scan entry can
/// move another local group onto this logical media item.
pub(super) async fn capture_local_metadata_projection_checkpoint_tx(
    tx: &mut Transaction<'_, Postgres>,
    media_item_id: i64,
) -> Result<LocalMetadataProjectionCheckpoint> {
    let row = sqlx::query(
        r#"
        select
            mi.title,
            mi.source_title,
            mi.original_title,
            mi.sort_title,
            mi.year,
            mi.tagline,
            mi.premiere_date,
            mi.content_rating,
            mi.country,
            mi.genres,
            mi.studio,
            mi.overview,
            mi.poster_path,
            mi.backdrop_path,
            mi.logo_path,
            source.source_path as selected_source_path
        from media_items mi
        left join lateral (
            select source_path, payload
            from media_local_metadata_sources
            where media_item_id = mi.id and is_selected
            order by id
            limit 1
        ) source on true
        where mi.id = $1
        for update of mi
        "#,
    )
    .bind(media_item_id)
    .fetch_one(&mut **tx)
    .await?;

    Ok(LocalMetadataProjectionCheckpoint {
        selected_source_path: row.get("selected_source_path"),
        title: row.get("title"),
        source_title: row.get("source_title"),
        original_title: row.get("original_title"),
        sort_title: row.get("sort_title"),
        year: row.get("year"),
        tagline: row.get("tagline"),
        premiere_date: row.get("premiere_date"),
        content_rating: row.get("content_rating"),
        country: row.get("country"),
        genres: row.get("genres"),
        studio: row.get("studio"),
        overview: row.get("overview"),
        poster_path: row.get("poster_path"),
        backdrop_path: row.get("backdrop_path"),
        logo_path: row.get("logo_path"),
    })
}

/// If an entry-local winner loses after provider-ID groups converge, restore
/// every field owned by either contender from the database's previously
/// selected source. This makes the final selected row, rather than processing
/// order, authoritative for public fields.
pub(super) async fn restore_authoritative_local_metadata_projection_tx(
    tx: &mut Transaction<'_, Postgres>,
    media_item_id: i64,
    checkpoint: &LocalMetadataProjectionCheckpoint,
    incoming: Option<&CreateLocalMetadataSnapshotParams>,
    scope: LocalMetadataProjectionScope,
) -> Result<()> {
    let Some((selected_source_path, selected_payload)) = sqlx::query_as::<_, (String, Value)>(
        r#"
            select source_path, payload
            from media_local_metadata_sources
            where media_item_id = $1 and is_selected
            order by id
            limit 1
            "#,
    )
    .bind(media_item_id)
    .fetch_optional(&mut **tx)
    .await?
    else {
        return Ok(());
    };

    let selected_is_incoming = incoming.is_some_and(|snapshot| {
        snapshot.is_selected && snapshot.source_path == selected_source_path
    });
    let selected_is_checkpoint =
        checkpoint.selected_source_path.as_deref() == Some(selected_source_path.as_str());
    if !selected_is_incoming && !selected_is_checkpoint {
        return Ok(());
    }

    let mut fields = LocalMetadataProjectionFields::from_payload(&selected_payload, scope);
    if selected_is_checkpoint {
        if let Some(incoming) = incoming {
            fields.merge(LocalMetadataProjectionFields::from_payload(
                &incoming.payload,
                scope,
            ));
        }
    }
    let mut values = checkpoint.clone();
    let has_persisted_projection = values.apply_selected_payload(&selected_payload);
    if selected_is_incoming && !has_persisted_projection {
        // Older snapshots have no resolved artwork/public projection. The
        // incoming entry already carries their correctly merged values.
        return Ok(());
    }
    if selected_is_incoming {
        // Date parsing remains an application responsibility; the incoming
        // entry already wrote this field before the final source was chosen.
        fields.premiere_date = false;
    }
    if !fields.any() {
        return Ok(());
    }

    update_local_metadata_projection_fields_tx(tx, media_item_id, fields, &values).await
}

async fn update_local_metadata_projection_fields_tx(
    tx: &mut Transaction<'_, Postgres>,
    media_item_id: i64,
    fields: LocalMetadataProjectionFields,
    values: &LocalMetadataProjectionCheckpoint,
) -> Result<()> {
    sqlx::query(
        r#"
        update media_items
        set title = case when $2 then $16 else title end,
            source_title = case when $2 then $17 else source_title end,
            original_title = case when $3 then $18 else original_title end,
            sort_title = case when $4 then $19 else sort_title end,
            year = case when $5 then $20 else year end,
            tagline = case when $6 then $21 else tagline end,
            premiere_date = case when $7 then $22 else premiere_date end,
            content_rating = case when $8 then $23 else content_rating end,
            country = case when $9 then $24 else country end,
            genres = case when $10 then $25 else genres end,
            studio = case when $11 then $26 else studio end,
            overview = case when $12 then $27 else overview end,
            poster_path = case when $13 then $28 else poster_path end,
            backdrop_path = case when $14 then $29 else backdrop_path end,
            logo_path = case when $15 then $30 else logo_path end,
            updated_at = now()
        where id = $1
        "#,
    )
    .bind(media_item_id)
    .bind(fields.title)
    .bind(fields.original_title)
    .bind(fields.sort_title)
    .bind(fields.year)
    .bind(fields.tagline)
    .bind(fields.premiere_date)
    .bind(fields.content_rating)
    .bind(fields.country)
    .bind(fields.genres)
    .bind(fields.studio)
    .bind(fields.overview)
    .bind(fields.poster)
    .bind(fields.backdrop)
    .bind(fields.logo)
    .bind(&values.title)
    .bind(&values.source_title)
    .bind(&values.original_title)
    .bind(&values.sort_title)
    .bind(values.year)
    .bind(&values.tagline)
    .bind(values.premiere_date)
    .bind(&values.content_rating)
    .bind(&values.country)
    .bind(&values.genres)
    .bind(&values.studio)
    .bind(&values.overview)
    .bind(&values.poster_path)
    .bind(&values.backdrop_path)
    .bind(&values.logo_path)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

impl LocalMetadataProjectionFields {
    fn from_payload(payload: &Value, scope: LocalMetadataProjectionScope) -> Self {
        let Some(metadata) = payload.get("metadata") else {
            return Self::default();
        };
        let has_text = |name: &str| {
            metadata
                .get(name)
                .and_then(Value::as_str)
                .is_some_and(|value| !value.trim().is_empty())
        };
        let has_values = |name: &str| {
            metadata
                .get(name)
                .and_then(Value::as_array)
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
                .and_then(Value::as_array)
                .is_some_and(|values| !values.is_empty())
        };
        let episode = matches!(scope, LocalMetadataProjectionScope::Episode);

        Self {
            title: has_text("title"),
            original_title: has_text("original_title"),
            sort_title: has_text("sort_title"),
            year: metadata.get("year").is_some_and(|value| !value.is_null()),
            tagline: has_text("tagline"),
            premiere_date: has_text("premiered") || has_text("aired"),
            content_rating: has_text("content_rating"),
            country: !episode && has_values("countries"),
            genres: !episode && has_values("genres"),
            studio: !episode && has_values("studios"),
            overview: has_text("overview") || has_text("outline"),
            poster: if episode {
                has_artwork("thumbnails") || has_artwork("posters")
            } else {
                has_artwork("posters")
            },
            backdrop: has_artwork("backdrops"),
            logo: has_artwork("logos"),
        }
    }

    fn merge(&mut self, other: Self) {
        self.title |= other.title;
        self.original_title |= other.original_title;
        self.sort_title |= other.sort_title;
        self.year |= other.year;
        self.tagline |= other.tagline;
        self.premiere_date |= other.premiere_date;
        self.content_rating |= other.content_rating;
        self.country |= other.country;
        self.genres |= other.genres;
        self.studio |= other.studio;
        self.overview |= other.overview;
        self.poster |= other.poster;
        self.backdrop |= other.backdrop;
        self.logo |= other.logo;
    }

    fn any(self) -> bool {
        self.title
            || self.original_title
            || self.sort_title
            || self.year
            || self.tagline
            || self.premiere_date
            || self.content_rating
            || self.country
            || self.genres
            || self.studio
            || self.overview
            || self.poster
            || self.backdrop
            || self.logo
    }
}

/// Project only fields owned by the previously or currently selected NFO.
/// Provider identity, match state, and remote-only gaps remain untouched.
pub(super) async fn apply_local_metadata_projection_tx(
    tx: &mut Transaction<'_, Postgres>,
    media_item_id: i64,
    entry: &CreateMediaEntryParams,
    scope: LocalMetadataProjectionScope,
) -> Result<()> {
    let (snapshot, removed_source_path) = match scope {
        LocalMetadataProjectionScope::Series => (
            entry.series_local_nfo.as_ref(),
            entry.removed_series_local_nfo_source_path.as_deref(),
        ),
        LocalMetadataProjectionScope::Movie | LocalMetadataProjectionScope::Episode => (
            entry.local_nfo.as_ref(),
            entry.removed_local_nfo_source_path.as_deref(),
        ),
    };
    let previous = sqlx::query_as::<_, (String, Value)>(
        r#"
        select source_path, payload
        from media_local_metadata_sources
        where media_item_id = $1 and is_selected
        order by id
        limit 1
        for update
        "#,
    )
    .bind(media_item_id)
    .fetch_optional(&mut **tx)
    .await?;

    let removes_previous = previous.as_ref().is_some_and(|(source_path, _)| {
        removed_source_path == Some(source_path.as_str())
            || snapshot.is_some_and(|snapshot| {
                !snapshot.is_selected && snapshot.source_path == *source_path
            })
    });
    let selects_current = snapshot.is_some_and(|snapshot| snapshot.is_selected);
    if !removes_previous && !selects_current {
        return Ok(());
    }

    let mut fields = LocalMetadataProjectionFields::default();
    if let Some((_, payload)) = previous.as_ref() {
        fields.merge(LocalMetadataProjectionFields::from_payload(payload, scope));
    }
    if let Some(snapshot) = snapshot.filter(|snapshot| snapshot.is_selected) {
        fields.merge(LocalMetadataProjectionFields::from_payload(
            &snapshot.payload,
            scope,
        ));
    }
    if !fields.any() {
        return Ok(());
    }

    let (
        title,
        source_title,
        original_title,
        sort_title,
        year,
        tagline,
        premiere_date,
        content_rating,
        country,
        genres,
        studio,
        overview,
        poster,
        backdrop,
        logo,
    ) = match scope {
        LocalMetadataProjectionScope::Movie => (
            display_entry_title(entry),
            entry.source_title.clone(),
            entry.original_title.clone(),
            entry.sort_title.clone(),
            entry.year,
            entry.tagline.clone(),
            entry.premiere_date,
            entry.content_rating.clone(),
            entry.country.clone(),
            entry.genres.clone(),
            entry.studio.clone(),
            entry.overview.clone(),
            entry.poster_path.clone(),
            entry.backdrop_path.clone(),
            entry.logo_path.clone(),
        ),
        LocalMetadataProjectionScope::Series => (
            display_entry_title(entry),
            entry.source_title.clone(),
            entry.original_title.clone(),
            entry.sort_title.clone(),
            entry.year,
            entry.tagline.clone(),
            entry.premiere_date,
            entry.content_rating.clone(),
            entry.country.clone(),
            entry.genres.clone(),
            entry.studio.clone(),
            entry.overview.clone(),
            entry.series_poster_path.clone(),
            entry.series_backdrop_path.clone(),
            entry.series_logo_path.clone(),
        ),
        LocalMetadataProjectionScope::Episode => {
            let episode_number = entry.episode_number.unwrap_or(0);
            let episode_title = entry
                .episode_title
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| format!("Episode {episode_number}"));
            (
                episode_title.clone(),
                episode_title,
                entry.episode_original_title.clone(),
                entry.episode_sort_title.clone(),
                entry.episode_year,
                entry.episode_tagline.clone(),
                entry.episode_premiere_date,
                entry.episode_content_rating.clone(),
                None,
                None,
                None,
                entry.episode_overview.clone(),
                entry.poster_path.clone(),
                entry.backdrop_path.clone(),
                entry.logo_path.clone(),
            )
        }
    };

    sqlx::query(
        r#"
        update media_items
        set title = case when $2 then $16 else title end,
            source_title = case when $2 then $17 else source_title end,
            original_title = case when $3 then $18 else original_title end,
            sort_title = case when $4 then $19 else sort_title end,
            year = case when $5 then $20 else year end,
            tagline = case when $6 then $21 else tagline end,
            premiere_date = case when $7 then $22 else premiere_date end,
            content_rating = case when $8 then $23 else content_rating end,
            country = case when $9 then $24 else country end,
            genres = case when $10 then $25 else genres end,
            studio = case when $11 then $26 else studio end,
            overview = case when $12 then $27 else overview end,
            poster_path = case when $13 then $28 else poster_path end,
            backdrop_path = case when $14 then $29 else backdrop_path end,
            logo_path = case when $15 then $30 else logo_path end,
            updated_at = now()
        where id = $1
        "#,
    )
    .bind(media_item_id)
    .bind(fields.title)
    .bind(fields.original_title)
    .bind(fields.sort_title)
    .bind(fields.year)
    .bind(fields.tagline)
    .bind(fields.premiere_date)
    .bind(fields.content_rating)
    .bind(fields.country)
    .bind(fields.genres)
    .bind(fields.studio)
    .bind(fields.overview)
    .bind(fields.poster)
    .bind(fields.backdrop)
    .bind(fields.logo)
    .bind(title)
    .bind(source_title)
    .bind(original_title)
    .bind(sort_title)
    .bind(year)
    .bind(tagline)
    .bind(premiere_date)
    .bind(content_rating)
    .bind(country)
    .bind(genres)
    .bind(studio)
    .bind(overview)
    .bind(poster)
    .bind(backdrop)
    .bind(logo)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

fn display_entry_title(entry: &CreateMediaEntryParams) -> String {
    let title = entry.title.trim();
    if !title.is_empty() {
        return title.to_string();
    }
    let source_title = entry.source_title.trim();
    if !source_title.is_empty() {
        return source_title.to_string();
    }
    "Untitled".to_string()
}

/// 手动替换剧集元数据后，覆盖本地已存在季的远端 metadata。
#[derive(Debug, Clone)]
pub struct UpdateSeriesSeasonMetadataParams {
    pub series_id: i64,
    pub expected_provider_item_id: String,
    pub expected_media_item_updated_at: OffsetDateTime,
    pub season_number: i32,
    pub title: Option<String>,
    pub overview: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
}

/// 手动替换剧集元数据后，覆盖本地已存在集的远端 metadata。
#[derive(Debug, Clone)]
pub struct UpdateSeriesEpisodeMetadataParams {
    pub series_id: i64,
    pub expected_provider_item_id: String,
    pub expected_media_item_updated_at: OffsetDateTime,
    pub season_number: i32,
    pub episode_number: i32,
    pub title: Option<String>,
    pub overview: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
}

/// 刷新单个媒体文件时允许更新的源文件和探测字段。
#[derive(Debug, Clone)]
pub struct UpdateMediaFileMetadataParams {
    pub file_path: String,
    pub source_kind: MediaSourceKind,
    pub stream_reference_hash: Option<String>,
    pub container: Option<String>,
    pub file_size: i64,
    pub duration_seconds: Option<i32>,
    pub video_title: Option<String>,
    pub video_codec: Option<String>,
    pub video_profile: Option<String>,
    pub video_level: Option<String>,
    pub audio_codec: Option<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub bitrate: Option<i64>,
    pub video_bitrate: Option<i64>,
    pub video_frame_rate: Option<f64>,
    pub video_aspect_ratio: Option<String>,
    pub video_scan_type: Option<String>,
    pub video_color_primaries: Option<String>,
    pub video_color_space: Option<String>,
    pub video_color_transfer: Option<String>,
    pub video_bit_depth: Option<i32>,
    pub video_pixel_format: Option<String>,
    pub video_reference_frames: Option<i32>,
    pub technical_tags: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ListMediaItemsForLibraryParams {
    pub library_id: i64,
    pub query: Option<String>,
    pub year: Option<i32>,
    pub category: LibraryMediaCategory,
    pub sort_by: MediaItemSortBy,
    pub sort_order: SortOrder,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryMediaCategory {
    All,
    Movie,
    Series,
    NeedsReview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaItemSortBy {
    Title,
    Year,
    Rating,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortOrder {
    Asc,
    Desc,
}

#[derive(Debug, Clone)]
pub struct ListMediaItemsForLibraryResult {
    pub items: Vec<mova_domain::MediaItem>,
    pub total: i64,
}

#[derive(Debug, Clone)]
pub struct RecentlyAddedLibraryMediaItems {
    pub library: mova_domain::Library,
    pub items: Vec<mova_domain::MediaItem>,
    pub total: i64,
}

#[derive(Debug, Clone)]
pub struct GlobalSearchParams {
    pub query: String,
    pub visible_library_ids: Option<Vec<i64>>,
    pub limit: i64,
}

#[derive(Debug, Clone)]
pub struct GlobalSearchResult {
    pub kind: String,
    pub library_id: i64,
    pub library_name: String,
    pub media_item_id: i64,
    pub series_media_item_id: Option<i64>,
    pub media_type: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub year: Option<i32>,
    pub overview: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub season_number: Option<i32>,
    pub episode_number: Option<i32>,
    pub ratings: Vec<MediaRating>,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone)]
pub struct LibraryMediaTypeCounts {
    pub movie_count: i64,
    pub series_count: i64,
}

#[derive(Debug, Clone)]
pub struct MediaItemPlaybackHeader {
    pub media_item_id: i64,
    pub library_id: i64,
    pub media_type: String,
    pub series_media_item_id: Option<i64>,
    pub title: String,
    pub original_title: Option<String>,
    pub year: Option<i32>,
    pub logo_path: Option<String>,
    pub logo_updated_at: OffsetDateTime,
    pub season_id: Option<i64>,
    pub season_number: Option<i32>,
    pub episode_number: Option<i32>,
    pub episode_title: Option<String>,
    pub season_intro_start_seconds: Option<i32>,
    pub season_intro_end_seconds: Option<i32>,
    pub episode_intro_start_seconds: Option<i32>,
    pub episode_intro_end_seconds: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct ExistingMediaMetadataSummary {
    pub media_item_id: i64,
    pub logical_metadata_owner_id: i64,
    pub media_file_id: i64,
    pub file_path: String,
    pub media_type: String,
    pub metadata_provider: Option<String>,
    pub metadata_provider_item_id: Option<String>,
    pub metadata_status: String,
    pub metadata_failure_reason: Option<String>,
    pub remote_media_type: Option<String>,
    pub has_local_nfo: bool,
    pub local_nfo_source_path: Option<String>,
    pub local_nfo_payload: Option<serde_json::Value>,
    pub tmdb_remote_snapshot: Option<serde_json::Value>,
    pub title: String,
    pub source_title: String,
    pub original_title: Option<String>,
    pub sort_title: Option<String>,
    pub year: Option<i32>,
    pub tagline: Option<String>,
    pub premiere_date: Option<time::Date>,
    pub content_rating: Option<String>,
    pub country: Option<String>,
    pub genres: Option<String>,
    pub studio: Option<String>,
    pub overview: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub logo_path: Option<String>,
    pub scan_hash: Option<String>,
    pub source_kind: MediaSourceKind,
    pub stream_reference_hash: Option<String>,
    pub container: Option<String>,
    pub file_size: i64,
    pub duration_seconds: Option<i32>,
    pub video_title: Option<String>,
    pub video_codec: Option<String>,
    pub video_profile: Option<String>,
    pub video_level: Option<String>,
    pub audio_codec: Option<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub bitrate: Option<i64>,
    pub video_bitrate: Option<i64>,
    pub video_frame_rate: Option<f64>,
    pub video_aspect_ratio: Option<String>,
    pub video_scan_type: Option<String>,
    pub video_color_primaries: Option<String>,
    pub video_color_space: Option<String>,
    pub video_color_transfer: Option<String>,
    pub video_bit_depth: Option<i32>,
    pub video_pixel_format: Option<String>,
    pub video_reference_frames: Option<i32>,
    pub technical_tags: Vec<String>,
    pub local_analysis_version: i32,
    pub audio_tracks: Vec<CreateAudioTrackParams>,
    pub subtitle_tracks: Vec<CreateSubtitleTrackParams>,
    pub series_title: Option<String>,
    pub series_metadata_provider: Option<String>,
    pub series_metadata_provider_item_id: Option<String>,
    pub series_has_local_nfo: bool,
    pub series_local_nfo_source_path: Option<String>,
    pub series_local_nfo_payload: Option<serde_json::Value>,
    pub series_tmdb_remote_snapshot: Option<serde_json::Value>,
    pub series_source_title: Option<String>,
    pub series_original_title: Option<String>,
    pub series_sort_title: Option<String>,
    pub series_year: Option<i32>,
    pub series_tagline: Option<String>,
    pub series_premiere_date: Option<time::Date>,
    pub series_content_rating: Option<String>,
    pub series_country: Option<String>,
    pub series_genres: Option<String>,
    pub series_studio: Option<String>,
    pub series_overview: Option<String>,
    pub series_poster_path: Option<String>,
    pub series_backdrop_path: Option<String>,
    pub series_logo_path: Option<String>,
    pub season_title: Option<String>,
    pub season_number: Option<i32>,
    pub season_overview: Option<String>,
    pub season_poster_path: Option<String>,
    pub season_backdrop_path: Option<String>,
    pub episode_title: Option<String>,
    pub episode_number: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct SeriesEpisodeOutlineCacheEntry {
    pub series_media_item_id: i64,
    pub outline_json: String,
    pub fetched_at: OffsetDateTime,
    pub expires_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone)]
pub struct UpsertSeriesEpisodeOutlineCacheParams {
    pub series_media_item_id: i64,
    pub expected_provider_item_id: String,
    pub expected_media_item_updated_at: OffsetDateTime,
    pub outline_json: String,
    pub fetched_at: OffsetDateTime,
    pub expires_at: OffsetDateTime,
}
