use crate::response::{ScanItemProgressResponse, ScanJobResponse};
use axum::response::sse::Event;
use mova_application::{ScanJobItemProgressUpdate, ScanJobProgressUpdate};
use mova_domain::UserProfile;
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
    ScanJobUpdated { update: ScanJobProgressUpdate },
    ScanJobFinished { update: ScanJobProgressUpdate },
    ScanItemUpdated { item: ScanJobItemProgressUpdate },
    LibraryUpdated { library_id: i64 },
    LibraryDeleted { library_id: i64 },
    MediaItemMetadataUpdated { library_id: i64, media_item_id: i64 },
}

impl RealtimeEvent {
    pub fn event_name(&self) -> &'static str {
        match self {
            Self::ScanJobUpdated { .. } => "scan.job.updated",
            Self::ScanJobFinished { .. } => "scan.job.finished",
            Self::ScanItemUpdated { .. } => "scan.item.updated",
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
            Self::ScanJobUpdated { update } | Self::ScanJobFinished { update } => {
                update.scan_job.library_id
            }
            Self::ScanItemUpdated { item } => item.library_id,
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
    #[serde(rename = "scan.item.updated")]
    ScanItemUpdated { item: ScanItemProgressResponse },
    #[serde(rename = "library.updated")]
    LibraryUpdated { library_id: i64 },
    #[serde(rename = "library.deleted")]
    LibraryDeleted { library_id: i64 },
    #[serde(rename = "media_item.metadata.updated")]
    MediaItemMetadataUpdated { library_id: i64, media_item_id: i64 },
}

impl RealtimeEventResponse {
    fn from_event(event: &RealtimeEvent, api_time_offset: UtcOffset) -> Self {
        match event {
            RealtimeEvent::ScanJobUpdated { update } => Self::ScanJobUpdated {
                scan_job: ScanJobResponse::from_realtime(
                    update.scan_job.clone(),
                    update.phase.clone(),
                    api_time_offset,
                ),
            },
            RealtimeEvent::ScanJobFinished { update } => Self::ScanJobFinished {
                scan_job: ScanJobResponse::from_realtime(
                    update.scan_job.clone(),
                    update.phase.clone(),
                    api_time_offset,
                ),
            },
            RealtimeEvent::ScanItemUpdated { item } => Self::ScanItemUpdated {
                item: ScanItemProgressResponse::from_domain(item.clone()),
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

#[cfg(test)]
mod tests {
    use super::RealtimeEvent;
    use mova_application::ScanJobItemProgressUpdate;
    use mova_domain::{User, UserProfile, UserRole};
    use time::{OffsetDateTime, UtcOffset};

    fn build_user_profile(role: UserRole, library_ids: Vec<i64>) -> UserProfile {
        UserProfile {
            user: User {
                id: 1,
                username: "viewer01".to_string(),
                nickname: "viewer01".to_string(),
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
    fn library_updated_event_uses_library_visibility_rules() {
        let viewer = build_user_profile(UserRole::Viewer, vec![7]);
        let outsider = build_user_profile(UserRole::Viewer, vec![8]);
        let event = RealtimeEvent::LibraryUpdated { library_id: 7 };

        assert_eq!(event.event_name(), "library.updated");
        assert!(event.is_visible_to(&viewer));
        assert!(!event.is_visible_to(&outsider));
    }

    #[test]
    fn scan_item_event_can_be_serialized_into_sse() {
        let event = RealtimeEvent::ScanItemUpdated {
            item: ScanJobItemProgressUpdate {
                scan_job_id: 41,
                library_id: 7,
                item_key: "/media/movies/interstellar.mkv".to_string(),
                media_type: "movie".to_string(),
                title: "Interstellar".to_string(),
                season_number: None,
                episode_number: None,
                item_index: 1,
                total_items: 3,
                stage: "metadata".to_string(),
                progress_percent: 36,
            },
        };

        assert_eq!(event.event_name(), "scan.item.updated");
        assert!(event.to_sse_event(UtcOffset::UTC).is_some());
    }
}
