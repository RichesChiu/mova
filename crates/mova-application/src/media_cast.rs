use crate::{
    error::{ApplicationError, ApplicationResult},
    libraries::get_library,
    media_classification::metadata_lookup_type_for_media_type,
    metadata::{MetadataLookup, MetadataProvider, RemoteCastMember},
};
use mova_domain::{MediaCastMember, MediaItem};
use sqlx::postgres::PgPool;
use std::sync::Arc;
use time::{Duration, OffsetDateTime};

const MEDIA_CAST_CACHE_TTL_SECONDS: i64 = 7 * 24 * 60 * 60;
const MEDIA_CAST_FAILURE_CACHE_TTL_SECONDS: i64 = 30 * 60;
const MAX_MEDIA_CAST_MEMBERS: usize = 20;

pub async fn list_media_item_cast(
    pool: &PgPool,
    media_item: &MediaItem,
    metadata_provider: Arc<dyn MetadataProvider>,
) -> ApplicationResult<Vec<MediaCastMember>> {
    if media_item.media_type.eq_ignore_ascii_case("episode") {
        return Ok(Vec::new());
    }

    let now = OffsetDateTime::now_utc();
    let cached_members = mova_db::list_media_item_cast_members(pool, media_item.id)
        .await
        .map_err(ApplicationError::from)?;
    let cache_entry = mova_db::get_media_item_cast_cache(pool, media_item.id)
        .await
        .map_err(ApplicationError::from)?;

    if let Some(cache_entry) = cache_entry.as_ref() {
        if cache_entry.expires_at > now {
            return Ok(cached_members);
        }
    }

    if !metadata_provider.is_enabled() {
        return Ok(cached_members);
    }

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

            if cache_entry.is_none() {
                persist_cast_members(
                    pool,
                    media_item.id,
                    &[],
                    now,
                    MEDIA_CAST_FAILURE_CACHE_TTL_SECONDS,
                )
                .await?;
            }

            return Ok(cached_members);
        }
    };

    if let Some(remote_cast) = remote_cast {
        let cast_members = normalize_remote_cast(media_item.id, remote_cast);
        persist_cast_members(
            pool,
            media_item.id,
            &cast_members,
            now,
            MEDIA_CAST_CACHE_TTL_SECONDS,
        )
        .await?;
        return Ok(cast_members);
    }

    if cache_entry.is_none() {
        persist_cast_members(
            pool,
            media_item.id,
            &[],
            now,
            MEDIA_CAST_FAILURE_CACHE_TTL_SECONDS,
        )
        .await?;
    }

    Ok(cached_members)
}

pub async fn invalidate_media_item_cast_cache(
    pool: &PgPool,
    media_item_id: i64,
) -> ApplicationResult<()> {
    mova_db::delete_media_item_cast_cache(pool, media_item_id)
        .await
        .map_err(ApplicationError::from)
}

async fn persist_cast_members(
    pool: &PgPool,
    media_item_id: i64,
    members: &[MediaCastMember],
    fetched_at: OffsetDateTime,
    ttl_seconds: i64,
) -> ApplicationResult<()> {
    let expires_at = fetched_at
        .checked_add(Duration::seconds(ttl_seconds))
        .ok_or_else(|| {
            ApplicationError::Unexpected(anyhow::anyhow!(
                "failed to calculate cast cache expiration for media item {}",
                media_item_id
            ))
        })?;

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
            expires_at,
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
