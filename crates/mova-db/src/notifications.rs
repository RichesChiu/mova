use anyhow::{Context, Result};
use mova_domain::{Notification, NotificationFeed};
use sqlx::{postgres::PgPool, Row};
use std::collections::BTreeMap;
use time::OffsetDateTime;

pub async fn list_notifications(
    pool: &PgPool,
    user_id: i64,
    is_admin: bool,
    visible_library_ids: &[i64],
    category: Option<&str>,
    unread_only: bool,
    limit: i64,
) -> Result<NotificationFeed> {
    let rows = sqlx::query(
        r#"
        select
            n.id,
            n.category,
            n.notification_type,
            n.severity,
            n.library_id,
            n.payload,
            n.created_at,
            nr.read_at
        from notifications n
        left join notification_reads nr
          on nr.notification_id = n.id
         and nr.user_id = $1
        where (
            n.audience = 'server'
            or (n.audience = 'admin' and $2)
            or (
                n.audience = 'library'
                and ($2 or n.library_id = any($3))
            )
            or (n.audience = 'user' and n.user_id = $1)
        )
          and ($4::text is null or n.category = $4)
          and (not $5::boolean or nr.notification_id is null)
        order by n.created_at desc, n.id desc
        limit $6
        "#,
    )
    .bind(user_id)
    .bind(is_admin)
    .bind(visible_library_ids)
    .bind(category)
    .bind(unread_only)
    .bind(limit.clamp(1, 50))
    .fetch_all(pool)
    .await
    .context("failed to list notifications")?;

    let count_rows = sqlx::query(
        r#"
        select n.category, count(*)::bigint as unread_count
        from notifications n
        left join notification_reads nr
          on nr.notification_id = n.id
         and nr.user_id = $1
        where nr.notification_id is null
          and (
              n.audience = 'server'
              or (n.audience = 'admin' and $2)
              or (
                  n.audience = 'library'
                  and ($2 or n.library_id = any($3))
              )
              or (n.audience = 'user' and n.user_id = $1)
          )
        group by n.category
        order by n.category
        "#,
    )
    .bind(user_id)
    .bind(is_admin)
    .bind(visible_library_ids)
    .fetch_all(pool)
    .await
    .context("failed to count unread notifications")?;

    let unread_by_category = count_rows
        .into_iter()
        .map(|row| (row.get("category"), row.get("unread_count")))
        .collect::<BTreeMap<String, i64>>();
    let total_unread = unread_by_category.values().sum();

    Ok(NotificationFeed {
        items: rows
            .into_iter()
            .map(|row| {
                let read_at: Option<OffsetDateTime> = row.get("read_at");
                Notification {
                    id: row.get("id"),
                    category: row.get("category"),
                    notification_type: row.get("notification_type"),
                    severity: row.get("severity"),
                    library_id: row.get("library_id"),
                    payload: row.get("payload"),
                    is_read: read_at.is_some(),
                    read_at,
                    created_at: row.get("created_at"),
                }
            })
            .collect(),
        total_unread,
        unread_by_category,
    })
}

pub async fn mark_notification_read(
    pool: &PgPool,
    notification_id: i64,
    user_id: i64,
    is_admin: bool,
    visible_library_ids: &[i64],
) -> Result<bool> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to start notification read transaction")?;
    let result = sqlx::query(
        r#"
        with visible as (
            select n.id
            from notifications n
            where n.id = $1
              and (
                  n.audience = 'server'
                  or (n.audience = 'admin' and $3)
                  or (
                      n.audience = 'library'
                      and ($3 or n.library_id = any($4))
                  )
                  or (n.audience = 'user' and n.user_id = $2)
              )
        ), inserted as (
            insert into notification_reads (notification_id, user_id, read_at)
            select id, $2, now()
            from visible
            on conflict (notification_id, user_id) do nothing
            returning notification_id
        )
        select
            exists(select 1 from visible) as is_visible,
            exists(select 1 from inserted) as was_inserted
        "#,
    )
    .bind(notification_id)
    .bind(user_id)
    .bind(is_admin)
    .bind(visible_library_ids)
    .fetch_one(&mut *tx)
    .await
    .context("failed to mark notification as read")?;
    let is_visible = result.get("is_visible");
    let was_inserted = result.get("was_inserted");

    if was_inserted {
        bump_user_notification_revision(&mut tx, user_id).await?;
    }
    tx.commit()
        .await
        .context("failed to commit notification read transaction")?;
    Ok(is_visible)
}

