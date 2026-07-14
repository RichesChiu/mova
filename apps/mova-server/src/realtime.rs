use crate::response::{ScanItemProgressResponse, ScanJobResponse};
use axum::response::sse::Event;
use mova_application::{ScanJobEvent, ScanJobProgressUpdate};
use mova_domain::UserProfile;
use serde::Serialize;
use sqlx::postgres::{PgListener, PgPool};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};
use time::UtcOffset;
use tokio::sync::{broadcast, mpsc};

const REALTIME_BATCH_BUFFER_SIZE: usize = 32;
const DISPATCH_COMMAND_BUFFER_SIZE: usize = 2_048;
const RESOURCE_BATCH_INTERVAL: Duration = Duration::from_millis(500);
const SCAN_PROGRESS_BATCH_INTERVAL: Duration = Duration::from_millis(200);
const CONTINUE_WATCHING_BATCH_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RealtimeVisibility {
    Public,
    Admin,
    Library(i64),
    User(i64),
}

#[derive(Debug, Clone)]
pub struct RealtimeMessage {
    event_name: &'static str,
    data: Arc<str>,
    visibility: RealtimeVisibility,
}

impl RealtimeMessage {
    fn json<T: Serialize>(
        event_name: &'static str,
        payload: &T,
        visibility: RealtimeVisibility,
    ) -> Option<Self> {
        serde_json::to_string(payload).ok().map(|data| Self {
            event_name,
            data: Arc::from(data),
            visibility,
        })
    }

    pub fn event_name(&self) -> &'static str {
        self.event_name
    }

    pub fn is_visible_to(&self, user: &UserProfile) -> bool {
        match self.visibility {
            RealtimeVisibility::Public => true,
            RealtimeVisibility::Admin => user.is_admin(),
            RealtimeVisibility::Library(library_id) => user.can_access_library(library_id),
            RealtimeVisibility::User(user_id) => user.user.id == user_id,
        }
    }

    pub fn to_sse_event(&self) -> Event {
        Event::default()
            .event(self.event_name)
            .data(self.data.as_ref())
    }
}

#[derive(Clone)]
pub struct RealtimeHub {
    sender: broadcast::Sender<RealtimeMessage>,
}

impl Default for RealtimeHub {
    fn default() -> Self {
        let (sender, _) = broadcast::channel(REALTIME_BATCH_BUFFER_SIZE);
        Self { sender }
    }
}

impl RealtimeHub {
    fn publish(&self, message: RealtimeMessage) {
        let _ = self.sender.send(message);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<RealtimeMessage> {
        self.sender.subscribe()
    }
}

#[derive(Clone)]
pub struct RealtimeDispatcherHandle {
    sender: mpsc::Sender<RealtimeCommand>,
}

impl RealtimeDispatcherHandle {
    pub fn publish_scan_event(&self, event: ScanJobEvent) {
        if self
            .sender
            .try_send(RealtimeCommand::ScanEvent(event))
            .is_err()
        {
            tracing::debug!("dropping transient scan progress because dispatcher is saturated");
        }
    }

    pub async fn publish_resource_immediately(&self, resource_key: String) {
        let _ = self
            .sender
            .send(RealtimeCommand::ResourceChangedImmediate(resource_key))
            .await;
    }
}

impl Default for RealtimeDispatcherHandle {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel(1);
        drop(receiver);
        Self { sender }
    }
}

pub fn start_realtime_dispatcher(
    pool: PgPool,
    hub: RealtimeHub,
    api_time_offset: UtcOffset,
) -> RealtimeDispatcherHandle {
    let (sender, receiver) = mpsc::channel(DISPATCH_COMMAND_BUFFER_SIZE);
    let dispatcher_sender = sender.clone();
    let listener_pool = pool.clone();

    tokio::spawn(async move {
        run_postgres_revision_listener(listener_pool, dispatcher_sender).await;
    });
    tokio::spawn(async move {
        RealtimeDispatcher::new(pool, hub, api_time_offset, receiver)
            .run()
            .await;
    });

    RealtimeDispatcherHandle { sender }
}

#[derive(Debug)]
enum RealtimeCommand {
    ResourceChanged(String),
    ResourceChangedImmediate(String),
    ScanEvent(ScanJobEvent),
}

#[derive(Default)]
struct ScanProgressBatch {
    scan_job: Option<ScanJobProgressUpdate>,
    items: HashMap<String, mova_application::ScanJobItemProgressUpdate>,
}

struct RealtimeDispatcher {
    pool: PgPool,
    hub: RealtimeHub,
    api_time_offset: UtcOffset,
    receiver: mpsc::Receiver<RealtimeCommand>,
    pending_resources: HashSet<String>,
    pending_continue_watching: HashSet<String>,
    pending_scans: HashMap<i64, ScanProgressBatch>,
}

