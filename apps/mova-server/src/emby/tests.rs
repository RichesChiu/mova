use super::{
    auth,
    dto::{self, Kind},
};
use axum::http::{HeaderMap, HeaderValue};
use std::collections::HashMap;

#[test]
fn resource_ids_separate_tables_and_reject_wrong_kinds() {
    let item = dto::external_id(Kind::Item, 42);
    assert_ne!(item, dto::external_id(Kind::Season, 42));
    assert_eq!(dto::require_id(&item, Kind::Item).unwrap(), 42);
    assert!(dto::require_id(&item, Kind::File).is_err());
    assert!(dto::parse_id("../../file").is_err());
    assert!(dto::parse_id(&dto::external_id(Kind::Item, 0)).is_err());
}

#[test]
fn emby_headers_accept_quoted_device_names_and_reject_conflicting_tokens() {
    let mut headers = HeaderMap::new();
    headers.insert("x-emby-authorization",HeaderValue::from_static("MediaBrowser Client=\"Popcorn\", Device=\"Phone, tablet\", DeviceId=\"d\", Version=\"1\", Token=\"abc\""));
    assert_eq!(
        auth::parameters(&headers).unwrap()["device"],
        "Phone, tablet"
    );
    assert_eq!(auth::credential(&headers, &HashMap::new()).unwrap(), "abc");
    assert!(auth::credential(
        &headers,
        &HashMap::from([("api_key".into(), "different".into())])
    )
    .is_err());
}

#[test]
fn router_registers_without_ambiguous_paths() {
    let _ = super::router();
}

use crate::state::*;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    Router,
};
use serde_json::{json, Value};
use sqlx::PgPool;
use std::{path::PathBuf, sync::Arc};
use tower::ServiceExt;

