use sqlx::{PgPool, Row};

fn assert_sqlstate(error: sqlx::Error, expected: &str) {
    let actual = error
        .as_database_error()
        .and_then(|error| error.code())
        .map(|code| code.into_owned());
    assert_eq!(actual.as_deref(), Some(expected), "{error}");
}

async fn seed_library(pool: &PgPool, name: &str, root_path: &str) -> i64 {
    sqlx::query_scalar(
        r#"
        insert into libraries (name, root_path)
        values ($1, $2)
        returning id
        "#,
    )
    .bind(name)
    .bind(root_path)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn seed_media_item(pool: &PgPool, library_id: i64, media_type: &str, title: &str) -> i64 {
    sqlx::query_scalar(
        r#"
        insert into media_items (library_id, media_type, title, source_title)
        values ($1, $2, $3, $3)
        returning id
        "#,
    )
    .bind(library_id)
    .bind(media_type)
    .bind(title)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn seed_user(pool: &PgPool, username: &str) -> i64 {
    sqlx::query_scalar(
        r#"
        insert into users (
            username,
            username_normalized,
            nickname,
            password_hash,
            role
        )
        values ($1, lower($1), $1, 'hash', 'viewer')
        returning id
        "#,
    )
    .bind(username)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn seed_media_file(
    pool: &PgPool,
    library_id: i64,
    media_item_id: i64,
    file_path: &str,
) -> i64 {
    sqlx::query_scalar(
        r#"
        insert into media_files (library_id, media_item_id, file_path, file_size)
        values ($1, $2, $3, 1)
        returning id
        "#,
    )
    .bind(library_id)
    .bind(media_item_id)
    .bind(file_path)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
async fn media_hierarchy_enforces_parent_types_and_library_ownership(pool: PgPool) {
    let library_a = seed_library(&pool, "Library A", "/media/a").await;
    let library_b = seed_library(&pool, "Library B", "/media/b").await;
    let series_a = seed_media_item(&pool, library_a, "series", "Series A").await;
    let movie_a = seed_media_item(&pool, library_a, "movie", "Movie A").await;
    let episode_a = seed_media_item(&pool, library_a, "episode", "Episode A").await;
    let episode_b = seed_media_item(&pool, library_b, "episode", "Episode B").await;

    let season_a = sqlx::query_scalar::<_, i64>(
        r#"
        insert into seasons (library_id, series_id, season_number, title)
        values ($1, $2, 1, 'Season 1')
        returning id
        "#,
    )
    .bind(library_a)
    .bind(series_a)
    .fetch_one(&pool)
    .await
    .unwrap();

    sqlx::query(
        r#"
        insert into episodes (media_item_id, library_id, season_id, episode_number)
        values ($1, $2, $3, 1)
        "#,
    )
    .bind(episode_a)
    .bind(library_a)
    .bind(season_a)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        r#"
        insert into series_episode_outline_cache (
            series_media_item_id,
            library_id,
            outline_json,
            expires_at
        )
        values ($1, $2, '{}', now() + interval '1 day')
        "#,
    )
    .bind(series_a)
    .bind(library_a)
    .execute(&pool)
    .await
    .unwrap();

    let error = sqlx::query(
        r#"
        insert into series_episode_outline_cache (
            series_media_item_id,
            library_id,
            outline_json,
            expires_at
        )
        values ($1, $2, '{}', now() + interval '1 day')
        "#,
    )
    .bind(movie_a)
    .bind(library_a)
    .execute(&pool)
    .await
    .unwrap_err();
    assert_sqlstate(error, "23503");

    let error = sqlx::query(
        r#"
        insert into seasons (library_id, series_id, season_number)
        values ($1, $2, 2)
        "#,
    )
    .bind(library_a)
    .bind(movie_a)
    .execute(&pool)
    .await
    .unwrap_err();
    assert_sqlstate(error, "23503");

    let error = sqlx::query(
        r#"
        insert into seasons (library_id, series_id, season_number)
        values ($1, $2, 2)
        "#,
    )
    .bind(library_b)
    .bind(series_a)
    .execute(&pool)
    .await
    .unwrap_err();
    assert_sqlstate(error, "23503");

    let error = sqlx::query(
        r#"
        insert into episodes (media_item_id, library_id, season_id, episode_number)
        values ($1, $2, $3, 2)
        "#,
    )
    .bind(movie_a)
    .bind(library_a)
    .bind(season_a)
    .execute(&pool)
    .await
    .unwrap_err();
    assert_sqlstate(error, "23503");

    let error = sqlx::query(
        r#"
        insert into episodes (media_item_id, library_id, season_id, episode_number)
        values ($1, $2, $3, 2)
        "#,
    )
    .bind(episode_b)
    .bind(library_b)
    .bind(season_a)
    .execute(&pool)
    .await
    .unwrap_err();
    assert_sqlstate(error, "23503");

    let error = sqlx::query("update media_items set media_type = 'movie' where id = $1")
        .bind(series_a)
        .execute(&pool)
        .await
        .unwrap_err();
    assert_sqlstate(error, "23503");

    let error = sqlx::query("update media_items set media_type = 'movie' where id = $1")
        .bind(episode_a)
        .execute(&pool)
        .await
        .unwrap_err();
    assert_sqlstate(error, "23503");
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
async fn subtitle_rows_enforce_source_shape_format_and_identity(pool: PgPool) {
    let library_id = seed_library(&pool, "Subtitles", "/media/subtitles").await;
    let media_item_id = seed_media_item(&pool, library_id, "movie", "Movie").await;
    let media_file_id = seed_media_file(
        &pool,
        library_id,
        media_item_id,
        "/media/subtitles/movie.mkv",
    )
    .await;

    sqlx::query(
        r#"
        insert into subtitle_files (
            media_file_id, source_kind, file_path, stream_index, subtitle_format
        )
        values
            ($1, 'external', '/media/subtitles/movie.en.srt', null, 'srt'),
            ($1, 'embedded', null, 2, 'subrip')
        "#,
    )
    .bind(media_file_id)
    .execute(&pool)
    .await
    .unwrap();

    let invalid_rows = [
        ("unknown", None, None, Some("srt")),
        ("external", None, None, Some("srt")),
        ("external", Some(""), None, Some("srt")),
        ("external", Some("/tmp/wrong.srt"), Some(3), Some("srt")),
        ("embedded", Some("/tmp/wrong.srt"), Some(4), Some("srt")),
        ("embedded", None, None, Some("srt")),
        ("embedded", None, Some(-1), Some("srt")),
        ("external", Some("/tmp/null-format.srt"), None, None),
        ("external", Some("/tmp/blank-format.srt"), None, Some("")),
    ];

    for (source_kind, file_path, stream_index, subtitle_format) in invalid_rows {
        let error = sqlx::query(
            r#"
            insert into subtitle_files (
                media_file_id, source_kind, file_path, stream_index, subtitle_format
            )
            values ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(media_file_id)
        .bind(source_kind)
        .bind(file_path)
        .bind(stream_index)
        .bind(subtitle_format)
        .execute(&pool)
        .await
        .unwrap_err();
        assert_sqlstate(
            error,
            if subtitle_format.is_none() {
                "23502"
            } else {
                "23514"
            },
        );
    }

    let duplicate_external = sqlx::query(
        r#"
        insert into subtitle_files (
            media_file_id, source_kind, file_path, stream_index, subtitle_format
        )
        values ($1, 'external', '/media/subtitles/movie.en.srt', null, 'srt')
        "#,
    )
    .bind(media_file_id)
    .execute(&pool)
    .await
    .unwrap_err();
    assert_sqlstate(duplicate_external, "23505");

    let duplicate_embedded = sqlx::query(
        r#"
        insert into subtitle_files (
            media_file_id, source_kind, file_path, stream_index, subtitle_format
        )
        values ($1, 'embedded', null, 2, 'subrip')
        "#,
    )
    .bind(media_file_id)
    .execute(&pool)
    .await
    .unwrap_err();
    assert_sqlstate(duplicate_embedded, "23505");
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
async fn cast_members_belong_to_a_cache_aggregate_and_follow_its_lifecycle(pool: PgPool) {
    let library_id = seed_library(&pool, "Cast", "/media/cast").await;
    let media_item_id = seed_media_item(&pool, library_id, "movie", "Movie").await;

    let error = sqlx::query(
        r#"
        insert into media_item_cast_members (media_item_id, sort_order, name)
        values ($1, 0, 'Actor')
        "#,
    )
    .bind(media_item_id)
    .execute(&pool)
    .await
    .unwrap_err();
    assert_sqlstate(error, "23503");

    sqlx::query(
        r#"
        insert into media_item_cast_cache (media_item_id, expires_at)
        values ($1, now() + interval '1 day')
        "#,
    )
    .bind(media_item_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"
        insert into media_item_cast_members (media_item_id, sort_order, name)
        values ($1, 0, 'Actor')
        "#,
    )
    .bind(media_item_id)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query("delete from media_item_cast_cache where media_item_id = $1")
        .bind(media_item_id)
        .execute(&pool)
        .await
        .unwrap();

    let member_count = sqlx::query_scalar::<_, i64>(
        "select count(*) from media_item_cast_members where media_item_id = $1",
    )
    .bind(media_item_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(member_count, 0);
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
async fn playback_state_enforces_media_file_ownership_and_delete_semantics(pool: PgPool) {
    let library_id = seed_library(&pool, "Playback", "/media/playback").await;
    let item_a = seed_media_item(&pool, library_id, "movie", "Movie A").await;
    let item_b = seed_media_item(&pool, library_id, "movie", "Movie B").await;
    let file_a = seed_media_file(&pool, library_id, item_a, "/media/playback/a.mkv").await;
    let user_a = seed_user(&pool, "viewer-a").await;
    let user_b = seed_user(&pool, "viewer-b").await;

    sqlx::query(
        r#"
        insert into playback_progress (
            user_id, media_item_id, last_media_file_id, position_seconds
        )
        values ($1, $2, $3, 10)
        "#,
    )
    .bind(user_a)
    .bind(item_a)
    .bind(file_a)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        r#"
        insert into continue_watching (
            user_id, media_item_id, last_played_media_item_id, last_media_file_id
        )
        values ($1, $2, $2, $3)
        "#,
    )
    .bind(user_a)
    .bind(item_a)
    .bind(file_a)
    .execute(&pool)
    .await
    .unwrap();

    let error = sqlx::query(
        r#"
        insert into playback_progress (
            user_id, media_item_id, last_media_file_id, position_seconds
        )
        values ($1, $2, $3, 10)
        "#,
    )
    .bind(user_b)
    .bind(item_b)
    .bind(file_a)
    .execute(&pool)
    .await
    .unwrap_err();
    assert_sqlstate(error, "23503");

    let error = sqlx::query(
        r#"
        insert into continue_watching (
            user_id, media_item_id, last_played_media_item_id, last_media_file_id
        )
        values ($1, $2, $2, $3)
        "#,
    )
    .bind(user_b)
    .bind(item_b)
    .bind(file_a)
    .execute(&pool)
    .await
    .unwrap_err();
    assert_sqlstate(error, "23503");

    sqlx::query("delete from media_files where id = $1")
        .bind(file_a)
        .execute(&pool)
        .await
        .unwrap();

    let progress_file_id = sqlx::query_scalar::<_, Option<i64>>(
        "select last_media_file_id from playback_progress where user_id = $1",
    )
    .bind(user_a)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(progress_file_id, None);

    let continue_count =
        sqlx::query_scalar::<_, i64>("select count(*) from continue_watching where user_id = $1")
            .bind(user_a)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(continue_count, 0);
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
async fn scan_group_stage_flags_are_monotonic(pool: PgPool) {
    let library_id = seed_library(&pool, "Scans", "/media/scans").await;
    let scan_job_id = sqlx::query_scalar::<_, i64>(
        r#"
        insert into scan_jobs (library_id, status)
        values ($1, 'pending')
        returning id
        "#,
    )
    .bind(library_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    let error = sqlx::query(
        r#"
        insert into scan_job_groups (
            scan_job_id, group_key, file_count, local_analyzed, local_committed
        )
        values ($1, 'invalid-local', 1, false, true)
        "#,
    )
    .bind(scan_job_id)
    .execute(&pool)
    .await
    .unwrap_err();
    assert_sqlstate(error, "23514");

    let error = sqlx::query(
        r#"
        insert into scan_job_groups (
            scan_job_id, group_key, file_count,
            local_analyzed, local_committed, remote_completed
        )
        values ($1, 'invalid-remote', 1, true, false, true)
        "#,
    )
    .bind(scan_job_id)
    .execute(&pool)
    .await
    .unwrap_err();
    assert_sqlstate(error, "23514");

    sqlx::query(
        r#"
        insert into scan_job_groups (
            scan_job_id, group_key, file_count,
            local_analyzed, local_committed, remote_completed
        )
        values ($1, 'valid', 1, true, true, true)
        "#,
    )
    .bind(scan_job_id)
    .execute(&pool)
    .await
    .unwrap();

    let row = sqlx::query(
        r#"
        select local_analyzed, local_committed, remote_completed
        from scan_job_groups
        where scan_job_id = $1 and group_key = 'valid'
        "#,
    )
    .bind(scan_job_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(row.get::<bool, _>("local_analyzed"));
    assert!(row.get::<bool, _>("local_committed"));
    assert!(row.get::<bool, _>("remote_completed"));
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
async fn scan_job_state_and_counters_reject_impossible_snapshots(pool: PgPool) {
    let library_id = seed_library(&pool, "Scan State", "/media/scan-state").await;

    for statement in [
        r#"
        insert into scan_jobs (library_id, status, phase, finished_at)
        values ($1, 'success', 'finished', null)
        "#,
        r#"
        insert into scan_jobs (
            library_id, status, phase, progress_percent, finished_at
        )
        values ($1, 'success', 'finished', 99, now())
        "#,
        r#"
        insert into scan_jobs (
            library_id, status, phase, progress_percent, finished_at
        )
        values ($1, 'running', 'finished', 99, now())
        "#,
        r#"
        insert into scan_jobs (
            library_id, status, total_files,
            local_analyzed_files, local_committed_files, remote_completed_files
        )
        values ($1, 'pending', 2, 1, 2, 2)
        "#,
        r#"
        insert into scan_jobs (
            library_id, status, total_files,
            local_analyzed_files, local_committed_files, remote_completed_files
        )
        values ($1, 'pending', 2, 2, 1, 2)
        "#,
    ] {
        let error = sqlx::query(statement)
            .bind(library_id)
            .execute(&pool)
            .await
            .unwrap_err();
        assert_sqlstate(error, "23514");
    }

    sqlx::query(
        r#"
        insert into scan_jobs (
            library_id, status, phase, total_files, scanned_files,
            reused_files, local_analyzed_files, local_committed_files,
            remote_completed_files, progress_percent, finished_at
        )
        values ($1, 'success', 'finished', 2, 2, 1, 2, 2, 2, 100, now())
        "#,
    )
    .bind(library_id)
    .execute(&pool)
    .await
    .unwrap();
}
