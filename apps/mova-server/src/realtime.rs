use crate::response::ScanJobResponse;
use axum::response::sse::Event;
use mova_domain::{ScanJob, UserProfile};
use serde::Serialize;
use time::UtcOffset;
use tokio::sync::broadcast;

const REALTIME_EVENT_BUFFER_SIZE: usize = 256;

#[derive(Clone)]
pub struct RealtimeHub {
    sender: broadcast::Sender<RealtimeEvent>,
}

impl Default for RealtimeHub {
    fn default() -> Self {
        let (sender, _) = broadcast::channel(REALTIME_EVENT_BUFFER_SIZE);
        Self { sender }
    }
}

impl RealtimeHub {
    pub fn publish(&self, event: RealtimeEvent) {
        let _ = self.sender.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<RealtimeEvent> {
        self.sender.subscribe()
    }
}

#[derive(Debug, Clone)]
pub enum RealtimeEvent {
    ScanJobUpdated { scan_job: ScanJob },
    ScanJobFinished { scan_job: ScanJob },
}

impl RealtimeEvent {
    pub fn event_name(&self) -> &'static str {
        match self {
            Self::ScanJobUpdated { .. } => "scan.job.updated",
            Self::ScanJobFinished { .. } => "scan.job.finished",
        }
    }

    pub fn is_visible_to(&self, user: &UserProfile) -> bool {
        user.can_access_library(self.library_id())
    }

    pub fn to_sse_event(&self, api_time_offset: UtcOffset) -> Option<Event> {
        let response = RealtimeEventResponse::from_event(self, api_time_offset);

        Event::default()
            .event(self.event_name())
            .json_data(response)
            .ok()
    }

    fn library_id(&self) -> i64 {
        match self {
            Self::ScanJobUpdated { scan_job } | Self::ScanJobFinished { scan_job } => {
                scan_job.library_id
            }
        }
    }
}

#[derive(Debug, Serialize)]
struct RealtimeEventResponse {
    #[serde(rename = "type")]
    event_type: String,
    scan_job: ScanJobResponse,
}

impl RealtimeEventResponse {
    fn from_event(event: &RealtimeEvent, api_time_offset: UtcOffset) -> Self {
        let scan_job = match event {
            RealtimeEvent::ScanJobUpdated { scan_job }
            | RealtimeEvent::ScanJobFinished { scan_job } => scan_job.clone(),
        };

        Self {
            event_type: event.event_name().to_string(),
            scan_job: ScanJobResponse::from_domain(scan_job, api_time_offset),
        }
    }
}