struct Fixture {
    state: AppState,
    root: PathBuf,
    owner: i64,
    library: i64,
    movie: i64,
    file: i64,
    series: i64,
    season: i64,
    episode: i64,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
impl Fixture {
    fn router(&self) -> Router {
        crate::app::build_router(self.state.clone(), None)
    }
}

async fn fixture(pool: PgPool) -> Fixture {
    let root = std::env::temp_dir().join(format!("mova-emby-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("movie.mp4"), b"0123456789").unwrap();
    std::fs::write(root.join("poster.jpg"), b"\xff\xd8\xffposter").unwrap();
    let owner = mova_application::bootstrap_admin(
        &pool,
        mova_application::BootstrapAdminInput {
            username: "owner".into(),
            password: "test-password".into(),
        },
        time::Duration::days(30),
    )
    .await
    .unwrap()
    .user
    .user
    .id;
    let library = mova_db::create_library(
        &pool,
        mova_db::CreateLibraryParams {
            name: "Library".into(),
            description: None,
            metadata_language: "en-US".into(),
            root_path: root.to_string_lossy().into(),
        },
    )
    .await
    .unwrap()
    .id;
    let movie = seed_item(&pool, library, "movie", "Movie").await;
    sqlx::query("update media_items set poster_path=$1 where id=$2")
        .bind(root.join("poster.jpg").to_string_lossy().as_ref())
        .bind(movie)
        .execute(&pool)
        .await
        .unwrap();
    let file = seed_file(&pool, library, movie, &root.join("movie.mp4")).await;
    let series = seed_item(&pool, library, "series", "Series").await;
    let season = sqlx::query_scalar(
        "insert into seasons(series_id,season_number,library_id) values($1,1,$2) returning id",
    )
    .bind(series)
    .bind(library)
    .fetch_one(&pool)
    .await
    .unwrap();
    let episode = seed_item(&pool, library, "episode", "Episode 1").await;
    sqlx::query("insert into episodes(media_item_id,season_id,episode_number,library_id) values($1,$2,1,$3)")
        .bind(episode)
        .bind(season)
        .bind(library)
        .execute(&pool)
        .await
        .unwrap();
    seed_file(&pool, library, episode, &root.join("movie.mp4")).await;
    let missing = seed_item(&pool, library, "episode", "Missing episode").await;
    sqlx::query("insert into episodes(media_item_id,season_id,episode_number,library_id) values($1,$2,2,$3)")
        .bind(missing)
        .bind(season)
        .bind(library)
        .execute(&pool)
        .await
        .unwrap();
    let state = AppState {
        db: pool,
        api_time_offset: time::UtcOffset::UTC,
        build_version: "test".into(),
        cache_dir: root.join("cache"),
        metadata_provider: Arc::new(mova_application::NullMetadataProvider),
        session_cookie_secure: false,
        auth_rate_limiter: Default::default(),
        scan_registry: Default::default(),
        realtime_hub: Default::default(),
        realtime_dispatcher: Default::default(),
        background_jobs: Default::default(),
        strm_streaming: Default::default(),
        storage_environment: Arc::new(mova_application::HostLibraryStorageEnvironment),
        media_inspections: Default::default(),
    };
    Fixture {
        state,
        root,
        owner,
        library,
        movie,
        file,
        series,
        season,
        episode,
    }
}
async fn seed_item(pool: &PgPool, library: i64, kind: &str, title: &str) -> i64 {
    sqlx::query_scalar("insert into media_items(library_id,media_type,title,source_title) values($1,$2,$3,$3) returning id").bind(library).bind(kind).bind(title).fetch_one(pool).await.unwrap()
}
async fn seed_file(pool: &PgPool, library: i64, item: i64, path: &std::path::Path) -> i64 {
    // Each persisted version has its own carrier path.
    let unique = path.with_file_name(format!("{item}.mp4"));
    std::fs::copy(path, &unique).unwrap();
    sqlx::query_scalar("insert into media_files(library_id,media_item_id,file_path,container,file_size,duration_seconds,video_codec,audio_codec) values($1,$2,$3,'mp4',10,1000,'h264','aac') returning id")
        .bind(library).bind(item).bind(unique.to_string_lossy().as_ref()).fetch_one(pool).await.unwrap()
}
async fn call(
    router: Router,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: Value,
) -> (StatusCode, HeaderMap, Vec<u8>) {
    let mut request = Request::builder().method(method).uri(path);
    if let Some(token) = token {
        request = request.header("X-Emby-Token", token);
    }
    let body = if body.is_null() {
        Body::empty()
    } else {
        request = request.header("content-type", "application/json");
        Body::from(body.to_string())
    };
    let response = router.oneshot(request.body(body).unwrap()).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .unwrap()
        .to_vec();
    (status, headers, bytes)
}
async fn json_call(
    router: Router,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: Value,
) -> Value {
    let (status, _, bytes) = call(router, method, path, token, body).await;
    assert!(
        status.is_success(),
        "{method} {path}: {status} {}",
        String::from_utf8_lossy(&bytes)
    );
    serde_json::from_slice(&bytes).unwrap()
}
async fn login(f: &Fixture, username: &str) -> String {
    json_call(
        f.router(),
        "POST",
        "/emby/Users/AuthenticateByName",
        None,
        json!({"Username":username,"Pw":"test-password"}),
    )
    .await["AccessToken"]
        .as_str()
        .unwrap()
        .into()
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and isolated PostgreSQL"]
async fn emby_login_browse_play_resume_logout(pool: PgPool) {
    let f = fixture(pool).await;
    let token = login(&f, "owner").await;
    let user = dto::external_id(Kind::User, f.owner);
    let item = dto::external_id(Kind::Item, f.movie);
    let info = json_call(
        f.router(),
        "GET",
        "/emby/System/Info/Public",
        None,
        Value::Null,
    )
    .await;
    assert_eq!(
        info["Id"],
        json_call(f.router(), "GET", "/System/Info/Public", None, Value::Null).await["Id"]
    );
    let views = json_call(
        f.router(),
        "GET",
        &format!("/emby/Users/{user}/Views"),
        Some(&token),
        Value::Null,
    )
    .await;
    assert_eq!(
        views["Items"][0]["Id"],
        dto::external_id(Kind::Library, f.library)
    );
    let detail = json_call(
        f.router(),
        "GET",
        &format!("/emby/Users/{user}/Items/{item}"),
        Some(&token),
        Value::Null,
    )
    .await;
    assert!(!detail.to_string().contains(f.root.to_str().unwrap()));
    let image = call(
        f.router(),
        "GET",
        &format!("/emby/Items/{item}/Images/Primary?api_key={token}"),
        None,
        Value::Null,
    )
    .await;
    assert_eq!(image.0, StatusCode::OK);
    assert_eq!(image.2, b"\xff\xd8\xffposter");
    let playback = json_call(
        f.router(),
        "POST",
        &format!("/emby/Items/{item}/PlaybackInfo"),
        Some(&token),
        json!({"UserId":user}),
    )
    .await;
    let stream = playback["MediaSources"][0]["DirectStreamUrl"]
        .as_str()
        .unwrap();
    let response = f
        .router()
        .oneshot(
            Request::builder()
                .uri(stream)
                .header("Range", "bytes=3-6")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(response.headers()["content-range"], "bytes 3-6/10");
    assert_eq!(
        to_bytes(response.into_body(), 100).await.unwrap().as_ref(),
        b"3456"
    );
    let head = call(f.router(), "HEAD", stream, None, Value::Null).await;
    assert_eq!(head.0, StatusCode::OK);
    assert!(head.2.is_empty());
    assert_eq!(head.1["content-length"], "10");
    let mut report = json!({"ItemId":item,"MediaSourceId":dto::external_id(Kind::File,f.file),"PlaySessionId":playback["PlaySessionId"],"PositionTicks":120_000_000});
    for endpoint in [
        "/Sessions/Playing",
        "/Sessions/Playing/Progress",
        "/Sessions/Playing/Stopped",
    ] {
        assert_eq!(
            call(
                f.router(),
                "POST",
                &format!("/emby{endpoint}"),
                Some(&token),
                report.clone()
            )
            .await
            .0,
            StatusCode::NO_CONTENT
        );
    }
    report["PositionTicks"] = json!(50_000_000);
    assert_eq!(
        call(
            f.router(),
            "POST",
            "/emby/Sessions/Playing/Progress",
            Some(&token),
            report
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    let progress =
        mova_application::get_playback_progress_for_media_item(&f.state.db, f.owner, f.movie)
            .await
            .unwrap()
            .unwrap();
    assert_eq!(progress.position_seconds, 12);
    assert!(!progress.is_finished);
    let resume = json_call(
        f.router(),
        "GET",
        &format!("/emby/Users/{user}/Items/Resume"),
        Some(&token),
        Value::Null,
    )
    .await;
    assert_eq!(resume["Items"][0]["Id"], item);
    assert_eq!(
        resume["Items"][0]["UserData"]["PlaybackPositionTicks"],
        120_000_000
    );
    // Clients that omit PlaySessionId can start again after stopping.
    for (endpoint, seconds) in [("Playing", 20), ("Playing/Stopped", 25), ("Playing", 30)] {
        assert_eq!(
            call(
                f.router(),
                "POST",
                &format!("/emby/Sessions/{endpoint}"),
                Some(&token),
                json!({"ItemId":item,"PositionTicks":seconds * 10_000_000})
            )
            .await
            .0,
            StatusCode::NO_CONTENT
        );
    }
    assert_eq!(
        mova_application::get_playback_progress_for_media_item(&f.state.db, f.owner, f.movie)
            .await
            .unwrap()
            .unwrap()
            .position_seconds,
        30
    );
    assert_eq!(
        call(
            f.router(),
            "POST",
            "/emby/Sessions/Logout",
            Some(&token),
            Value::Null
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        call(f.router(), "GET", "/emby/Items", Some(&token), Value::Null)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and isolated PostgreSQL"]
async fn emby_catalog_pagination_series_and_permissions(pool: PgPool) {
    let f = fixture(pool).await;
    let token = login(&f, "owner").await;
    let series = dto::external_id(Kind::Item, f.series);
    let season = dto::external_id(Kind::Season, f.season);
    let seasons = json_call(
        f.router(),
        "GET",
        &format!("/emby/Shows/{series}/Seasons"),
        Some(&token),
        Value::Null,
    )
    .await;
    assert_eq!(seasons["Items"][0]["Id"], season);
    assert_eq!(seasons["Items"][0]["ChildCount"], 1);
    let episodes = json_call(
        f.router(),
        "GET",
        &format!("/emby/Shows/{series}/Episodes?SeasonId={season}"),
        Some(&token),
        Value::Null,
    )
    .await;
    assert_eq!(episodes["TotalRecordCount"], 1);
    assert_eq!(
        episodes["Items"][0]["Id"],
        dto::external_id(Kind::Item, f.episode)
    );
    let first = json_call(
        f.router(),
        "GET",
        "/emby/Items?Recursive=true&StartIndex=0&Limit=2",
        Some(&token),
        Value::Null,
    )
    .await;
    let second = json_call(
        f.router(),
        "GET",
        "/emby/Items?Recursive=true&StartIndex=1&Limit=2",
        Some(&token),
        Value::Null,
    )
    .await;
    assert_eq!(first["TotalRecordCount"], 3);
    assert_eq!(first["Items"][1]["Id"], second["Items"][0]["Id"]);
    let viewer = mova_application::create_user(
        &f.state.db,
        f.owner,
        mova_application::CreateUserInput {
            username: "viewer".into(),
            password: "test-password".into(),
            role: "viewer".into(),
            is_enabled: true,
            library_ids: vec![],
        },
    )
    .await
    .unwrap();
    let viewer_token = login(&f, "viewer").await;
    assert_eq!(
        json_call(
            f.router(),
            "GET",
            "/emby/Items?Recursive=true",
            Some(&viewer_token),
            Value::Null
        )
        .await["TotalRecordCount"],
        0
    );
    assert_eq!(
        call(
            f.router(),
            "GET",
            &format!("/emby/Items/{}", dto::external_id(Kind::Item, f.movie)),
            Some(&viewer_token),
            Value::Null
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        call(
            f.router(),
            "GET",
            &format!(
                "/emby/Users/{}/Views",
                dto::external_id(Kind::User, f.owner)
            ),
            Some(&viewer_token),
            Value::Null
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    mova_application::update_user(
        &f.state.db,
        f.owner,
        viewer.user.id,
        mova_application::UpdateUserInput {
            is_enabled: Some(false),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(
        call(
            f.router(),
            "GET",
            "/emby/Items",
            Some(&viewer_token),
            Value::Null
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and isolated PostgreSQL"]
async fn emby_unknown_routes_do_not_return_native_envelopes(pool: PgPool) {
    let f = fixture(pool).await;
    let (status, _, bytes) = call(f.router(), "GET", "/emby/Unknown", None, Value::Null).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(body.get("ResponseStatus").is_some());
    assert!(body.get("data").is_none());
    assert_eq!(
        call(f.router(), "GET", "/emby/Items", None, Value::Null)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    std::fs::write(f.root.join("index.html"), "<html>web application</html>").unwrap();
    let router = crate::app::build_router(f.state.clone(), Some(f.root.clone()));
    let unknown = call(
        router,
        "GET",
        "/Users/unsupported/action",
        None,
        Value::Null,
    )
    .await;
    assert_eq!(unknown.0, StatusCode::NOT_FOUND);
    assert!(!String::from_utf8_lossy(&unknown.2).contains("<html>"));
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and isolated PostgreSQL"]
async fn emby_strm_streams_ranges_without_forwarding_credentials(pool: PgPool) {
    use axum::{response::IntoResponse, routing::get};
    use sha2::{Digest, Sha256};
    // Determine the local private interface without sending any datagram.
    // Loopback is intentionally forbidden by the production STRM policy.
    let probe = std::net::UdpSocket::bind("0.0.0.0:0").unwrap();
    probe.connect("192.0.2.1:9").unwrap();
    let ip = probe.local_addr().unwrap().ip();
    let listener = tokio::net::TcpListener::bind((ip, 0)).await.unwrap();
    let address = listener.local_addr().unwrap();
    let upstream = Router::new()
        .route(
            "/video",
            get(|headers: HeaderMap| async move {
                assert!(headers.get("authorization").is_none());
                assert!(headers.get("x-emby-token").is_none());
                assert!(headers.get("cookie").is_none());
                if headers.contains_key("range") {
                    (
                        StatusCode::PARTIAL_CONTENT,
                        [
                            ("content-type", "video/mp4"),
                            ("content-range", "bytes 3-6/10"),
                            ("accept-ranges", "bytes"),
                        ],
                        "3456",
                    )
                        .into_response()
                } else {
                    (
                        StatusCode::OK,
                        [("content-type", "video/mp4"), ("accept-ranges", "bytes")],
                        "0123456789",
                    )
                        .into_response()
                }
            }),
        )
        .route(
            "/hls",
            get(|| async {
                (
                    [("content-type", "application/vnd.apple.mpegurl")],
                    "#EXTM3U\n",
                )
            }),
        );
    let server = tokio::spawn(async move { axum::serve(listener, upstream).await.unwrap() });
    let mut f = fixture(pool).await;
    f.state.strm_streaming = mova_application::StrmStreamingService::new(
        mova_application::StrmStreamingConfig::new("test", Some(&address.to_string())).unwrap(),
    )
    .unwrap();
    let carrier = f.root.join("remote.strm");
    let reference = format!("http://{address}/video");
    std::fs::write(&carrier, &reference).unwrap();
    let remote = seed_item(&f.state.db, f.library, "movie", "Remote").await;
    let file:i64=sqlx::query_scalar("insert into media_files(library_id,media_item_id,file_path,file_size,source_kind,stream_reference_hash) values($1,$2,$3,$4,'strm',$5) returning id")
        .bind(f.library).bind(remote).bind(carrier.to_string_lossy().as_ref()).bind(reference.len() as i64).bind(format!("{:x}",Sha256::digest(reference.as_bytes()))).fetch_one(&f.state.db).await.unwrap();
    let token = login(&f, "owner").await;
    let item = dto::external_id(Kind::Item, remote);
    let playback = json_call(
        f.router(),
        "POST",
        &format!("/emby/Items/{item}/PlaybackInfo"),
        Some(&token),
        json!({}),
    )
    .await;
    assert!(playback["MediaSources"][0].get("Size").is_none());
    assert!(playback["MediaSources"][0].get("RunTimeTicks").is_none());
    assert!(!playback.to_string().contains(&address.to_string()));
    let url = playback["MediaSources"][0]["DirectStreamUrl"]
        .as_str()
        .unwrap();
    let response = f
        .router()
        .oneshot(
            Request::builder()
                .uri(url)
                .header("Range", "bytes=3-6")
                .header("X-Emby-Token", &token)
                .header("Cookie", "private=value")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(
        to_bytes(response.into_body(), 100).await.unwrap().as_ref(),
        b"3456"
    );
    let head = call(f.router(), "HEAD", url, None, Value::Null).await;
    assert_eq!(head.0, StatusCode::OK);
    assert!(head.2.is_empty());
    let report = json!({"ItemId":item,"MediaSourceId":dto::external_id(Kind::File,file),"PlaySessionId":playback["PlaySessionId"],"PositionTicks":300_000_000,"RunTimeTicks":1_000_000_000});
    assert_eq!(
        call(
            f.router(),
            "POST",
            "/emby/Sessions/Playing/Stopped",
            Some(&token),
            report
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    let progress =
        mova_application::get_playback_progress_for_media_item(&f.state.db, f.owner, remote)
            .await
            .unwrap()
            .unwrap();
    assert_eq!(progress.position_seconds, 30);
    assert_eq!(progress.duration_seconds, Some(100));
    assert!(!progress.is_finished);
    std::fs::write(&carrier, format!("http://{address}/hls")).unwrap();
    assert_eq!(
        call(f.router(), "GET", url, None, Value::Null).await.0,
        StatusCode::BAD_GATEWAY
    );
    std::fs::write(&carrier, "http://127.0.0.1:80/private").unwrap();
    assert_eq!(
        call(f.router(), "GET", url, None, Value::Null).await.0,
        StatusCode::FORBIDDEN
    );
    server.abort();
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and isolated PostgreSQL"]
async fn emby_profiles_sources_and_password_revocation(pool: PgPool) {
    let f = fixture(pool).await;
    let token = login(&f, "owner").await;
    let item = dto::external_id(Kind::Item, f.movie);
    let other_user = dto::external_id(Kind::User, f.owner + 1);
    assert_eq!(
        call(
            f.router(),
            "POST",
            &format!("/emby/Items/{item}/PlaybackInfo?UserId={other_user}"),
            Some(&token),
            json!({"UserId":dto::external_id(Kind::User,f.owner)})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let wrong_source = json!({"MediaSourceId":dto::external_id(Kind::File,i64::MAX)});
    assert_eq!(
        call(
            f.router(),
            "POST",
            &format!("/emby/Items/{item}/PlaybackInfo"),
            Some(&token),
            wrong_source
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let unsupported=json_call(f.router(),"POST",&format!("/emby/Items/{item}/PlaybackInfo"),Some(&token),json!({"DeviceProfile":{"DirectPlayProfiles":[{"Type":"Video","Container":"mkv","VideoCodec":"hevc"}]}})).await;
    assert_eq!(unsupported["ErrorCode"], "NoCompatibleStream");
    assert_eq!(unsupported["MediaSources"], json!([]));
    let older = json_call(
        f.router(),
        "POST",
        &format!("/emby/Items/{item}/PlaybackInfo"),
        Some(&token),
        json!({}),
    )
    .await;
    assert_eq!(call(f.router(), "POST", "/emby/Sessions/Playing", Some(&token),
        json!({"ItemId":item,"PlaySessionId":older["PlaySessionId"],"PositionTicks":100_000_000})).await.0, StatusCode::NO_CONTENT);
    let playback = json_call(
        f.router(),
        "POST",
        &format!("/emby/Items/{item}/PlaybackInfo"),
        Some(&token),
        json!({}),
    )
    .await;
    let report = json!({"ItemId":item,"PlaySessionId":playback["PlaySessionId"],"PositionTicks":9_500_000_000i64});
    assert_eq!(
        call(
            f.router(),
            "POST",
            "/emby/Sessions/Playing/Stopped",
            Some(&token),
            report
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    assert!(
        mova_application::get_playback_progress_for_media_item(&f.state.db, f.owner, f.movie)
            .await
            .unwrap()
            .unwrap()
            .is_finished
    );
    // An older active session cannot overwrite a newer completed playback.
    assert_eq!(call(f.router(), "POST", "/emby/Sessions/Playing/Progress", Some(&token),
        json!({"ItemId":item,"PlaySessionId":older["PlaySessionId"],"PositionTicks":200_000_000})).await.0, StatusCode::NO_CONTENT);
    assert!(
        mova_application::get_playback_progress_for_media_item(&f.state.db, f.owner, f.movie)
            .await
            .unwrap()
            .unwrap()
            .is_finished
    );
    mova_db::update_user_password_and_revoke_sessions(
        &f.state.db,
        f.owner,
        "replacement-hash",
        None,
    )
    .await
    .unwrap();
    assert_eq!(
        call(
            f.router(),
            "GET",
            "/emby/Users/Me",
            Some(&token),
            Value::Null
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
}

#[sqlx::test(migrations = false)]
#[ignore = "requires DATABASE_URL and isolated PostgreSQL"]
async fn emby_migration_preserves_initialized_library_and_progress(pool: PgPool) {
    for migration in [
        include_str!("../../../../migrations/0001_init.sql"),
        include_str!("../../../../migrations/0002_nfo_metadata.sql"),
        include_str!("../../../../migrations/0003_intro_detection.sql"),
        include_str!("../../../../migrations/0004_strm_sources.sql"),
        include_str!("../../../../migrations/0005_series_episode_outline_jsonb.sql"),
        include_str!("../../../../migrations/0006_library_media_sorting.sql"),
        include_str!("../../../../migrations/0007_library_storage.sql"),
        include_str!("../../../../migrations/0008_media_file_probe_issue.sql"),
        include_str!("../../../../migrations/0009_media_file_modified_at.sql"),
        include_str!("../../../../migrations/0010_scan_job_scope.sql"),
    ] {
        sqlx::raw_sql(migration).execute(&pool).await.unwrap();
    }
    let f = fixture(pool).await;
    mova_application::update_playback_progress_for_media_item(
        &f.state.db,
        f.owner,
        f.movie,
        mova_application::UpdatePlaybackProgressInput {
            media_file_id: f.file,
            position_seconds: 123,
            duration_seconds: Some(1000),
            is_finished: false,
        },
    )
    .await
    .unwrap();
    sqlx::raw_sql(include_str!(
        "../../../../migrations/0011_emby_sessions.sql"
    ))
    .execute(&f.state.db)
    .await
    .unwrap();
    let token = login(&f, "owner").await;
    let user = dto::external_id(Kind::User, f.owner);
    let resume = json_call(
        f.router(),
        "GET",
        &format!("/emby/Users/{user}/Items/Resume"),
        Some(&token),
        Value::Null,
    )
    .await;
    assert_eq!(
        resume["Items"][0]["Id"],
        dto::external_id(Kind::Item, f.movie)
    );
    assert_eq!(
        resume["Items"][0]["UserData"]["PlaybackPositionTicks"],
        1_230_000_000
    );
}
