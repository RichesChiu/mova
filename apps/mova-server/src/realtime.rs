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
    LibraryUpdated { library_id: i64 },
    LibraryDeleted { library_id: i64 },
    MediaItemMetadataUpdated { library_id: i64, media_item_id: i64 },
}

impl RealtimeEvent {
    pub fn event_name(&self) -> &'static str {
        match self {
            Self::ScanJobUpdated { .. } => "scan.job.updated",
            Self::ScanJobFinished { .. } => "scan.job.finished",
            Self::LibraryUpdated { .. } => "library.updated",
            Self::LibraryDeleted { .. } => "library.deleted",
            Self::MediaItemMetadataUpdated { .. } => "media_item.metadata.updated",
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
            Self::LibraryUpdated { library_id }
            | Self::LibraryDeleted { library_id }
            | Self::MediaItemMetadataUpdated { library_id, .. } => *library_id,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum RealtimeEventResponse {
    #[serde(rename = "scan.job.updated")]
    ScanJobUpdated { scan_job: ScanJobResponse },
    #[serde(rename = "scan.job.finished")]
    ScanJobFinished { scan_job: ScanJobResponse },
    #[serde(rename = "library.updated")]
    LibraryUpdated { library_id: i64 },
    #[serde(rename = "library.deleted")]
    LibraryDeleted { library_id: i64 },
    #[serde(rename = "media_item.metadata.updated")]
    MediaItemMetadataUpdated {
        library_id: i64,
        media_item_id: i64,
    },
}

impl RealtimeEventResponse {
    fn from_event(event: &RealtimeEvent, api_time_offset: UtcOffset) -> Self {
        match event {
            RealtimeEvent::ScanJobUpdated { scan_job } => Self::ScanJobUpdated {
                scan_job: ScanJobResponse::from_domain(scan_job.clone(), api_time_offset),
            },
            RealtimeEvent::ScanJobFinished { scan_job } => Self::ScanJobFinished {
                scan_job: ScanJobResponse::from_domain(scan_job.clone(), api_time_offset),
            },
            RealtimeEvent::LibraryUpdated { library_id } => Self::LibraryUpdated {
                library_id: *library_id,
            },
            RealtimeEvent::LibraryDeleted { library_id } => Self::LibraryDeleted {
                library_id: *library_id,
            },
            RealtimeEvent::MediaItemMetadataUpdated {
                library_id,
                media_item_id,
            } => Self::MediaItemMetadataUpdated {
                library_id: *library_id,
                media_item_id: *media_item_id,
            },
        }
    }
}
