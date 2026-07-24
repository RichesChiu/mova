use crate::realtime::REALTIME_PROTOCOL_VERSION;
use crate::{
    auth::require_user,
    error::ApiError,
    response::{ok, ApiJson, ScanJobResponse},
    state::AppState,
};
use axum::{
    extract::State,
    http::HeaderMap,
    response::sse::{Event, KeepAlive, Sse},
};
use axum_extra::extract::cookie::CookieJar;
use serde::Serialize;
use std::{
    collections::BTreeMap,
    convert::Infallible,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};
use tokio_stream::{
    wrappers::{errors::BroadcastStreamRecvError, BroadcastStream},
    Stream, StreamMap,
};

#[derive(Debug, Serialize)]
pub struct RealtimeStateResponse {
    protocol_version: u8,
    server_epoch: String,
    resources: BTreeMap<String, i64>,
    active_scans: Vec<ScanJobResponse>,
}

pub(crate) struct RealtimeResourceSnapshot {
    pub server_epoch: String,
    pub resources: BTreeMap<String, i64>,
}

pub async fn state(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
) -> Result<ApiJson<RealtimeStateResponse>, ApiError> {
    let user = require_user(&state, &headers, &jar).await?;
    let snapshot = load_realtime_resource_snapshot(&state, &user).await?;
    let visible_library_ids = user.library_visibility().restricted_library_ids();
    let active_scans = mova_db::list_active_scan_jobs(&state.db, visible_library_ids)
        .await
        .map_err(ApiError::from)?
        .into_iter()
        .map(|scan_job| ScanJobResponse::from_domain(scan_job, state.api_time_offset))
        .collect();

    Ok(ok(RealtimeStateResponse {
        protocol_version: REALTIME_PROTOCOL_VERSION,
        server_epoch: snapshot.server_epoch,
        resources: snapshot.resources,
        active_scans,
    }))
}

pub(crate) async fn load_realtime_resource_snapshot(
    state: &AppState,
    user: &mova_domain::UserProfile,
) -> Result<RealtimeResourceSnapshot, ApiError> {
    let visible_library_ids =
        mova_application::list_libraries(&state.db, user.library_visibility())
            .await
            .map_err(ApiError::from)?
            .into_iter()
            .map(|library| library.id)
            .collect::<Vec<_>>();
    let resource_keys = resource_keys_for_user(user, &visible_library_ids);
    let revisions = mova_db::list_realtime_revisions(&state.db, &resource_keys)
        .await
        .map_err(ApiError::from)?;
    let revision_by_key = revisions
        .into_iter()
        .map(|revision| (revision.resource_key, revision.revision))
        .collect::<BTreeMap<_, _>>();
    let resources = resource_keys
        .into_iter()
        .map(|key| {
            let revision = revision_by_key.get(&key).copied().unwrap_or(0);
            (key, revision)
        })
        .collect();
    Ok(RealtimeResourceSnapshot {
        server_epoch: mova_db::get_realtime_server_epoch(&state.db)
            .await
            .map_err(ApiError::from)?,
        resources,
    })
}

pub async fn events(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
) -> Result<Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let user = require_user(&state, &headers, &jar).await?;
    let mut receivers = StreamMap::new();
    for (index, receiver) in state.realtime_hub.subscribe(&user).into_iter().enumerate() {
        receivers.insert(index, BroadcastStream::new(receiver));
    }
    let stream = RealtimeSseStream {
        receivers,
        user,
        close_on_next_poll: false,
    };

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("heartbeat"),
    ))
}

struct RealtimeSseStream {
    receivers: StreamMap<usize, BroadcastStream<crate::realtime::RealtimeMessage>>,
    user: mova_domain::UserProfile,
    close_on_next_poll: bool,
}

impl Stream for RealtimeSseStream {
    type Item = Result<Event, Infallible>;

    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.close_on_next_poll {
            return Poll::Ready(None);
        }

        loop {
            match Pin::new(&mut self.receivers).poll_next(context) {
                Poll::Ready(Some((_, Ok(message)))) if message.is_visible_to(&self.user) => {
                    self.close_on_next_poll = message.closes_stream();
                    return Poll::Ready(Some(Ok(message.to_sse_event())));
                }
                Poll::Ready(Some((_, Ok(_)))) => continue,
                Poll::Ready(Some((_, Err(BroadcastStreamRecvError::Lagged(skipped))))) => {
                    tracing::warn!(
                        skipped,
                        user_id = self.user.user.id,
                        "closing lagged realtime event stream"
                    );
                    self.close_on_next_poll = true;
                    return Poll::Ready(Some(Ok(Event::default()
                        .event("resync.required")
                        .data(format!(
                            r#"{{"protocol_version":{REALTIME_PROTOCOL_VERSION},"reason":"client_lagged"}}"#
                        )))));
                }
                Poll::Ready(None) => return Poll::Ready(None),
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

fn resource_keys_for_user(
    user: &mova_domain::UserProfile,
    visible_library_ids: &[i64],
) -> Vec<String> {
    let mut keys = vec![
        format!("user:{}:profile", user.user.id),
        format!("user:{}:continue-watching", user.user.id),
        format!("user:{}:notifications", user.user.id),
    ];
    if user.is_admin() {
        keys.push("admin:libraries".to_string());
        keys.push("admin:users".to_string());
        keys.push("admin:notifications".to_string());
    } else {
        keys.push(format!("user:{}:libraries", user.user.id));
    }
    for library_id in visible_library_ids {
        keys.push(format!("library:{library_id}:settings"));
        keys.push(format!("library:{library_id}:catalog"));
        keys.push(format!("library:{library_id}:scan"));
        keys.push(format!("library:{library_id}:notifications"));
    }
    keys
}

#[cfg(test)]
mod tests {
    use super::resource_keys_for_user;
    use mova_domain::{User, UserProfile, UserRole};
    use time::OffsetDateTime;

    #[test]
    fn realtime_state_keys_only_include_visible_library_resources() {
        let user = UserProfile {
            user: User {
                id: 12,
                username: "viewer".to_string(),
                nickname: "viewer".to_string(),
                role: UserRole::Viewer,
                is_enabled: true,
                created_at: OffsetDateTime::UNIX_EPOCH,
                updated_at: OffsetDateTime::UNIX_EPOCH,
            },
            is_primary_admin: false,
            library_ids: vec![7],
        };

        let keys = resource_keys_for_user(&user, &[7]);
        assert!(keys.contains(&"library:7:catalog".to_string()));
        assert!(keys.contains(&"user:12:libraries".to_string()));
        assert!(!keys.contains(&"library:8:catalog".to_string()));
        assert!(!keys.contains(&"admin:libraries".to_string()));
        assert!(!keys.contains(&"admin:users".to_string()));
        assert!(!keys.contains(&"admin:notifications".to_string()));
    }

    #[test]
    fn realtime_state_includes_admin_notifications_for_admins() {
        let user = UserProfile {
            user: User {
                id: 1,
                username: "admin".to_string(),
                nickname: "admin".to_string(),
                role: UserRole::Admin,
                is_enabled: true,
                created_at: OffsetDateTime::UNIX_EPOCH,
                updated_at: OffsetDateTime::UNIX_EPOCH,
            },
            is_primary_admin: true,
            library_ids: Vec::new(),
        };

        let keys = resource_keys_for_user(&user, &[]);

        assert!(keys.contains(&"admin:notifications".to_string()));
    }
}
