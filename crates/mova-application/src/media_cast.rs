use crate::{
    error::{ApplicationError, ApplicationResult},
    libraries::get_library,
    media_classification::metadata_lookup_type_for_media_type,
    metadata::{MetadataLookup, MetadataProvider, RemoteCastMember},
};
use mova_domain::{MediaCastMember, MediaItem};
use sqlx::postgres::PgPool;
use std::{
    collections::HashSet,
    sync::{Arc, Mutex, OnceLock},
};
use time::OffsetDateTime;

const MAX_MEDIA_CAST_MEMBERS: usize = 20;

fn media_cast_inflight() -> &'static Mutex<HashSet<i64>> {
    static INFLIGHT: OnceLock<Mutex<HashSet<i64>>> = OnceLock::new();
    INFLIGHT.get_or_init(|| Mutex::new(HashSet::new()))
}

pub async fn list_media_item_cast(
    pool: &PgPool,
    media_item: &MediaItem,
    metadata_provider: Arc<dyn MetadataProvider>,
) -> ApplicationResult<Vec<MediaCastMember>> {
    if media_item.media_type.eq_ignore_ascii_case("episode") {
        return Ok(Vec::new());
    }

    let members = mova_db::list_media_item_cast_members(pool, media_item.id)
        .await
        .map_err(ApplicationError::from)?;

    if !members.is_empty() {
        return Ok(members);
    }

    let sync_record = mova_db::get_media_item_cast_cache(pool, media_item.id)
        .await
        .map_err(ApplicationError::from)?;
    let now = OffsetDateTime::now_utc();
    let has_persistent_sync_record = sync_record
        .as_ref()
        .is_some_and(|record| record.expires_at <= record.fetched_at);
    let has_unexpired_legacy_cache = sync_record
        .as_ref()
        .is_some_and(|record| record.expires_at > now);

    if has_persistent_sync_record || has_unexpired_legacy_cache || !metadata_provider.is_enabled() {
        return Ok(members);
    }

    if let Err(error) = ensure_media_item_cast(pool, media_item, metadata_provider).await {
        tracing::warn!(
            media_item_id = media_item.id,
            title = %media_item.title,
            error = ?error,
            "failed to sync media item cast on demand"
        );
    }

    mova_db::list_media_item_cast_members(pool, media_item.id)
        .await
        .map_err(ApplicationError::from)
}

pub async fn ensure_media_item_cast(
    pool: &PgPool,
    media_item: &MediaItem,
    metadata_provider: Arc<dyn MetadataProvider>,
) -> ApplicationResult<()> {
    if media_item.media_type.eq_ignore_ascii_case("episode") || !metadata_provider.is_enabled() {
        return Ok(());
    }

    {
        let mut inflight = media_cast_inflight()
            .lock()
            .map_err(|error| ApplicationError::Unexpected(anyhow::Error::msg(error.to_string())))?;
        if !inflight.insert(media_item.id) {
            tracing::debug!(
                media_item_id = media_item.id,
                title = %media_item.title,
                "media item cast sync already in progress"
            );
            return Ok(());
        }
    }

    let result = sync_media_item_cast(pool, media_item, metadata_provider).await;

    if let Ok(mut inflight) = media_cast_inflight().lock() {
        inflight.remove(&media_item.id);
    }

    result
}

pub async fn invalidate_media_item_cast_cache(
    pool: &PgPool,
    media_item_id: i64,
) -> ApplicationResult<()> {
    mova_db::delete_media_item_cast_cache(pool, media_item_id)
        .await
        .map_err(ApplicationError::from)
}

