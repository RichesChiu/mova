use crate::{
    error::ApplicationResult,
    media_items::{list_recently_added_media_items_by_library, ListRecentlyAddedByLibraryInput},
    playback_progress::list_continue_watching,
};
use mova_domain::{ContinueWatchingItem, LibraryDetail, MediaItem};
use sqlx::PgPool;

const HOME_LIBRARY_PREVIEW_LIMIT: i64 = 16;
const HOME_RECENTLY_ADDED_LIMIT: i64 = 8;
const HOME_CONTINUE_WATCHING_LIMIT: i64 = 20;

#[derive(Debug)]
pub struct HomeSnapshot {
    pub libraries: Vec<HomeLibrarySnapshot>,
    pub recently_added: Vec<crate::media_items::RecentlyAddedLibraryMediaItems>,
    pub continue_watching: Vec<ContinueWatchingItem>,
}

#[derive(Debug)]
pub struct HomeLibrarySnapshot {
    pub detail: LibraryDetail,
    pub preview_items: Vec<MediaItem>,
}

pub async fn get_home_snapshot(
    pool: &PgPool,
    user_id: i64,
    visible_library_ids: Option<Vec<i64>>,
) -> ApplicationResult<HomeSnapshot> {
    let library_details = mova_db::list_library_details(pool, visible_library_ids.as_deref())
        .await
        .map_err(crate::error::ApplicationError::from)?;
    let library_ids = library_details
        .iter()
        .map(|detail| detail.library.id)
        .collect::<Vec<_>>();
    let mut previews_by_library = mova_db::list_media_item_previews_by_library(
        pool,
        &library_ids,
        HOME_LIBRARY_PREVIEW_LIMIT,
    )
    .await
    .map_err(crate::error::ApplicationError::from)?;
    let library_snapshots = library_details
        .into_iter()
        .map(|detail| HomeLibrarySnapshot {
            preview_items: previews_by_library
                .remove(&detail.library.id)
                .unwrap_or_default(),
            detail,
        })
        .collect();

    let recently_added = list_recently_added_media_items_by_library(
        pool,
        ListRecentlyAddedByLibraryInput {
            visible_library_ids: visible_library_ids.clone(),
            days: None,
            limit: Some(HOME_RECENTLY_ADDED_LIMIT),
        },
    )
    .await?;
    let continue_watching =
        list_continue_watching(pool, user_id, Some(HOME_CONTINUE_WATCHING_LIMIT))
            .await?
            .into_iter()
            .filter(|item| {
                visible_library_ids
                    .as_ref()
                    .is_none_or(|ids| ids.contains(&item.media_item.library_id))
            })
            .collect();

    Ok(HomeSnapshot {
        libraries: library_snapshots,
        recently_added,
        continue_watching,
    })
}
