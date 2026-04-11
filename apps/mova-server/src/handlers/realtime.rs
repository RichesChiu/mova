use crate::{auth::require_user, error::ApiError, state::AppState};
use axum::{
    extract::State,
    response::sse::{KeepAlive, Sse},
};
use axum_extra::extract::cookie::CookieJar;
use std::{convert::Infallible, time::Duration};
use tokio_stream::{
    wrappers::{errors::BroadcastStreamRecvError, BroadcastStream},
    StreamExt,
};

pub async fn events(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<
    Sse<impl tokio_stream::Stream<Item = Result<axum::response::sse::Event, Infallible>>>,
    ApiError,
> {
    let user = require_user(&state, &jar).await?;
    let api_time_offset = state.api_time_offset;
    let stream_user = user.clone();

    let stream = BroadcastStream::new(state.realtime_hub.subscribe()).filter_map(move |message| {
        let user = stream_user.clone();

        match message {
            Ok(event) if event.is_visible_to(&user) => event.to_sse_event(api_time_offset).map(Ok),
            Ok(_) => None,
            Err(BroadcastStreamRecvError::Lagged(skipped)) => {
                tracing::warn!(
                    skipped,
                    user_id = user.user.id,
                    "realtime event stream lagged"
                );
                None
            }
        }
    });

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("heartbeat"),
    ))
}

#[cfg(test)]
mod tests {
    use super::events;
    use crate::{
        auth::{attach_session_cookie, SESSION_TTL},
        realtime::RealtimeEvent,
        state::{AppState, LibrarySyncRegistry, RealtimeHub, ScanRegistry},
    };
    use axum::{extract::State, response::IntoResponse};
    use axum_extra::extract::cookie::CookieJar;
    use http_body_util::BodyExt;
    use mova_application::{NullMetadataProvider, ScanJobItemProgressUpdate};
    use mova_domain::UserRole;
    use std::{path::PathBuf, sync::Arc};
    use time::{OffsetDateTime, UtcOffset};
    use tokio::time::{timeout, Duration};

    fn build_test_state(pool: sqlx::postgres::PgPool) -> AppState {
        AppState {
            db: pool,
            api_time_offset: UtcOffset::UTC,
            artwork_cache_dir: PathBuf::from("/tmp/mova-test-artwork"),
            metadata_provider: Arc::new(NullMetadataProvider),
            scan_registry: ScanRegistry::default(),
            library_sync_registry: LibrarySyncRegistry::default(),
            realtime_hub: RealtimeHub::default(),
        }
    }

    async fn seed_library(pool: &sqlx::postgres::PgPool, name: &str, root_path: &str) -> i64 {
        mova_db::create_library(
            pool,
            mova_db::CreateLibraryParams {
                name: name.to_string(),
                description: None,
                library_type: "movie".to_string(),
                metadata_language: "zh-CN".to_string(),
                root_path: root_path.to_string(),
                is_enabled: true,
            },
        )
        .await
        .unwrap()
        .id
    }

    async fn seed_viewer_session(
        pool: &sqlx::postgres::PgPool,
        username: &str,
        library_ids: Vec<i64>,
        session_token: &str,
    ) -> CookieJar {
        let user = mova_db::create_user(
            pool,
            mova_db::CreateUserParams {
                username: username.to_string(),
                nickname: username.to_string(),
                password_hash: "hash".to_string(),
                role: UserRole::Viewer,
                is_enabled: true,
                library_ids,
            },
        )
        .await
        .unwrap();

        let expires_at = OffsetDateTime::now_utc() + SESSION_TTL;
        mova_db::create_session(
            pool,
            mova_db::CreateSessionParams {
                token: session_token.to_string(),
                user_id: user.user.id,
                expires_at,
            },
        )
        .await
        .unwrap();

        attach_session_cookie(CookieJar::new(), session_token, expires_at)
    }

