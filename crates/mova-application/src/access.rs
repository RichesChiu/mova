use crate::{error::ApplicationError, ApplicationResult};
use mova_domain::{Library, MediaFile, MediaItem, Season, UserProfile};
use sqlx::PgPool;

/// 在 SQL 查询阶段应用角色优先、媒体库授权其次的可见范围。
pub async fn authorize_library(
    pool: &PgPool,
    user: &UserProfile,
    library_id: i64,
) -> ApplicationResult<Library> {
    let visibility = user.library_visibility();
    match mova_db::get_library_with_visibility(
        pool,
        library_id,
        visibility.restricted_library_ids(),
    )
    .await
    .map_err(ApplicationError::from)?
    {
        mova_db::VisibilityResult::Visible(library) => Ok(library),
        mova_db::VisibilityResult::Forbidden { library_id } => {
            Err(library_forbidden(user, library_id))
        }
        mova_db::VisibilityResult::Missing => Err(ApplicationError::NotFound(format!(
            "library not found: {library_id}"
        ))),
    }
}

/// 一次数据库查询完成媒体条目、所属媒体库和访问权限解析。
pub async fn authorize_media_item_with_library(
    pool: &PgPool,
    user: &UserProfile,
    media_item_id: i64,
) -> ApplicationResult<(MediaItem, Library)> {
    let visibility = user.library_visibility();
    match mova_db::get_media_item_with_library_visibility(
        pool,
        media_item_id,
        visibility.restricted_library_ids(),
    )
    .await
    .map_err(ApplicationError::from)?
    {
        mova_db::VisibilityResult::Visible(resource) => Ok(resource),
        mova_db::VisibilityResult::Forbidden { library_id } => {
            Err(library_forbidden(user, library_id))
        }
        mova_db::VisibilityResult::Missing => Err(ApplicationError::NotFound(format!(
            "media item not found: {media_item_id}"
        ))),
    }
}

/// 一次数据库查询完成媒体文件、所属媒体库和访问权限解析。
pub async fn authorize_media_file_with_library(
    pool: &PgPool,
    user: &UserProfile,
    media_file_id: i64,
) -> ApplicationResult<(MediaFile, Library)> {
    let visibility = user.library_visibility();
    match mova_db::get_media_file_with_library_visibility(
        pool,
        media_file_id,
        visibility.restricted_library_ids(),
    )
    .await
    .map_err(ApplicationError::from)?
    {
        mova_db::VisibilityResult::Visible(resource) => Ok(resource),
        mova_db::VisibilityResult::Forbidden { library_id } => {
            Err(library_forbidden(user, library_id))
        }
        mova_db::VisibilityResult::Missing => Err(ApplicationError::NotFound(format!(
            "media file not found: {media_file_id}"
        ))),
    }
}

/// 一次数据库查询完成季、所属媒体库和访问权限解析。
pub async fn authorize_season_with_library(
    pool: &PgPool,
    user: &UserProfile,
    season_id: i64,
) -> ApplicationResult<(Season, Library)> {
    let visibility = user.library_visibility();
    match mova_db::get_season_with_library_visibility(
        pool,
        season_id,
        visibility.restricted_library_ids(),
    )
    .await
    .map_err(ApplicationError::from)?
    {
        mova_db::VisibilityResult::Visible(resource) => Ok(resource),
        mova_db::VisibilityResult::Forbidden { library_id } => {
            Err(library_forbidden(user, library_id))
        }
        mova_db::VisibilityResult::Missing => Err(ApplicationError::NotFound(format!(
            "season not found: {season_id}"
        ))),
    }
}

fn library_forbidden(user: &UserProfile, library_id: i64) -> ApplicationError {
    ApplicationError::Forbidden(format!(
        "user {} cannot access library {}",
        user.user.username, library_id
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        authorize_library, authorize_media_file_with_library, authorize_media_item_with_library,
        authorize_season_with_library,
    };
    use crate::ApplicationError;
    use mova_domain::{User, UserProfile, UserRole};
    use time::OffsetDateTime;

    fn profile(role: UserRole, library_ids: Vec<i64>) -> UserProfile {
        UserProfile {
            user: User {
                id: 42,
                username: "viewer".to_string(),
                nickname: "Viewer".to_string(),
                role,
                is_enabled: true,
                created_at: OffsetDateTime::UNIX_EPOCH,
                updated_at: OffsetDateTime::UNIX_EPOCH,
            },
            library_ids,
        }
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn resource_authorization_is_applied_by_the_joined_sql_query(pool: sqlx::PgPool) {
        let allowed_library_id = sqlx::query_scalar::<_, i64>(
            "insert into libraries (name, root_path) values ('Allowed', '/allowed') returning id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let denied_library_id = sqlx::query_scalar::<_, i64>(
            "insert into libraries (name, root_path) values ('Denied', '/denied') returning id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let series_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into media_items (library_id, media_type, title, source_title)
            values ($1, 'series', 'Series', 'Series')
            returning id
            "#,
        )
        .bind(allowed_library_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        let season_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into seasons (library_id, series_id, season_number)
            values ($1, $2, 1)
            returning id
            "#,
        )
        .bind(allowed_library_id)
        .bind(series_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        let file_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into media_files (library_id, media_item_id, file_path, file_size)
            values ($1, $2, '/allowed/series.mkv', 1)
            returning id
            "#,
        )
        .bind(allowed_library_id)
        .bind(series_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        let denied_item_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into media_items (library_id, media_type, title, source_title)
            values ($1, 'movie', 'Denied movie', 'Denied movie')
            returning id
            "#,
        )
        .bind(denied_library_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        let viewer = profile(UserRole::Viewer, vec![allowed_library_id]);
        let admin = profile(UserRole::Admin, Vec::new());

        assert_eq!(
            authorize_library(&pool, &viewer, allowed_library_id)
                .await
                .unwrap()
                .id,
            allowed_library_id
        );
        assert_eq!(
            authorize_media_item_with_library(&pool, &viewer, series_id)
                .await
                .unwrap()
                .1
                .id,
            allowed_library_id
        );
        assert_eq!(
            authorize_media_file_with_library(&pool, &viewer, file_id)
                .await
                .unwrap()
                .1
                .id,
            allowed_library_id
        );
        assert_eq!(
            authorize_season_with_library(&pool, &viewer, season_id)
                .await
                .unwrap()
                .1
                .id,
            allowed_library_id
        );
        assert!(matches!(
            authorize_media_item_with_library(&pool, &viewer, denied_item_id).await,
            Err(ApplicationError::Forbidden(_))
        ));
        assert_eq!(
            authorize_media_item_with_library(&pool, &admin, denied_item_id)
                .await
                .unwrap()
                .1
                .id,
            denied_library_id
        );
    }
}