async fn sync_media_item_cast(
    pool: &PgPool,
    media_item: &MediaItem,
    metadata_provider: Arc<dyn MetadataProvider>,
) -> ApplicationResult<()> {
    let now = OffsetDateTime::now_utc();

    let library = get_library(pool, media_item.library_id).await?;
    let lookup = MetadataLookup {
        title: media_item.title.clone(),
        year: media_item.year,
        library_type: metadata_lookup_type_for_media_type(&media_item.media_type).to_string(),
        language: Some(library.metadata_language),
        provider_item_id: media_item.metadata_provider_item_id,
    };

    let remote_cast = match metadata_provider.lookup_cast(&lookup).await {
        Ok(remote_cast) => remote_cast,
        Err(error) => {
            tracing::warn!(
                media_item_id = media_item.id,
                title = %media_item.title,
                year = media_item.year,
                media_type = %media_item.media_type,
                error = ?error,
                "failed to fetch remote cast metadata"
            );
            return Ok(());
        }
    };

    if let Some(remote_cast) = remote_cast {
        let cast_members = normalize_remote_cast(media_item.id, remote_cast);
        persist_cast_members(pool, media_item.id, &cast_members, now).await?;
        return Ok(());
    }

    persist_cast_members(pool, media_item.id, &[], now).await?;

    Ok(())
}

async fn persist_cast_members(
    pool: &PgPool,
    media_item_id: i64,
    members: &[MediaCastMember],
    fetched_at: OffsetDateTime,
) -> ApplicationResult<()> {
    mova_db::replace_media_item_cast(
        pool,
        mova_db::ReplaceMediaItemCastParams {
            media_item_id,
            members: members
                .iter()
                .map(|member| mova_db::ReplaceMediaItemCastMember {
                    person_id: member.person_id,
                    sort_order: member.sort_order,
                    name: member.name.clone(),
                    character_name: member.character_name.clone(),
                    profile_path: member.profile_path.clone(),
                })
                .collect(),
            fetched_at,
            expires_at: fetched_at,
        },
    )
    .await
    .map_err(ApplicationError::from)
}

fn normalize_remote_cast(
    media_item_id: i64,
    remote_cast: Vec<RemoteCastMember>,
) -> Vec<MediaCastMember> {
    let mut cast_members = remote_cast
        .into_iter()
        .filter_map(|member| {
            let name = member.name.trim();
            if name.is_empty() {
                return None;
            }

            Some(MediaCastMember {
                media_item_id,
                person_id: member.person_id,
                sort_order: member.sort_order,
                name: name.to_string(),
                character_name: member.character_name.and_then(|value| {
                    let trimmed = value.trim();
                    if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed.to_string())
                    }
                }),
                profile_path: member.profile_path.and_then(|value| {
                    let trimmed = value.trim();
                    if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed.to_string())
                    }
                }),
            })
        })
        .collect::<Vec<_>>();

    cast_members.sort_by(|left, right| {
        left.sort_order
            .cmp(&right.sort_order)
            .then_with(|| left.name.cmp(&right.name))
    });
    cast_members.truncate(MAX_MEDIA_CAST_MEMBERS);
    for (index, member) in cast_members.iter_mut().enumerate() {
        member.sort_order = i32::try_from(index).unwrap_or(i32::MAX);
    }
    cast_members
}

#[cfg(test)]
mod tests {
    use super::normalize_remote_cast;
    use crate::metadata::RemoteCastMember;

    #[test]
    fn normalize_remote_cast_orders_and_limits_members() {
        let cast = normalize_remote_cast(
            42,
            vec![
                RemoteCastMember {
                    person_id: Some(3),
                    sort_order: 10,
                    name: "Third".to_string(),
                    character_name: None,
                    profile_path: None,
                },
                RemoteCastMember {
                    person_id: Some(1),
                    sort_order: 0,
                    name: "First".to_string(),
                    character_name: Some("Lead".to_string()),
                    profile_path: None,
                },
                RemoteCastMember {
                    person_id: Some(2),
                    sort_order: 5,
                    name: "Second".to_string(),
                    character_name: None,
                    profile_path: None,
                },
            ],
        );

        assert_eq!(cast.len(), 3);
        assert_eq!(cast[0].name, "First");
        assert_eq!(cast[1].name, "Second");
        assert_eq!(cast[2].name, "Third");
        assert_eq!(cast[0].character_name.as_deref(), Some("Lead"));
    }
}