    async fn read_first_sse_chunk(
        response: axum::response::Response,
        wait: Duration,
    ) -> Option<String> {
        let mut body = response.into_body();
        let frame = match timeout(wait, body.frame()).await {
            Ok(Some(Ok(frame))) => frame,
            Ok(Some(Err(error))) => panic!("failed to read SSE body frame: {error}"),
            Ok(None) | Err(_) => return None,
        };
        let bytes = frame.into_data().expect("expected an SSE data frame");

        Some(String::from_utf8(bytes.to_vec()).expect("SSE body must be valid utf-8"))
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn events_stream_serializes_library_updated_messages(pool: sqlx::postgres::PgPool) {
        let state = build_test_state(pool.clone());
        let library_id = seed_library(&pool, "Movies", "/media/movies").await;
        let jar =
            seed_viewer_session(&pool, "viewer01", vec![library_id], "realtime-test-session").await;

        let sse = events(State(state.clone()), jar).await.unwrap();
        state
            .realtime_hub
            .publish(RealtimeEvent::LibraryUpdated { library_id });

        let response = sse.into_response();
        let content_type = response
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();

        assert!(content_type.starts_with("text/event-stream"));

        let body = read_first_sse_chunk(response, Duration::from_secs(1))
            .await
            .expect("expected a visible SSE event");

        assert!(body.contains("event: library.updated"));
        assert!(body.contains("\"type\":\"library.updated\""));
        assert!(body.contains(&format!("\"library_id\":{}", library_id)));
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn events_stream_serializes_scan_item_progress_messages(pool: sqlx::postgres::PgPool) {
        let state = build_test_state(pool.clone());
        let library_id = seed_library(&pool, "Movies", "/media/movies").await;
        let jar =
            seed_viewer_session(&pool, "viewer01", vec![library_id], "realtime-item-session").await;

        let sse = events(State(state.clone()), jar).await.unwrap();
        state.realtime_hub.publish(RealtimeEvent::ScanItemUpdated {
            item: ScanJobItemProgressUpdate {
                scan_job_id: 41,
                library_id,
                item_key: "/media/movies/Interstellar (2014)/Interstellar.mkv".to_string(),
                media_type: "movie".to_string(),
                title: "Interstellar".to_string(),
                season_number: None,
                episode_number: None,
                item_index: 1,
                total_items: 3,
                stage: "artwork".to_string(),
                progress_percent: 68,
            },
        });

        let body = read_first_sse_chunk(sse.into_response(), Duration::from_secs(1))
            .await
            .expect("expected a visible SSE event");

        assert!(body.contains("event: scan.item.updated"));
        assert!(body.contains("\"type\":\"scan.item.updated\""));
        assert!(body.contains("\"title\":\"Interstellar\""));
        assert!(body.contains("\"stage\":\"artwork\""));
        assert!(body.contains("\"progress_percent\":68"));
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn events_stream_hides_library_updates_from_viewers_without_access(
        pool: sqlx::postgres::PgPool,
    ) {
        let state = build_test_state(pool.clone());
        let visible_library_id = seed_library(&pool, "Movies", "/media/movies").await;
        let hidden_library_id = seed_library(&pool, "Anime", "/media/anime").await;
        let jar = seed_viewer_session(
            &pool,
            "viewer01",
            vec![visible_library_id],
            "realtime-hidden-library-session",
        )
        .await;

        let sse = events(State(state.clone()), jar).await.unwrap();
        state.realtime_hub.publish(RealtimeEvent::LibraryUpdated {
            library_id: hidden_library_id,
        });

        let body = read_first_sse_chunk(sse.into_response(), Duration::from_millis(200)).await;

        assert!(
            body.is_none(),
            "hidden library events should not be streamed"
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn events_stream_hides_scan_item_updates_from_viewers_without_access(
        pool: sqlx::postgres::PgPool,
    ) {
        let state = build_test_state(pool.clone());
        let visible_library_id = seed_library(&pool, "Movies", "/media/movies").await;
        let hidden_library_id = seed_library(&pool, "Anime", "/media/anime").await;
        let jar = seed_viewer_session(
            &pool,
            "viewer01",
            vec![visible_library_id],
            "realtime-hidden-scan-item-session",
        )
        .await;

        let sse = events(State(state.clone()), jar).await.unwrap();
        state.realtime_hub.publish(RealtimeEvent::ScanItemUpdated {
            item: ScanJobItemProgressUpdate {
                scan_job_id: 41,
                library_id: hidden_library_id,
                item_key: "/media/anime/Spirited Away.mkv".to_string(),
                media_type: "movie".to_string(),
                title: "Spirited Away".to_string(),
                season_number: None,
                episode_number: None,
                item_index: 1,
                total_items: 1,
                stage: "metadata".to_string(),
                progress_percent: 35,
            },
        });

        let body = read_first_sse_chunk(sse.into_response(), Duration::from_millis(200)).await;

        assert!(
            body.is_none(),
            "hidden scan item events should not be streamed"
        );
    }
}
