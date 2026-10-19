use sqlx::postgres::PgPool;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
use time::UtcOffset;
use tokio::sync::Notify;

pub use crate::auth_rate_limit::AuthRateLimiter;
pub use crate::realtime::{RealtimeDispatcherHandle, RealtimeHub};

/// 通过 Axum state 注入到各个 handler 的共享依赖。
#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub api_time_offset: UtcOffset,
    pub build_version: String,
    pub cache_dir: PathBuf,
    pub metadata_provider: Arc<dyn mova_application::MetadataProvider>,
    pub session_cookie_secure: bool,
    pub auth_rate_limiter: AuthRateLimiter,
    pub scan_registry: ScanRegistry,
    pub realtime_hub: RealtimeHub,
    pub realtime_dispatcher: RealtimeDispatcherHandle,
    pub background_jobs: BackgroundJobNotifier,
    pub strm_streaming: mova_application::StrmStreamingService,
    pub storage_environment: Arc<dyn mova_application::LibraryStorageEnvironment>,
    pub media_inspections: MediaInspectionThrottle,
}

/// Keeps automatic scans triggered by opening items from piling up: one
/// scoped scan request per library directory within the cooldown. Entries
/// live in memory only; a restart simply allows the next request.
#[derive(Clone, Default)]
pub struct MediaInspectionThrottle {
    inner: Arc<std::sync::Mutex<std::collections::HashMap<(i64, String), std::time::Instant>>>,
}

impl MediaInspectionThrottle {
    pub const COOLDOWN: std::time::Duration = std::time::Duration::from_secs(600);

    /// Returns the directories that may be handed to a scan now and records
    /// them; directories requested within the cooldown are left out.
    pub fn admit(&self, library_id: i64, directories: &[String]) -> Vec<String> {
        let now = std::time::Instant::now();
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        inner.retain(|_, requested_at| now.duration_since(*requested_at) < Self::COOLDOWN);
        directories
            .iter()
            .filter(|directory| {
                let key = (library_id, (*directory).clone());
                if inner.contains_key(&key) {
                    return false;
                }
                inner.insert(key, now);
                true
            })
            .cloned()
            .collect()
    }
}

#[derive(Clone, Default)]
pub struct BackgroundJobNotifier {
    notify: Arc<Notify>,
}

impl BackgroundJobNotifier {
    pub fn wake(&self) {
        self.notify.notify_waiters();
    }

    pub async fn wait(&self) {
        self.notify.notified().await;
    }
}

/// 记录当前进程内活跃扫描任务和正在删除的媒体库。
#[derive(Clone, Default)]
pub struct ScanRegistry {
    inner: Arc<Mutex<ScanRegistryInner>>,
}

#[derive(Default)]
struct ScanRegistryInner {
    active_scans: HashMap<i64, ActiveScan>,
    deleting_libraries: HashSet<i64>,
}

#[derive(Clone)]
pub struct ActiveScanHandle {
    scan_job_id: i64,
    cancel_flag: Arc<AtomicBool>,
    finished: Arc<AtomicBool>,
    notify: Arc<Notify>,
}