impl RealtimeDispatcher {
    fn new(
        pool: PgPool,
        hub: RealtimeHub,
        api_time_offset: UtcOffset,
        receiver: mpsc::Receiver<RealtimeCommand>,
    ) -> Self {
        Self {
            pool,
            hub,
            api_time_offset,
            receiver,
            pending_resources: HashSet::new(),
            pending_continue_watching: HashSet::new(),
            pending_scans: HashMap::new(),
        }
    }

    async fn run(mut self) {
        let mut resource_tick = tokio::time::interval(RESOURCE_BATCH_INTERVAL);
        let mut scan_tick = tokio::time::interval(SCAN_PROGRESS_BATCH_INTERVAL);
        let mut continue_watching_tick = tokio::time::interval(CONTINUE_WATCHING_BATCH_INTERVAL);
        resource_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        scan_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        continue_watching_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                command = self.receiver.recv() => {
                    let Some(command) = command else {
                        break;
                    };
                    self.handle_command(command).await;
                }
                _ = resource_tick.tick() => self.flush_resource_changes().await,
                _ = scan_tick.tick() => self.flush_scan_progress().await,
                _ = continue_watching_tick.tick() => self.flush_continue_watching_changes().await,
            }
        }
    }

    async fn handle_command(&mut self, command: RealtimeCommand) {
        match command {
            RealtimeCommand::ResourceChanged(resource_key) => {
                if let Some(user_id) = session_user_id(&resource_key) {
                    self.publish_session_invalidated(user_id);
                } else if resource_key.ends_with(":continue-watching") {
                    self.pending_continue_watching.insert(resource_key);
                } else {
                    self.pending_resources.insert(resource_key);
                }
            }
            RealtimeCommand::ResourceChangedImmediate(resource_key) => {
                self.pending_continue_watching.remove(&resource_key);
                self.flush_resource_keys(vec![resource_key]).await;
            }
            RealtimeCommand::ScanEvent(ScanJobEvent::Updated(update)) => {
                let scan_job_id = update.scan_job.id;
                self.pending_scans.entry(scan_job_id).or_default().scan_job = Some(update);
            }
            RealtimeCommand::ScanEvent(ScanJobEvent::ItemUpdated(item)) => {
                self.pending_scans
                    .entry(item.scan_job_id)
                    .or_default()
                    .items
                    .insert(item.item_key.clone(), item);
            }
            RealtimeCommand::ScanEvent(ScanJobEvent::Finished(update)) => {
                self.publish_scan_finished(update).await;
            }
        }
    }

    async fn flush_resource_changes(&mut self) {
        if self.pending_resources.is_empty() {
            return;
        }

        let resource_keys = self.pending_resources.drain().collect::<Vec<_>>();
        self.flush_resource_keys(resource_keys).await;
    }

    async fn flush_continue_watching_changes(&mut self) {
        if self.pending_continue_watching.is_empty() {
            return;
        }
        let resource_keys = self.pending_continue_watching.drain().collect::<Vec<_>>();
        self.flush_resource_keys(resource_keys).await;
    }

    async fn flush_resource_keys(&mut self, resource_keys: Vec<String>) {
        let revisions = match mova_db::list_realtime_revisions(&self.pool, &resource_keys).await {
            Ok(revisions) => revisions,
            Err(error) => {
                tracing::warn!(error = ?error, "failed to load realtime resource revisions");
                for resource_key in resource_keys {
                    if resource_key.ends_with(":continue-watching") {
                        self.pending_continue_watching.insert(resource_key);
                    } else {
                        self.pending_resources.insert(resource_key);
                    }
                }
                return;
            }
        };

        let mut grouped: HashMap<RealtimeVisibility, Vec<ResourceRevisionResponse>> =
            HashMap::new();
        for revision in revisions {
            let Some(visibility) = visibility_for_resource(&revision.resource_key) else {
                continue;
            };
            grouped
                .entry(visibility)
                .or_default()
                .push(ResourceRevisionResponse {
                    resource: revision.resource_key,
                    revision: revision.revision,
                });
        }

        for (visibility, mut changes) in grouped {
            changes.sort_by(|left, right| left.resource.cmp(&right.resource));
            let payload = ResourcesChangedResponse {
                version: 1,
                changes,
            };
            if let Some(message) = RealtimeMessage::json("resources.changed", &payload, visibility)
            {
                self.hub.publish(message);
            }
        }
    }

    async fn flush_scan_progress(&mut self) {
        let pending = std::mem::take(&mut self.pending_scans);
        for (scan_job_id, batch) in pending {
            self.publish_scan_batch(scan_job_id, batch, "scan.progress")
                .await;
        }
    }

    async fn publish_scan_finished(&mut self, update: ScanJobProgressUpdate) {
        let scan_job_id = update.scan_job.id;
        let mut batch = self.pending_scans.remove(&scan_job_id).unwrap_or_default();
        batch.scan_job = Some(update);
        self.publish_scan_batch(scan_job_id, batch, "scan.finished")
            .await;
    }

    async fn publish_scan_batch(
        &self,
        scan_job_id: i64,
        batch: ScanProgressBatch,
        event_name: &'static str,
    ) {
        let update = match batch.scan_job {
            Some(update) => update,
            None => match mova_db::get_scan_job(&self.pool, scan_job_id).await {
                Ok(Some(scan_job)) => ScanJobProgressUpdate {
                    scan_job,
                    phase: None,
                },
                Ok(None) => return,
                Err(error) => {
                    tracing::warn!(scan_job_id, error = ?error, "failed to load scan job for realtime progress");
                    return;
                }
            },
        };

        let library_id = update.scan_job.library_id;
        let mut items = batch.items.into_values().collect::<Vec<_>>();
        items.sort_by_key(|item| item.item_index);
        let payload = ScanProgressResponse {
            version: 1,
            scan_job: ScanJobResponse::from_realtime(
                update.scan_job,
                update.phase,
                self.api_time_offset,
            ),
            items: items
                .into_iter()
                .map(ScanItemProgressResponse::from_domain)
                .collect(),
        };

        if let Some(message) = RealtimeMessage::json(
            event_name,
            &payload,
            RealtimeVisibility::Library(library_id),
        ) {
            self.hub.publish(message);
        }
    }

    fn publish_session_invalidated(&self, user_id: i64) {
        let payload = SessionInvalidatedResponse {
            reason: "authorization_changed",
        };
        if let Some(message) = RealtimeMessage::json(
            "session.invalidated",
            &payload,
            RealtimeVisibility::User(user_id),
        ) {
            self.hub.publish(message);
        }
    }
}

