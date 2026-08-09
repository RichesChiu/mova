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

#[sqlx::test(migrations = false)]
#[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
async fn nfo_metadata_migration_upgrades_0001_data_and_enforces_source_ownership(pool: PgPool) {
    sqlx::raw_sql(include_str!("../../../migrations/0001_init.sql"))
        .execute(&pool)
        .await
        .unwrap();

    let library_id = seed_library(&pool, "Upgrade NFO", "/media/upgrade-nfo").await;
    let media_item_id = seed_media_item(&pool, library_id, "movie", "Upgrade Movie").await;
    sqlx::query(
        r#"
        insert into media_item_external_ids (media_item_id, provider, external_id)
        values ($1, 'tmdb', '42')
        "#,
    )
    .bind(media_item_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"
        insert into media_item_ratings (
            media_item_id, source, kind, score, scale, rating_count,
            retrieved_via, fetched_at
        )
        values ($1, 'tmdb', 'audience', 8.25, 10, 100, 'tmdb', now())
        "#,
    )
    .bind(media_item_id)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::raw_sql(include_str!("../../../migrations/0002_nfo_metadata.sql"))
        .execute(&pool)
        .await
        .unwrap();

    let upgraded_external_id = sqlx::query_as::<_, (String, String, Option<i64>)>(
        r#"
        select external_id, retrieved_via, local_metadata_source_id
        from media_item_external_ids
        where media_item_id = $1 and provider = 'tmdb'
        "#,
    )
    .bind(media_item_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        upgraded_external_id,
        ("42".to_string(), "tmdb".to_string(), None)
    );
    let upgraded_rating = sqlx::query_as::<_, (f64, String, Option<i64>)>(
        r#"
        select score::double precision, retrieved_via, local_metadata_source_id
        from media_item_ratings
        where media_item_id = $1 and source = 'tmdb' and kind = 'audience'
        "#,
    )
    .bind(media_item_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(upgraded_rating, (8.25, "tmdb".to_string(), None));

    let source_id = sqlx::query_scalar::<_, i64>(
        r#"
        insert into media_local_metadata_sources (
            library_id, media_item_id, source_path, document_type, is_selected, payload
        )
        values ($1, $2, '/media/upgrade-nfo/movie.nfo', 'movie', true, '{}')
        returning id
        "#,
    )
    .bind(library_id)
    .bind(media_item_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let generated_target_type = sqlx::query_scalar::<_, String>(
        "select target_media_type from media_local_metadata_sources where id = $1",
    )
    .bind(source_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(generated_target_type, "movie");

    sqlx::query(
        r#"
        insert into media_item_external_ids (
            media_item_id, provider, external_id, retrieved_via, local_metadata_source_id
        )
        values ($1, 'tmdb', 'nfo-42', 'nfo', $2)
        "#,
    )
    .bind(media_item_id)
    .bind(source_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"
        insert into media_item_ratings (
            media_item_id, source, kind, score, scale, retrieved_via,
            local_metadata_source_id, fetched_at
        )
        values ($1, 'tmdb', 'audience', 9, 10, 'nfo', $2, now())
        "#,
    )
    .bind(media_item_id)
    .bind(source_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"
        insert into media_item_credits (
            media_item_id, local_metadata_source_id, credit_type,
            retrieved_via, sort_order, name
        )
        values ($1, $2, 'actor', 'nfo', 0, 'Upgrade Actor')
        "#,
    )
    .bind(media_item_id)
    .bind(source_id)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query("delete from media_local_metadata_sources where id = $1")
        .bind(source_id)
        .execute(&pool)
        .await
        .unwrap();
    let remaining = sqlx::query_as::<_, (i64, i64, i64)>(
        r#"
        select
            (select count(*) from media_item_external_ids where media_item_id = $1),
            (select count(*) from media_item_ratings where media_item_id = $1),
            (select count(*) from media_item_credits where media_item_id = $1)
        "#,
    )
    .bind(media_item_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(remaining, (1, 1, 0));
}

#[sqlx::test(migrations = false)]
#[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
async fn strm_migration_upgrades_0003_rows_and_enforces_source_shape(pool: PgPool) {
    for migration in [
        include_str!("../../../migrations/0001_init.sql"),
        include_str!("../../../migrations/0002_nfo_metadata.sql"),
        include_str!("../../../migrations/0003_intro_detection.sql"),
    ] {
        sqlx::raw_sql(migration).execute(&pool).await.unwrap();
    }

    let library_id = seed_library(&pool, "STRM Upgrade", "/media/strm-upgrade").await;
    let media_item_id = seed_media_item(&pool, library_id, "movie", "Upgrade Movie").await;
    let old_media_file_id = seed_media_file(
        &pool,
        library_id,
        media_item_id,
        "/media/strm-upgrade/movie.mkv",
    )
    .await;

    sqlx::raw_sql(include_str!("../../../migrations/0004_strm_sources.sql"))
        .execute(&pool)
        .await
        .unwrap();

    let upgraded = sqlx::query_as::<_, (String, Option<String>)>(
        "select source_kind, stream_reference_hash from media_files where id = $1",
    )
    .bind(old_media_file_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(upgraded, ("local_file".to_string(), None));

    let reference_hash = "a".repeat(64);
    sqlx::query(
        r#"
        insert into media_files (
            library_id, media_item_id, file_path, source_kind,
            stream_reference_hash, file_size
        )
        values ($1, $2, '/media/strm-upgrade/movie.strm', 'strm', $3, 64)
        "#,
    )
    .bind(library_id)
    .bind(media_item_id)
    .bind(&reference_hash)
    .execute(&pool)
    .await
    .unwrap();

    for (path, source_kind, hash) in [
        (
            "/media/strm-upgrade/local-with-hash.mkv",
            "local_file",
            Some(reference_hash.clone()),
        ),
        ("/media/strm-upgrade/strm-without-hash.strm", "strm", None),
        (
            "/media/strm-upgrade/short-hash.strm",
            "strm",
            Some("a".repeat(63)),
        ),
        (
            "/media/strm-upgrade/uppercase-hash.strm",
            "strm",
            Some("A".repeat(64)),
        ),
        (
            "/media/strm-upgrade/non-hex-hash.strm",
            "strm",
            Some("g".repeat(64)),
        ),
        (
            "/media/strm-upgrade/unknown-source.strm",
            "remote_url",
            Some(reference_hash.clone()),
        ),
    ] {
        let error = sqlx::query(
            r#"
            insert into media_files (
                library_id, media_item_id, file_path, source_kind,
                stream_reference_hash, file_size
            )
            values ($1, $2, $3, $4, $5, 1)
            "#,
        )
        .bind(library_id)
        .bind(media_item_id)
        .bind(path)
        .bind(source_kind)
        .bind(hash)
        .execute(&pool)
        .await
        .unwrap_err();
        assert_sqlstate(error, "23514");
    }

    sqlx::query("delete from libraries where id = $1")
        .bind(library_id)
        .execute(&pool)
        .await
        .unwrap();
    let remaining =
        sqlx::query_scalar::<_, i64>("select count(*) from media_files where library_id = $1")
            .bind(library_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(remaining, 0);
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
async fn local_metadata_sources_allow_versions_but_enforce_one_selected_projection(pool: PgPool) {
    let library_id = seed_library(&pool, "NFO", "/media/nfo").await;
    let media_item_id = seed_media_item(&pool, library_id, "movie", "Movie").await;

    sqlx::query(
        r#"
        insert into media_local_metadata_sources (
            library_id, media_item_id, source_path, document_type, is_selected, payload
        )
        values
            ($1, $2, '/media/nfo/version-a.nfo', 'movie', true, '{"title":"A"}'),
            ($1, $2, '/media/nfo/version-b.nfo', 'movie', false, '{"title":"B"}')
        "#,
    )
    .bind(library_id)
    .bind(media_item_id)
    .execute(&pool)
    .await
    .unwrap();

    let selected_source_id = sqlx::query_scalar::<_, i64>(
        r#"
        select id
        from media_local_metadata_sources
        where media_item_id = $1 and is_selected
        "#,
    )
    .bind(media_item_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    let duplicate_selected = sqlx::query(
        r#"
        insert into media_local_metadata_sources (
            library_id, media_item_id, source_path, document_type, is_selected
        )
        values ($1, $2, '/media/nfo/version-c.nfo', 'movie', true)
        "#,
    )
    .bind(library_id)
    .bind(media_item_id)
    .execute(&pool)
    .await
    .unwrap_err();
    assert_sqlstate(duplicate_selected, "23505");

    let invalid_payload = sqlx::query(
        r#"
        insert into media_local_metadata_sources (
            library_id, media_item_id, source_path, document_type, payload
        )
        values ($1, $2, '/media/nfo/invalid.nfo', 'movie', '[]')
        "#,
    )
    .bind(library_id)
    .bind(media_item_id)
    .execute(&pool)
    .await
    .unwrap_err();
    assert_sqlstate(invalid_payload, "23514");

    sqlx::query(
        r#"
        insert into media_item_external_ids (
            media_item_id,
            provider,
            external_id,
            retrieved_via,
            local_metadata_source_id
        )
        values
            ($1, 'tmdb', 'remote-42', 'tmdb', null),
            ($1, 'tmdb', 'local-42', 'nfo', $2)
        "#,
    )
    .bind(media_item_id)
    .bind(selected_source_id)
    .execute(&pool)
    .await
    .unwrap();

    let external_id_count = sqlx::query_scalar::<_, i64>(
        "select count(*) from media_item_external_ids where media_item_id = $1",
    )
    .bind(media_item_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(external_id_count, 2);
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
async fn local_metadata_sources_prevent_incompatible_parent_media_type_updates(pool: PgPool) {
    let library_id = seed_library(&pool, "NFO Parent Types", "/media/nfo-parent-types").await;
    let cases = [
        ("movie", "movie", "series"),
        ("series", "tvshow", "episode"),
        ("episode", "episodedetails", "movie"),
    ];

    for (index, (media_type, document_type, incompatible_type)) in cases.into_iter().enumerate() {
        let title = format!("NFO Parent Type {index}");
        let media_item_id = seed_media_item(&pool, library_id, media_type, title.as_str()).await;
        let incompatible_document_type = if document_type == "movie" {
            "tvshow"
        } else {
            "movie"
        };
        let error = sqlx::query(
            r#"
            insert into media_local_metadata_sources (
                library_id, media_item_id, source_path, document_type, payload
            )
            values ($1, $2, $3, $4, '{}')
            "#,
        )
        .bind(library_id)
        .bind(media_item_id)
        .bind(format!("/media/nfo-parent-types/{index}-invalid.nfo"))
        .bind(incompatible_document_type)
        .execute(&pool)
        .await
        .unwrap_err();
        assert_sqlstate(error, "23503");

        sqlx::query(
            r#"
            insert into media_local_metadata_sources (
                library_id, media_item_id, source_path, document_type, payload
            )
            values ($1, $2, $3, $4, '{}')
            "#,
        )
        .bind(library_id)
        .bind(media_item_id)
        .bind(format!("/media/nfo-parent-types/{index}.nfo"))
        .bind(document_type)
        .execute(&pool)
        .await
        .unwrap();

        let generated_target_type = sqlx::query_scalar::<_, String>(
            "select target_media_type from media_local_metadata_sources where media_item_id = $1",
        )
        .bind(media_item_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(generated_target_type, media_type);

        let retained_type = sqlx::query_scalar::<_, String>(
            r#"
            update media_items
            set media_type = $2,
                title = title || ' updated'
            where id = $1
            returning media_type
            "#,
        )
        .bind(media_item_id)
        .bind(media_type)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(retained_type, media_type);

        let error = sqlx::query("update media_items set media_type = $2 where id = $1")
            .bind(media_item_id)
            .bind(incompatible_type)
            .execute(&pool)
            .await
            .unwrap_err();
        assert_sqlstate(error, "23503");

        let retained_type =
            sqlx::query_scalar::<_, String>("select media_type from media_items where id = $1")
                .bind(media_item_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(retained_type, media_type);
    }
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
async fn local_metadata_target_type_fk_serializes_concurrent_parent_updates(pool: PgPool) {
    use std::time::Duration;
    use tokio::{sync::oneshot, time::timeout};

    let library_id = seed_library(&pool, "Concurrent NFO", "/media/concurrent-nfo").await;
    let media_item_id = seed_media_item(&pool, library_id, "movie", "Concurrent Movie").await;
    let mut source_tx = pool.begin().await.unwrap();
    sqlx::query(
        r#"
        insert into media_local_metadata_sources (
            library_id, media_item_id, source_path, document_type, payload
        )
        values ($1, $2, '/media/concurrent-nfo/movie.nfo', 'movie', '{}')
        "#,
    )
    .bind(library_id)
    .bind(media_item_id)
    .execute(&mut *source_tx)
    .await
    .unwrap();

    let update_pool = pool.clone();
    let (started_tx, started_rx) = oneshot::channel();
    let mut update_task = tokio::spawn(async move {
        let mut tx = update_pool.begin().await.unwrap();
        let _ = started_tx.send(());
        let result = sqlx::query("update media_items set media_type = 'series' where id = $1")
            .bind(media_item_id)
            .execute(&mut *tx)
            .await;
        if result.is_ok() {
            tx.commit().await.unwrap();
        }
        result
    });
    started_rx.await.unwrap();

    assert!(
        timeout(Duration::from_millis(100), &mut update_task)
            .await
            .is_err(),
        "parent type update must wait for the concurrent source insert"
    );
    source_tx.commit().await.unwrap();

    let error = update_task.await.unwrap().unwrap_err();
    assert_sqlstate(error, "23503");
    let retained_type =
        sqlx::query_scalar::<_, String>("select media_type from media_items where id = $1")
            .bind(media_item_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(retained_type, "movie");
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
async fn deleting_local_metadata_source_cascades_only_its_nfo_projection(pool: PgPool) {
    let library_id = seed_library(&pool, "NFO Ownership", "/media/nfo-ownership").await;
    let media_item_id = seed_media_item(&pool, library_id, "movie", "Owned Movie").await;
    let source_id = sqlx::query_scalar::<_, i64>(
        r#"
        insert into media_local_metadata_sources (
            library_id, media_item_id, source_path, document_type, is_selected, payload
        )
        values ($1, $2, '/media/nfo-ownership/movie.nfo', 'movie', true, '{}')
        returning id
        "#,
    )
    .bind(library_id)
    .bind(media_item_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    let missing_owner = sqlx::query(
        r#"
        insert into media_item_external_ids (
            media_item_id, provider, external_id, retrieved_via
        )
        values ($1, 'imdb', 'tt-missing-owner', 'nfo')
        "#,
    )
    .bind(media_item_id)
    .execute(&pool)
    .await
    .unwrap_err();
    assert_sqlstate(missing_owner, "23514");

    let other_item_id = seed_media_item(&pool, library_id, "movie", "Other Movie").await;
    let other_source_id = sqlx::query_scalar::<_, i64>(
        r#"
        insert into media_local_metadata_sources (
            library_id, media_item_id, source_path, document_type, payload
        )
        values ($1, $2, '/media/nfo-ownership/other.nfo', 'movie', '{}')
        returning id
        "#,
    )
    .bind(library_id)
    .bind(other_item_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let wrong_owner = sqlx::query(
        r#"
        insert into media_item_external_ids (
            media_item_id, provider, external_id, retrieved_via, local_metadata_source_id
        )
        values ($1, 'tvdb', 'wrong-owner', 'nfo', $2)
        "#,
    )
    .bind(media_item_id)
    .bind(other_source_id)
    .execute(&pool)
    .await
    .unwrap_err();
    assert_sqlstate(wrong_owner, "23503");

    sqlx::query(
        r#"
        insert into media_item_external_ids (
            media_item_id, provider, external_id, retrieved_via, local_metadata_source_id
        )
        values
            ($1, 'tmdb', 'remote-id', 'tmdb', null),
            ($1, 'tmdb', 'nfo-id', 'nfo', $2)
        "#,
    )
    .bind(media_item_id)
    .bind(source_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"
        insert into media_item_ratings (
            media_item_id, source, kind, score, scale, retrieved_via,
            local_metadata_source_id, fetched_at
        )
        values
            ($1, 'tmdb', 'audience', 8, 10, 'tmdb', null, now()),
            ($1, 'tmdb', 'audience', 9, 10, 'nfo', $2, now())
        "#,
    )
    .bind(media_item_id)
    .bind(source_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"
        insert into media_item_credits (
            media_item_id, local_metadata_source_id, credit_type, retrieved_via,
            sort_order, person_provider, provider_person_id, name, role
        )
        values
            ($1, $2, 'actor', 'nfo', 0, 'tmdb', '1', 'Same Name', 'Lead'),
            ($1, $2, 'actor', 'nfo', 0, 'tmdb', '2', 'Same Name', 'Double')
        "#,
    )
    .bind(media_item_id)
    .bind(source_id)
    .execute(&pool)
    .await
    .unwrap();
    let credit_count = sqlx::query_scalar::<_, i64>(
        "select count(*) from media_item_credits where media_item_id = $1",
    )
    .bind(media_item_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(credit_count, 2, "same-name credits must remain lossless");

    sqlx::query("delete from media_local_metadata_sources where id = $1")
        .bind(source_id)
        .execute(&pool)
        .await
        .unwrap();

    let remaining_external_ids = sqlx::query_scalar::<_, i64>(
        "select count(*) from media_item_external_ids where media_item_id = $1",
    )
    .bind(media_item_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let remaining_ratings = sqlx::query_scalar::<_, i64>(
        "select count(*) from media_item_ratings where media_item_id = $1",
    )
    .bind(media_item_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let remaining_credits = sqlx::query_scalar::<_, i64>(
        "select count(*) from media_item_credits where media_item_id = $1",
    )
    .bind(media_item_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(remaining_external_ids, 1);
    assert_eq!(remaining_ratings, 1);
    assert_eq!(remaining_credits, 0);
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
            user_id, media_item_id, last_played_media_item_id
        )
        values ($1, $2, $2)
        "#,
    )
    .bind(user_a)
    .bind(item_a)
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
    assert_eq!(continue_count, 1);
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
