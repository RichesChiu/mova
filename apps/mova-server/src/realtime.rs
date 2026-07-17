use crate::response::{ScanItemProgressResponse, ScanJobResponse};
use axum::response::sse::Event;
use mova_application::{ScanJobEvent, ScanJobProgressUpdate};
use mova_domain::UserProfile;
use serde::Serialize;
use sqlx::postgres::{PgListener, PgPool};
use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, RwLock,
    },
    time::Duration,
};
use time::UtcOffset;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::{broadcast, mpsc};

const REALTIME_BATCH_BUFFER_SIZE: usize = 32;
const DISPATCH_COMMAND_BUFFER_SIZE: usize = 2_048;
const RESOURCE_BATCH_INTERVAL: Duration = Duration::from_millis(500);
const SCAN_PROGRESS_BATCH_INTERVAL: Duration = Duration::from_millis(200);
const CONTINUE_WATCHING_BATCH_INTERVAL: Duration = Duration::from_secs(1);
const FINISHED_SCAN_EVENT_GUARD_TTL: Duration = Duration::from_secs(60);
pub(crate) const REALTIME_PROTOCOL_VERSION: u8 = 1;

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

    pub fn closes_stream(&self) -> bool {
        matches!(self.event_name, "resync.required" | "session.invalidated")
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

struct RealtimeHubInner {
    public_sender: broadcast::Sender<RealtimeMessage>,
    admin_sender: broadcast::Sender<RealtimeMessage>,
    library_senders: RwLock<HashMap<i64, broadcast::Sender<RealtimeMessage>>>,
    user_senders: RwLock<HashMap<i64, broadcast::Sender<RealtimeMessage>>>,
}

#[derive(Clone)]
pub struct RealtimeHub {
    inner: Arc<RealtimeHubInner>,
}

impl Default for RealtimeHub {
    fn default() -> Self {
        let (public_sender, _) = broadcast::channel(REALTIME_BATCH_BUFFER_SIZE);
        let (admin_sender, _) = broadcast::channel(REALTIME_BATCH_BUFFER_SIZE);
        Self {
            inner: Arc::new(RealtimeHubInner {
                public_sender,
                admin_sender,
                library_senders: RwLock::new(HashMap::new()),
                user_senders: RwLock::new(HashMap::new()),
            }),
        }
    }
}

impl RealtimeHub {
    fn publish(&self, message: RealtimeMessage) {
        match message.visibility {
            RealtimeVisibility::Public => {
                let _ = self.inner.public_sender.send(message);
            }
            RealtimeVisibility::Admin => {
                let _ = self.inner.admin_sender.send(message);
            }
            RealtimeVisibility::Library(library_id) => {
                // Administrators see every library, while regular users only subscribe to
                // channels for libraries granted in their connection-time permission snapshot.
                let _ = self.inner.admin_sender.send(message.clone());
                let _ = Self::scoped_sender(&self.inner.library_senders, library_id).send(message);
            }
            RealtimeVisibility::User(user_id) => {
                let _ = Self::scoped_sender(&self.inner.user_senders, user_id).send(message);
            }
        }
    }

    pub fn subscribe(&self, user: &UserProfile) -> Vec<broadcast::Receiver<RealtimeMessage>> {
        let mut receivers = vec![
            self.inner.public_sender.subscribe(),
            Self::scoped_sender(&self.inner.user_senders, user.user.id).subscribe(),
        ];

        if user.is_admin() {
            receivers.push(self.inner.admin_sender.subscribe());
        } else {
            receivers.extend(user.library_ids.iter().map(|library_id| {
                Self::scoped_sender(&self.inner.library_senders, *library_id).subscribe()
            }));
        }

        receivers
    }

    fn scoped_sender(
        senders: &RwLock<HashMap<i64, broadcast::Sender<RealtimeMessage>>>,
        scope_id: i64,
    ) -> broadcast::Sender<RealtimeMessage> {
        let mut senders = senders.write().unwrap_or_else(|error| error.into_inner());
        senders
            .entry(scope_id)
            .or_insert_with(|| broadcast::channel(REALTIME_BATCH_BUFFER_SIZE).0)
            .clone()
    }

    fn publish_resync_required(&self, reason: &'static str) {
        let payload = ResyncRequiredResponse {
            protocol_version: REALTIME_PROTOCOL_VERSION,
            reason,
        };
        if let Some(message) =
            RealtimeMessage::json("resync.required", &payload, RealtimeVisibility::Public)
        {
            self.publish(message);
        }
    }
}

#[derive(Clone)]
pub struct RealtimeDispatcherHandle {
    sender: mpsc::Sender<RealtimeCommand>,
    reliable_sender: mpsc::UnboundedSender<RealtimeCommand>,
    scan_event_sequence: Arc<AtomicU64>,
}

impl RealtimeDispatcherHandle {
    pub fn publish_scan_event(&self, event: ScanJobEvent) {
        let is_reliable = matches!(
            event,
            ScanJobEvent::Checkpoint(_) | ScanJobEvent::Finished(_)
        );
        let command = RealtimeCommand::ScanEvent {
            sequence: self.scan_event_sequence.fetch_add(1, Ordering::Relaxed),
            event,
        };

        if is_reliable {
            if self.reliable_sender.send(command).is_err() {
                tracing::warn!("failed to deliver reliable scan event because dispatcher stopped");
            }
            return;
        }

        match self.sender.try_send(command) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                tracing::debug!("dropping transient scan progress because dispatcher is saturated");
            }
            Err(TrySendError::Closed(_)) => {
                tracing::warn!("failed to deliver scan event because dispatcher stopped");
            }
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
        let (reliable_sender, reliable_receiver) = mpsc::unbounded_channel();
        drop(receiver);
        drop(reliable_receiver);
        Self {
            sender,
            reliable_sender,
            scan_event_sequence: Arc::new(AtomicU64::new(1)),
        }
    }
}