pub async fn mark_all_notifications_read(
    pool: &PgPool,
    user_id: i64,
    is_admin: bool,
    visible_library_ids: &[i64],
    category: Option<&str>,
) -> Result<u64> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to start mark all notifications transaction")?;
    let result = sqlx::query(
        r#"
        insert into notification_reads (notification_id, user_id, read_at)
        select n.id, $1, now()
        from notifications n
        where (
            n.audience = 'server'
            or (n.audience = 'admin' and $2)
            or (
                n.audience = 'library'
                and ($2 or n.library_id = any($3))
            )
            or (n.audience = 'user' and n.user_id = $1)
        )
          and ($4::text is null or n.category = $4)
        on conflict (notification_id, user_id) do nothing
        "#,
    )
    .bind(user_id)
    .bind(is_admin)
    .bind(visible_library_ids)
    .bind(category)
    .execute(&mut *tx)
    .await
    .context("failed to mark all notifications as read")?;

    if result.rows_affected() > 0 {
        bump_user_notification_revision(&mut tx, user_id).await?;
    }
    tx.commit()
        .await
        .context("failed to commit mark all notifications transaction")?;
    Ok(result.rows_affected())
}

async fn bump_user_notification_revision(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: i64,
) -> Result<()> {
    sqlx::query("select mova_bump_realtime_revision($1)")
        .bind(format!("user:{user_id}:notifications"))
        .fetch_one(&mut **tx)
        .await
        .context("failed to bump user notification revision")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::list_notifications;

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn list_notifications_filters_read_items_only_when_requested(
        pool: sqlx::postgres::PgPool,
    ) {
        let user_id: i64 = sqlx::query_scalar(
            r#"
            insert into users (
                username,
                username_normalized,
                nickname,
                password_hash,
                role
            )
            values ('viewer', 'viewer', 'Viewer', 'test-password-hash', 'viewer')
            returning id
            "#,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let read_notification_id: i64 = sqlx::query_scalar(
            r#"
            insert into notifications (
                category,
                notification_type,
                severity,
                audience,
                source_key
            )
            values ('system', 'system.read', 'info', 'server', 'system:read')
            returning id
            "#,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let unread_notification_id: i64 = sqlx::query_scalar(
            r#"
            insert into notifications (
                category,
                notification_type,
                severity,
                audience,
                source_key
            )
            values ('system', 'system.unread', 'info', 'server', 'system:unread')
            returning id
            "#,
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        sqlx::query("insert into notification_reads (notification_id, user_id) values ($1, $2)")
            .bind(read_notification_id)
            .bind(user_id)
            .execute(&pool)
            .await
            .unwrap();

        let complete_feed = list_notifications(&pool, user_id, false, &[], None, false, 20)
            .await
            .unwrap();
        let unread_feed = list_notifications(&pool, user_id, false, &[], None, true, 20)
            .await
            .unwrap();

        assert_eq!(complete_feed.items.len(), 2);
        assert!(complete_feed.items.iter().any(|item| item.is_read));
        assert_eq!(unread_feed.items.len(), 1);
        assert_eq!(unread_feed.items[0].id, unread_notification_id);
        assert!(!unread_feed.items[0].is_read);
        assert_eq!(unread_feed.total_unread, 1);
        assert_eq!(unread_feed.unread_by_category.get("system"), Some(&1));
    }
}