#[derive(Clone)]
struct ActiveScan {
    scan_job_id: i64,
    cancel_flag: Arc<AtomicBool>,
    finished: Arc<AtomicBool>,
    notify: Arc<Notify>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BeginDeleteError {
    AlreadyDeleting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegisterScanError {
    DeleteInProgress,
    AlreadyRunning,
}

pub struct DeleteGuard {
    registry: ScanRegistry,
    library_id: i64,
}

impl ScanRegistry {
    /// 标记媒体库进入删除流程，后续新的扫描启动请求会被拒绝。
    pub fn begin_delete(&self, library_id: i64) -> Result<DeleteGuard, BeginDeleteError> {
        let mut inner = self.inner.lock().expect("scan registry lock poisoned");
        if !inner.deleting_libraries.insert(library_id) {
            return Err(BeginDeleteError::AlreadyDeleting);
        }

        Ok(DeleteGuard {
            registry: self.clone(),
            library_id,
        })
    }

    /// 判断某个媒体库当前是否处于删除流程中。
    pub fn is_deleting(&self, library_id: i64) -> bool {
        let inner = self.inner.lock().expect("scan registry lock poisoned");
        inner.deleting_libraries.contains(&library_id)
    }

    /// 为新启动的扫描任务创建取消控制句柄。
    pub fn register_scan(
        &self,
        library_id: i64,
        scan_job_id: i64,
    ) -> Result<ActiveScanHandle, RegisterScanError> {
        let mut inner = self.inner.lock().expect("scan registry lock poisoned");
        if inner.deleting_libraries.contains(&library_id) {
            return Err(RegisterScanError::DeleteInProgress);
        }
        if inner.active_scans.contains_key(&library_id) {
            return Err(RegisterScanError::AlreadyRunning);
        }

        let cancel_flag = Arc::new(AtomicBool::new(false));
        let finished = Arc::new(AtomicBool::new(false));
        let notify = Arc::new(Notify::new());

        inner.active_scans.insert(
            library_id,
            ActiveScan {
                scan_job_id,
                cancel_flag: cancel_flag.clone(),
                finished: finished.clone(),
                notify: notify.clone(),
            },
        );

        Ok(ActiveScanHandle {
            scan_job_id,
            cancel_flag,
            finished,
            notify,
        })
    }

    /// 读取当前库正在执行的扫描任务控制句柄。
    pub fn active_scan(&self, library_id: i64) -> Option<ActiveScanHandle> {
        let inner = self.inner.lock().expect("scan registry lock poisoned");
        let scan = inner.active_scans.get(&library_id)?.clone();

        Some(ActiveScanHandle {
            scan_job_id: scan.scan_job_id,
            cancel_flag: scan.cancel_flag,
            finished: scan.finished,
            notify: scan.notify,
        })
    }

    /// 扫描任务结束时移除注册表里的活跃状态，并唤醒等待删除的人。
    pub fn finish_scan(&self, library_id: i64, scan_job_id: i64) {
        let active_scan = {
            let mut inner = self.inner.lock().expect("scan registry lock poisoned");
            let should_remove = inner
                .active_scans
                .get(&library_id)
                .map(|scan| scan.scan_job_id == scan_job_id)
                .unwrap_or(false);

            if should_remove {
                inner.active_scans.remove(&library_id)
            } else {
                None
            }
        };

        if let Some(scan) = active_scan {
            if !scan.finished.swap(true, Ordering::SeqCst) {
                scan.notify.notify_waiters();
            }
        }
    }

    fn end_delete(&self, library_id: i64) {
        let mut inner = self.inner.lock().expect("scan registry lock poisoned");
        inner.deleting_libraries.remove(&library_id);
    }
}

impl ActiveScanHandle {
    pub fn scan_job_id(&self) -> i64 {
        self.scan_job_id
    }

    pub fn cancel(&self) {
        self.cancel_flag.store(true, Ordering::SeqCst);
    }

    pub fn cancellation_flag(&self) -> Arc<AtomicBool> {
        self.cancel_flag.clone()
    }

    pub async fn wait_finished(&self) {
        if self.finished.load(Ordering::SeqCst) {
            return;
        }

        self.notify.notified().await;
    }
}

impl Drop for DeleteGuard {
    fn drop(&mut self) {
        self.registry.end_delete(self.library_id);
    }
}

#[cfg(test)]
mod tests {
    use super::{BeginDeleteError, RegisterScanError, ScanRegistry};

    #[tokio::test]
    async fn begin_delete_blocks_new_scan_registration() {
        let registry = ScanRegistry::default();
        let _guard = registry.begin_delete(7).unwrap();

        let result = registry.register_scan(7, 11);

        assert!(matches!(result, Err(RegisterScanError::DeleteInProgress)));
        assert!(registry.is_deleting(7));
    }

    #[tokio::test]
    async fn finish_scan_wakes_waiters() {
        let registry = ScanRegistry::default();
        let handle = registry.register_scan(9, 21).unwrap();
        let waiter = handle.clone();

        let wait_task = tokio::spawn(async move {
            waiter.wait_finished().await;
            waiter.scan_job_id()
        });

        registry.finish_scan(9, 21);

        let result = wait_task.await.unwrap();
        assert_eq!(result, 21);
    }

    #[test]
    fn begin_delete_rejects_duplicate_delete_requests() {
        let registry = ScanRegistry::default();
        let _guard = registry.begin_delete(3).unwrap();

        let result = registry.begin_delete(3);

        assert_eq!(result.err(), Some(BeginDeleteError::AlreadyDeleting));
    }
}

#[cfg(test)]
mod media_inspection_throttle_tests {
    use super::MediaInspectionThrottle;

    #[test]
    fn a_directory_is_admitted_once_per_cooldown_per_library() {
        let throttle = MediaInspectionThrottle::default();
        let alpha = "/media/movies/Alpha (2020)".to_string();
        let beta = "/media/movies/Beta (2021)".to_string();

        assert_eq!(
            throttle.admit(1, &[alpha.clone(), beta.clone()]),
            vec![alpha.clone(), beta.clone()]
        );
        // Asked again right away: nothing new to hand to a scan.
        assert!(throttle.admit(1, std::slice::from_ref(&alpha)).is_empty());
        // Another library has its own cooldown.
        assert_eq!(
            throttle.admit(2, std::slice::from_ref(&alpha)),
            vec![alpha.clone()]
        );
        // A new directory of the first library is still admitted.
        let gamma = "/media/movies/Gamma (2022)".to_string();
        assert_eq!(throttle.admit(1, &[alpha, gamma.clone()]), vec![gamma]);
    }
}