pub fn start_realtime_dispatcher(
    pool: PgPool,
    hub: RealtimeHub,
    api_time_offset: UtcOffset,
) -> RealtimeDispatcherHandle {
    let (sender, receiver) = mpsc::channel(DISPATCH_COMMAND_BUFFER_SIZE);
    let (reliable_sender, reliable_receiver) = mpsc::unbounded_channel();
    let dispatcher_sender = sender.clone();
    let listener_pool = pool.clone();
    let listener_hub = hub.clone();

    tokio::spawn(async move {
        run_postgres_revision_listener(listener_pool, dispatcher_sender, listener_hub).await;
    });
    tokio::spawn(async move {
        RealtimeDispatcher::new(pool, hub, api_time_offset, receiver, reliable_receiver)
            .run()
            .await;
    });

    RealtimeDispatcherHandle {
        sender,
        reliable_sender,
        scan_event_sequence: Arc::new(AtomicU64::new(1)),
    }
}

#[derive(Debug)]
enum RealtimeCommand {
    ResourceChanged(String),
    ResourceChangedImmediate(String),
    ScanEvent { sequence: u64, event: ScanJobEvent },
}

#[derive(Default)]
struct ScanProgressBatch {
    scan_job: Option<ScanJobProgressUpdate>,
    items: HashMap<String, mova_application::ScanJobItemProgressUpdate>,
}

struct FinishedScanEventGuard {
    sequence: u64,
    finished_at: tokio::time::Instant,
}

struct RealtimeDispatcher {
    pool: PgPool,
    hub: RealtimeHub,
    api_time_offset: UtcOffset,
    receiver: mpsc::Receiver<RealtimeCommand>,
    reliable_receiver: mpsc::UnboundedReceiver<RealtimeCommand>,
    pending_resources: HashSet<String>,
    pending_continue_watching: HashSet<String>,
    pending_scans: HashMap<i64, ScanProgressBatch>,
    finished_scan_jobs: HashMap<i64, FinishedScanEventGuard>,
}

