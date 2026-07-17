use crate::{ApplicationError, ApplicationResult};
use mova_domain::{NotificationFeed, UserProfile};
use sqlx::PgPool;

pub async fn list_notifications(
    pool: &PgPool,
    user: &UserProfile,
    category: Option<&str>,
    limit: Option<i64>,
) -> ApplicationResult<NotificationFeed> {
    let category = normalize_notification_category(category)?;
    mova_db::list_notifications(
        pool,
        user.user.id,
        user.is_admin(),
        &user.library_ids,
        category.as_deref(),
        limit.unwrap_or(20).clamp(1, 50),
    )
    .await
    .map_err(ApplicationError::from)
}

pub async fn mark_notification_read(
    pool: &PgPool,
    user: &UserProfile,
    notification_id: i64,
) -> ApplicationResult<()> {
    let marked = mova_db::mark_notification_read(
        pool,
        notification_id,
        user.user.id,
        user.is_admin(),
        &user.library_ids,
    )
    .await
    .map_err(ApplicationError::from)?;

    if !marked {
        return Err(ApplicationError::NotFound(format!(
            "notification {notification_id} not found"
        )));
    }
    Ok(())
}

pub async fn mark_all_notifications_read(
    pool: &PgPool,
    user: &UserProfile,
    category: Option<&str>,
) -> ApplicationResult<u64> {
    let category = normalize_notification_category(category)?;
    mova_db::mark_all_notifications_read(
        pool,
        user.user.id,
        user.is_admin(),
        &user.library_ids,
        category.as_deref(),
    )
    .await
    .map_err(ApplicationError::from)
}

fn normalize_notification_category(category: Option<&str>) -> ApplicationResult<Option<String>> {
    let Some(category) = category else {
        return Ok(None);
    };
    let normalized = category.trim().to_ascii_lowercase();
    if normalized.is_empty()
        || normalized.len() > 32
        || !normalized
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err(ApplicationError::Validation(
            "notification category is invalid".to_string(),
        ));
    }
    Ok(Some(normalized))
}

#[cfg(test)]
mod tests {
    #[test]
    fn notification_category_is_normalized_and_validated() {
        assert_eq!(
            super::normalize_notification_category(Some(" Scan ")).unwrap(),
            Some("scan".to_string())
        );
        assert!(super::normalize_notification_category(Some("scan/all")).is_err());
    }
}
