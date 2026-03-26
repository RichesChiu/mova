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
use tokio::sync::{watch, Notify};

/// 通过 Axum state 注入到各个 handler 的共享依赖。
#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub api_time_offset: UtcOffset,
    pub artwork_cache_dir: PathBuf,
    pub metadata_provider: Arc<dyn mova_application::MetadataProvider>,
    pub scan_registry: ScanRegistry,
    pub library_sync_registry: LibrarySyncRegistry,
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

/// 记录文件 watcher 和访问兜底校准状态。
#[derive(Clone, Default)]
pub struct LibrarySyncRegistry {
    inner: Arc<Mutex<LibrarySyncRegistryInner>>,
}

#[derive(Default)]
struct LibrarySyncRegistryInner {
    watchers: HashMap<i64, watch::Sender<bool>>,
    dirty_libraries: HashSet<i64>,
    active_syncs: HashSet<i64>,
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

impl LibrarySyncRegistry {
    pub fn replace_watcher(
        &self,
        library_id: i64,
        stop_tx: watch::Sender<bool>,
    ) -> Option<watch::Sender<bool>> {
        let mut inner = self
            .inner
            .lock()
            .expect("library sync registry lock poisoned");
        inner.watchers.insert(library_id, stop_tx)
    }

    pub fn stop_watcher(&self, library_id: i64) {
        let stop_tx = {
            let mut inner = self
                .inner
                .lock()
                .expect("library sync registry lock poisoned");
            inner.watchers.remove(&library_id)
        };

        if let Some(stop_tx) = stop_tx {
            let _ = stop_tx.send(true);
        }
    }

    pub fn mark_dirty(&self, library_id: i64) {
        let mut inner = self
            .inner
            .lock()
            .expect("library sync registry lock poisoned");
        inner.dirty_libraries.insert(library_id);
    }

    pub fn record_reconciled(&self, library_id: i64) {
        let mut inner = self
            .inner
            .lock()
            .expect("library sync registry lock poisoned");
        inner.dirty_libraries.remove(&library_id);
    }

    pub fn begin_sync(&self, library_id: i64) -> bool {
        let mut inner = self
            .inner
            .lock()
            .expect("library sync registry lock poisoned");
        if inner.active_syncs.contains(&library_id) {
            return false;
        }
        inner.active_syncs.insert(library_id);
        true
    }

    pub fn finish_sync(&self, library_id: i64, success: bool) {
        let mut inner = self
            .inner
            .lock()
            .expect("library sync registry lock poisoned");
        inner.active_syncs.remove(&library_id);
        if success {
            inner.dirty_libraries.remove(&library_id);
        } else {
            inner.dirty_libraries.insert(library_id);
        }
    }

    pub fn clear_library(&self, library_id: i64) {
        self.stop_watcher(library_id);
        let mut inner = self
            .inner
            .lock()
            .expect("library sync registry lock poisoned");
        inner.active_syncs.remove(&library_id);
        inner.dirty_libraries.remove(&library_id);
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
    use super::{BeginDeleteError, LibrarySyncRegistry, RegisterScanError, ScanRegistry};

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

    #[test]
    fn begin_sync_blocks_duplicate_in_progress_syncs() {
        let registry = LibrarySyncRegistry::default();

        assert!(registry.begin_sync(5));
        assert!(!registry.begin_sync(5));

        registry.finish_sync(5, true);

        assert!(registry.begin_sync(5));
    }
}