impl RealtimeDispatcher {
    fn new(
        pool: PgPool,
        hub: RealtimeHub,
        api_time_offset: UtcOffset,
        receiver: mpsc::Receiver<RealtimeCommand>,
        reliable_receiver: mpsc::UnboundedReceiver<RealtimeCommand>,
    ) -> Self {
        Self {
            pool,
            hub,
            api_time_offset,
            receiver,
            reliable_receiver,
            pending_resources: HashSet::new(),
            pending_continue_watching: HashSet::new(),
            pending_scans: HashMap::new(),
            finished_scan_jobs: HashMap::new(),
        }
    }

    async fn run(mut self) {
        let mut resource_tick = tokio::time::interval(RESOURCE_BATCH_INTERVAL);
        let mut scan_tick = tokio::time::interval(SCAN_PROGRESS_BATCH_INTERVAL);
        let mut continue_watching_tick = tokio::time::interval(CONTINUE_WATCHING_BATCH_INTERVAL);
        resource_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        scan_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        continue_watching_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut receiver_closed = false;
        let mut reliable_receiver_closed = false;

        loop {
            tokio::select! {
                biased;
                command = self.reliable_receiver.recv(), if !reliable_receiver_closed => {
                    match command {
                        Some(command) => self.handle_command(command).await,
                        None => reliable_receiver_closed = true,
                    }
                }
                command = self.receiver.recv(), if !receiver_closed => {
                    match command {
                        Some(command) => self.handle_command(command).await,
                        None => receiver_closed = true,
                    }
                }
                _ = resource_tick.tick() => self.flush_resource_changes().await,
                _ = scan_tick.tick() => self.flush_scan_progress().await,
                _ = continue_watching_tick.tick() => self.flush_continue_watching_changes().await,
            }

            if receiver_closed && reliable_receiver_closed {
                break;
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
            RealtimeCommand::ScanEvent {
                sequence,
                event: ScanJobEvent::Updated(update),
            } => {
                let scan_job_id = update.scan_job.id;
                if self.scan_event_is_stale(scan_job_id, sequence) {
                    return;
                }
                self.finished_scan_jobs.remove(&scan_job_id);
                self.pending_scans.entry(scan_job_id).or_default().scan_job = Some(update);
            }
            RealtimeCommand::ScanEvent {
                sequence,
                event: ScanJobEvent::Checkpoint(update),
            } => {
                let scan_job_id = update.scan_job.id;
                if self.scan_event_is_stale(scan_job_id, sequence) {
                    return;
                }
                self.finished_scan_jobs.remove(&scan_job_id);
                let mut batch = self.pending_scans.remove(&scan_job_id).unwrap_or_default();
                batch.scan_job = Some(update);
                self.publish_scan_batch(scan_job_id, batch, "scan.progress", true)
                    .await;
            }
            RealtimeCommand::ScanEvent {
                sequence,
                event: ScanJobEvent::ItemUpdated(item),
            } => {
                if self.scan_event_is_stale(item.scan_job_id, sequence) {
                    return;
                }
                self.finished_scan_jobs.remove(&item.scan_job_id);
                self.pending_scans
                    .entry(item.scan_job_id)
                    .or_default()
                    .items
                    .insert(item.item_key.clone(), item);
            }
            RealtimeCommand::ScanEvent {
                sequence,
                event: ScanJobEvent::Finished(update),
            } => {
                let scan_job_id = update.scan_job.id;
                if self.scan_event_is_stale(scan_job_id, sequence) {
                    return;
                }
                self.finished_scan_jobs.insert(
                    scan_job_id,
                    FinishedScanEventGuard {
                        sequence,
                        finished_at: tokio::time::Instant::now(),
                    },
                );
                self.publish_scan_finished(update).await;
            }
        }
    }

    fn scan_event_is_stale(&self, scan_job_id: i64, sequence: u64) -> bool {
        self.finished_scan_jobs
            .get(&scan_job_id)
            .is_some_and(|guard| sequence <= guard.sequence)
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
                protocol_version: REALTIME_PROTOCOL_VERSION,
                changes,
            };
            if let Some(message) = RealtimeMessage::json("resources.changed", &payload, visibility)
            {
                self.hub.publish(message);
            }
        }
    }

    async fn flush_scan_progress(&mut self) {
        let now = tokio::time::Instant::now();
        self.finished_scan_jobs.retain(|_, guard| {
            now.duration_since(guard.finished_at) < FINISHED_SCAN_EVENT_GUARD_TTL
        });
        let pending = std::mem::take(&mut self.pending_scans);
        for (scan_job_id, batch) in pending {
            self.publish_scan_batch(scan_job_id, batch, "scan.progress", false)
                .await;
        }
    }

    async fn publish_scan_finished(&mut self, update: ScanJobProgressUpdate) {
        let scan_job_id = update.scan_job.id;
        let mut batch = self.pending_scans.remove(&scan_job_id).unwrap_or_default();
        batch.scan_job = Some(update);
        self.publish_scan_batch(scan_job_id, batch, "scan.finished", true)
            .await;
    }

    async fn publish_scan_batch(
        &self,
        scan_job_id: i64,
        batch: ScanProgressBatch,
        event_name: &'static str,
        include_changes: bool,
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
            protocol_version: REALTIME_PROTOCOL_VERSION,
            scan_job: ScanJobResponse::from_realtime(
                update.scan_job,
                update.phase,
                self.api_time_offset,
            ),
            items: items
                .into_iter()
                .map(ScanItemProgressResponse::from_domain)
                .collect(),
            changes: if include_changes {
                self.load_scan_finished_revisions(library_id).await
            } else {
                Vec::new()
            },
        };

        if let Some(message) = RealtimeMessage::json(
            event_name,
            &payload,
            RealtimeVisibility::Library(library_id),
        ) {
            self.hub.publish(message);
        }
    }

    async fn load_scan_finished_revisions(&self, library_id: i64) -> Vec<ResourceRevisionResponse> {
        let resource_keys = vec![
            format!("library:{library_id}:catalog"),
            format!("library:{library_id}:scan"),
            format!("library:{library_id}:notifications"),
        ];
        match mova_db::list_realtime_revisions(&self.pool, &resource_keys).await {
            Ok(revisions) => {
                let mut changes = revisions
                    .into_iter()
                    .map(|revision| ResourceRevisionResponse {
                        resource: revision.resource_key,
                        revision: revision.revision,
                    })
                    .collect::<Vec<_>>();
                changes.sort_by(|left, right| left.resource.cmp(&right.resource));
                changes
            }
            Err(error) => {
                tracing::warn!(
                    library_id,
                    error = ?error,
                    "failed to load scan revisions for terminal realtime event"
                );
                Vec::new()
            }
        }
    }

    fn publish_session_invalidated(&self, user_id: i64) {
        let payload = SessionInvalidatedResponse {
            protocol_version: REALTIME_PROTOCOL_VERSION,
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

async fn run_postgres_revision_listener(
    pool: PgPool,
    sender: mpsc::Sender<RealtimeCommand>,
    hub: RealtimeHub,
) {
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

        hub.publish_resync_required("postgres_listener_subscribed");

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
    protocol_version: u8,
    changes: Vec<ResourceRevisionResponse>,
}

#[derive(Debug, Serialize)]
struct ScanProgressResponse {
    protocol_version: u8,
    scan_job: ScanJobResponse,
    items: Vec<ScanItemProgressResponse>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    changes: Vec<ResourceRevisionResponse>,
}

#[derive(Debug, Serialize)]
struct SessionInvalidatedResponse {
    protocol_version: u8,
    reason: &'static str,
}

#[derive(Debug, Serialize)]
struct ResyncRequiredResponse {
    protocol_version: u8,
    reason: &'static str,
}

#[cfg(test)]
mod tests {
    use super::{
        session_user_id, visibility_for_resource, RealtimeCommand, RealtimeDispatcherHandle,
        RealtimeMessage, RealtimeVisibility,
    };
    use mova_application::{ScanJobEvent, ScanJobProgressUpdate};
    use mova_domain::{ScanJob, User, UserProfile, UserRole};
    use std::sync::{atomic::AtomicU64, Arc};
    use time::OffsetDateTime;
    use tokio::sync::mpsc;

    fn test_user(id: i64, role: UserRole, library_ids: Vec<i64>) -> UserProfile {
        UserProfile {
            user: User {
                id,
                username: format!("user-{id}"),
                nickname: format!("User {id}"),
                role,
                is_enabled: true,
                created_at: OffsetDateTime::UNIX_EPOCH,
                updated_at: OffsetDateTime::UNIX_EPOCH,
            },
            is_primary_admin: false,
            library_ids,
        }
    }

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
        assert_eq!(
            visibility_for_resource("admin:libraries"),
            Some(RealtimeVisibility::Admin)
        );
    }

    #[test]
    fn session_resource_extracts_target_user() {
        assert_eq!(session_user_id("session:user:42"), Some(42));
        assert_eq!(session_user_id("user:42:profile"), None);
    }

    #[tokio::test]
    async fn reliable_scan_events_keep_order_when_transient_dispatch_is_saturated() {
        let (sender, mut receiver) = mpsc::channel(1);
        let (reliable_sender, mut reliable_receiver) = mpsc::unbounded_channel();
        sender
            .try_send(RealtimeCommand::ResourceChanged(
                "admin:libraries".to_string(),
            ))
            .expect("test channel should accept the first command");
        let handle = RealtimeDispatcherHandle {
            sender,
            reliable_sender,
            scan_event_sequence: Arc::new(AtomicU64::new(1)),
        };
        let update = ScanJobProgressUpdate {
            scan_job: ScanJob {
                id: 41,
                library_id: 7,
                status: "success".to_string(),
                phase: Some("finished".to_string()),
                total_files: 20,
                scanned_files: 20,
                local_analyzed_files: 20,
                local_committed_files: 20,
                remote_completed_files: 20,
                progress_percent: 100,
                created_at: OffsetDateTime::UNIX_EPOCH,
                started_at: Some(OffsetDateTime::UNIX_EPOCH),
                finished_at: Some(OffsetDateTime::UNIX_EPOCH),
                error_message: None,
            },
            phase: Some("finished".to_string()),
        };

        handle.publish_scan_event(ScanJobEvent::Checkpoint(update.clone()));
        handle.publish_scan_event(ScanJobEvent::Finished(update));

        assert!(matches!(
            receiver.recv().await,
            Some(RealtimeCommand::ResourceChanged(_))
        ));
        let checkpoint_sequence = match reliable_receiver.recv().await {
            Some(RealtimeCommand::ScanEvent {
                sequence,
                event: ScanJobEvent::Checkpoint(_),
            }) => sequence,
            other => panic!("expected checkpoint command, got {other:?}"),
        };
        let finished_sequence = match reliable_receiver.recv().await {
            Some(RealtimeCommand::ScanEvent {
                sequence,
                event: ScanJobEvent::Finished(_),
            }) => sequence,
            other => panic!("expected finished command, got {other:?}"),
        };
        assert!(checkpoint_sequence < finished_sequence);
    }

    #[tokio::test]
    async fn resync_required_is_published_as_a_terminal_public_event() {
        let hub = super::RealtimeHub::default();
        let user = test_user(12, UserRole::Viewer, vec![7]);
        let mut receivers = hub.subscribe(&user);
        let mut receiver = receivers.remove(0);

        hub.publish_resync_required("postgres_listener_subscribed");

        let message = receiver
            .recv()
            .await
            .expect("resync event should be published");
        assert!(message.closes_stream());
        assert_eq!(message.visibility, RealtimeVisibility::Public);
        assert!(message.data.contains(r#""protocol_version":1"#));
        assert!(message.data.contains("postgres_listener_subscribed"));
    }

    #[tokio::test]
    async fn scoped_hub_does_not_wake_unrelated_viewers_and_still_reaches_admins() {
        let hub = super::RealtimeHub::default();
        let viewer = test_user(12, UserRole::Viewer, vec![7]);
        let admin = test_user(1, UserRole::Admin, Vec::new());
        let mut viewer_receivers = hub.subscribe(&viewer);
        let mut admin_receivers = hub.subscribe(&admin);

        hub.publish(
            RealtimeMessage::json(
                "resources.changed",
                &serde_json::json!({"protocol_version": 1, "changes": []}),
                RealtimeVisibility::Library(8),
            )
            .unwrap(),
        );

        assert!(viewer_receivers
            .iter_mut()
            .all(|receiver| receiver.try_recv().is_err()));
        let admin_message = admin_receivers[2]
            .recv()
            .await
            .expect("admin channel should receive every library event");
        assert_eq!(admin_message.visibility, RealtimeVisibility::Library(8));

        hub.publish(
            RealtimeMessage::json(
                "resources.changed",
                &serde_json::json!({"protocol_version": 1, "changes": []}),
                RealtimeVisibility::Library(7),
            )
            .unwrap(),
        );
        let viewer_message = viewer_receivers[2]
            .recv()
            .await
            .expect("viewer library channel should receive an authorized event");
        assert_eq!(viewer_message.visibility, RealtimeVisibility::Library(7));
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn library_revision_triggers_separate_collection_and_settings(pool: sqlx::PgPool) {
        let library_id: i64 = sqlx::query_scalar(
            r#"
            insert into libraries (name, library_type, metadata_language, root_path)
            values ('Movies', 'mixed', 'zh-CN', '/media/movies')
            returning id
            "#,
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        let collection_key = "admin:libraries".to_string();
        let settings_key = format!("library:{library_id}:settings");
        let initial = mova_db::list_realtime_revisions(
            &pool,
            &[collection_key.clone(), settings_key.clone()],
        )
        .await
        .unwrap();
        assert_eq!(
            initial
                .iter()
                .find(|revision| revision.resource_key == collection_key)
                .map(|revision| revision.revision),
            Some(1)
        );
        assert!(!initial
            .iter()
            .any(|revision| revision.resource_key == settings_key));

        sqlx::query("update libraries set name = 'Movies 2' where id = $1")
            .bind(library_id)
            .execute(&pool)
            .await
            .unwrap();
        let updated = mova_db::list_realtime_revisions(
            &pool,
            &[collection_key.clone(), settings_key.clone()],
        )
        .await
        .unwrap();
        assert_eq!(
            updated
                .iter()
                .find(|revision| revision.resource_key == collection_key)
                .map(|revision| revision.revision),
            Some(1)
        );
        assert_eq!(
            updated
                .iter()
                .find(|revision| revision.resource_key == settings_key)
                .map(|revision| revision.revision),
            Some(1)
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a reachable Postgres test database"]
    async fn scan_revision_tracks_durable_lifecycle_transitions(pool: sqlx::PgPool) {
        let library_id: i64 = sqlx::query_scalar(
            r#"
            insert into libraries (name, library_type, metadata_language, root_path)
            values ('Series', 'mixed', 'zh-CN', '/media/series')
            returning id
            "#,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let scan_job_id: i64 = sqlx::query_scalar(
            r#"
            insert into scan_jobs (library_id, status, total_files, scanned_files)
            values ($1, 'pending', 0, 0)
            returning id
            "#,
        )
        .bind(library_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        let scan_key = format!("library:{library_id}:scan");

        let revision_after_insert =
            mova_db::list_realtime_revisions(&pool, std::slice::from_ref(&scan_key))
                .await
                .unwrap();
        assert_eq!(revision_after_insert[0].revision, 1);

        sqlx::query("update scan_jobs set status = 'running' where id = $1")
            .bind(scan_job_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("update scan_jobs set scanned_files = 1 where id = $1")
            .bind(scan_job_id)
            .execute(&pool)
            .await
            .unwrap();
        let revision_while_running =
            mova_db::list_realtime_revisions(&pool, std::slice::from_ref(&scan_key))
                .await
                .unwrap();
        assert_eq!(revision_while_running[0].revision, 2);

        sqlx::query("update scan_jobs set status = 'success' where id = $1")
            .bind(scan_job_id)
            .execute(&pool)
            .await
            .unwrap();
        let revision_after_finish = mova_db::list_realtime_revisions(&pool, &[scan_key])
            .await
            .unwrap();
        assert_eq!(revision_after_finish[0].revision, 3);
    }
}
