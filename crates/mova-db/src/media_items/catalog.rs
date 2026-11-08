use anyhow::Result;
use mova_domain::MediaItem;
use mova_domain::{AudioTrack, MediaFile, PlaybackProgress, SubtitleFile};
use sqlx::{PgPool, Postgres, QueryBuilder, Row};
use std::collections::HashMap;

pub struct EpisodeCoordinates {
    pub series_id: i64,
    pub series_name: String,
    pub season_id: i64,
    pub season_number: i32,
    pub episode_number: i32,
}

pub struct CatalogDetails {
    pub files: HashMap<i64, Vec<MediaFile>>,
    pub progress: HashMap<i64, PlaybackProgress>,
    pub episodes: HashMap<i64, EpisodeCoordinates>,
    pub audio: HashMap<i64, Vec<AudioTrack>>,
    pub subtitles: HashMap<i64, Vec<SubtitleFile>>,
}

pub async fn catalog_details(pool: &PgPool, user_id: i64, ids: &[i64]) -> Result<CatalogDetails> {
    let rows =
        sqlx::query("select * from media_files where media_item_id=any($1) order by created_at,id")
            .bind(ids)
            .fetch_all(pool)
            .await?;
    let mut files: HashMap<i64, Vec<MediaFile>> = HashMap::new();
    let mut file_ids = Vec::new();
    for row in rows {
        let f = super::query::map_media_file_row(row);
        file_ids.push(f.id);
        files.entry(f.media_item_id).or_default().push(f);
    }
    let rows=sqlx::query("select e.media_item_id,e.episode_number,s.id as season_id,s.season_number,s.series_id,mi.title as series_name from episodes e join seasons s on s.id=e.season_id join media_items mi on mi.id=s.series_id where e.media_item_id=any($1)")
        .bind(ids).fetch_all(pool).await?;
    let episodes = rows
        .into_iter()
        .map(|r| {
            (
                r.get("media_item_id"),
                EpisodeCoordinates {
                    series_id: r.get("series_id"),
                    series_name: r.get("series_name"),
                    season_id: r.get("season_id"),
                    season_number: r.get("season_number"),
                    episode_number: r.get("episode_number"),
                },
            )
        })
        .collect();
    let mut audio: HashMap<i64, Vec<AudioTrack>> = HashMap::new();
    for track in super::list_audio_tracks_for_media_files(pool, &file_ids).await? {
        audio.entry(track.media_file_id).or_default().push(track);
    }
    let mut subtitles: HashMap<i64, Vec<SubtitleFile>> = HashMap::new();
    for track in super::list_subtitle_files_for_media_files(pool, &file_ids).await? {
        subtitles
            .entry(track.media_file_id)
            .or_default()
            .push(track);
    }
    Ok(CatalogDetails {
        files,
        episodes,
        progress: crate::list_playback_progress_for_media_items(pool, user_id, ids).await?,
        audio,
        subtitles,
    })
}

#[derive(Debug, Clone, Default)]
pub struct CatalogQuery {
    pub user_id: i64,
    pub library_ids: Option<Vec<i64>>,
    pub library_id: Option<i64>,
    pub series_id: Option<i64>,
    pub season_id: Option<i64>,
    pub item_ids: Option<Vec<i64>>,
    pub kinds: Vec<String>,
    pub search: Option<String>,
    pub resume: bool,
    pub sort: Vec<(String, bool)>,
    pub offset: i64,
    pub limit: i64,
}

pub struct CatalogResult {
    pub items: Vec<MediaItem>,
    pub total: i64,
}

fn conditions<'a>(q: &'a CatalogQuery, sql: &mut QueryBuilder<'a, Postgres>) {
    sql.push(" from media_items mi left join episodes e on e.media_item_id=mi.id left join seasons s on s.id=e.season_id left join playback_progress pp on pp.media_item_id=mi.id and pp.user_id=")
        .push_bind(q.user_id).push(" where mi.media_type=any(").push_bind(&q.kinds).push(")");
    if let Some(ids) = &q.library_ids {
        sql.push(" and mi.library_id=any(").push_bind(ids).push(")");
    }
    if let Some(id) = q.library_id {
        sql.push(" and mi.library_id=").push_bind(id);
    }
    if let Some(id) = q.series_id {
        sql.push(" and s.series_id=").push_bind(id);
    }
    if let Some(id) = q.season_id {
        sql.push(" and e.season_id=").push_bind(id);
    }
    if let Some(ids) = &q.item_ids {
        sql.push(" and mi.id=any(").push_bind(ids).push(")");
    }
    sql.push(" and (mi.media_type='series' or exists(select 1 from media_files mf where mf.media_item_id=mi.id))");
    if let Some(search) = &q.search {
        sql.push(" and (mi.title ilike ")
            .push_bind(format!("%{}%", escape_like(search)))
            .push(" or mi.original_title ilike ")
            .push_bind(format!("%{}%", escape_like(search)))
            .push(")");
    }
    if q.resume {
        sql.push(" and pp.position_seconds>0 and not pp.is_finished and mi.media_type in ('movie','episode')");
    }
}

fn escape_like(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

pub async fn query_catalog(pool: &PgPool, q: &CatalogQuery) -> Result<CatalogResult> {
    let mut tx = pool.begin().await?;
    sqlx::query("set transaction isolation level repeatable read read only")
        .execute(&mut *tx)
        .await?;
    let mut count = QueryBuilder::new("select count(*)");
    conditions(q, &mut count);
    let total = count
        .build_query_scalar::<i64>()
        .fetch_one(&mut *tx)
        .await?;
    let mut sql = QueryBuilder::new("select mi.*");
    conditions(q, &mut sql);
    sql.push(" order by ");
    for (sort, descending) in &q.sort {
        let expression = match sort.as_str() {
            "DateCreated" => "mi.created_at",
            "DatePlayed" => "pp.last_watched_at",
            "ProductionYear" => "mi.year",
            "PremiereDate" => "mi.premiere_date",
            "IndexNumber" => "e.episode_number",
            "ParentIndexNumber" => "s.season_number",
            _ => "coalesce(mi.sort_title,mi.title)",
        };
        sql.push(expression).push(if *descending {
            " desc nulls last,"
        } else {
            " asc nulls last,"
        });
    }
    sql.push("mi.id asc limit ")
        .push_bind(q.limit)
        .push(" offset ")
        .push_bind(q.offset);
    let rows = sql.build().fetch_all(&mut *tx).await?;
    tx.commit().await?;
    let mut items: Vec<_> = rows
        .into_iter()
        .map(super::query::map_media_item_row)
        .collect();
    let ids: Vec<_> = items.iter().map(|i| i.id).collect();
    let mut ratings = super::list_media_item_ratings(pool, &ids).await?;
    for item in &mut items {
        item.ratings = ratings.remove(&item.id).unwrap_or_default();
    }
    Ok(CatalogResult { items, total })
}