async fn run_postgres_revision_listener(pool: PgPool, sender: mpsc::Sender<RealtimeCommand>) {
    loop {
        let mut listener = match PgListener::connect_with(&pool).await {
            Ok(listener) => listener,
            Err(error) => {
                tracing::warn!(error = ?error, "failed to connect realtime PostgreSQL listener");
                tokio::time::sleep(Duration::from_secs(2)).await;
                continue;
            }
        };

        if let Err(error) = listener.listen("mova_realtime").await {
            tracing::warn!(error = ?error, "failed to subscribe realtime PostgreSQL listener");
            tokio::time::sleep(Duration::from_secs(2)).await;
            continue;
        }

        loop {
            match listener.recv().await {
                Ok(notification) => {
                    if sender
                        .send(RealtimeCommand::ResourceChanged(
                            notification.payload().to_string(),
                        ))
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
                Err(error) => {
                    tracing::warn!(error = ?error, "realtime PostgreSQL listener disconnected");
                    break;
                }
            }
        }
    }
}

fn session_user_id(resource_key: &str) -> Option<i64> {
    resource_key.strip_prefix("session:user:")?.parse().ok()
}

fn visibility_for_resource(resource_key: &str) -> Option<RealtimeVisibility> {
    if resource_key == "libraries" {
        return Some(RealtimeVisibility::Public);
    }
    if resource_key.starts_with("admin:") {
        return Some(RealtimeVisibility::Admin);
    }

    let mut parts = resource_key.split(':');
    match (parts.next(), parts.next()) {
        (Some("library"), Some(id)) => id.parse().ok().map(RealtimeVisibility::Library),
        (Some("user"), Some(id)) => id.parse().ok().map(RealtimeVisibility::User),
        _ => None,
    }
}

#[derive(Debug, Serialize)]
struct ResourceRevisionResponse {
    resource: String,
    revision: i64,
}

#[derive(Debug, Serialize)]
struct ResourcesChangedResponse {
    version: u8,
    changes: Vec<ResourceRevisionResponse>,
}

#[derive(Debug, Serialize)]
struct ScanProgressResponse {
    version: u8,
    scan_job: ScanJobResponse,
    items: Vec<ScanItemProgressResponse>,
}

#[derive(Debug, Serialize)]
struct SessionInvalidatedResponse {
    reason: &'static str,
}

#[cfg(test)]
mod tests {
    use super::{session_user_id, visibility_for_resource, RealtimeVisibility};

    #[test]
    fn resource_visibility_is_derived_from_aggregate_key() {
        assert_eq!(
            visibility_for_resource("library:7:catalog"),
            Some(RealtimeVisibility::Library(7))
        );
        assert_eq!(
            visibility_for_resource("user:12:continue-watching"),
            Some(RealtimeVisibility::User(12))
        );
        assert_eq!(
            visibility_for_resource("admin:users"),
            Some(RealtimeVisibility::Admin)
        );
    }

    #[test]
    fn session_resource_extracts_target_user() {
        assert_eq!(session_user_id("session:user:42"), Some(42));
        assert_eq!(session_user_id("user:42:profile"), None);
    }
}
