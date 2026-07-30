use std::{
    collections::HashSet,
    io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
    time::SystemTime,
};
#[cfg(unix)]
use std::{ffi::CString, mem::MaybeUninit, os::unix::ffi::OsStrExt};
use tokio::{
    fs,
    sync::{Mutex as AsyncMutex, OwnedSemaphorePermit, Semaphore},
};

const GIBIBYTE: u64 = 1024 * 1024 * 1024;
const REMUX_OVERHEAD_ALLOWANCE_BYTES: u64 = 256 * 1024 * 1024;

/// An audio-track variant still contains the copied video stream, so the
/// artifact ceiling must accommodate large UHD remuxes.
pub const AUDIO_TRACK_CACHE_ARTIFACT_MAX_BYTES: u64 = 128 * GIBIBYTE;
pub const AUDIO_TRACK_CACHE_TOTAL_MAX_BYTES: u64 = 256 * GIBIBYTE;
pub const AUDIO_TRACK_CACHE_DISK_HEADROOM_BYTES: u64 = 5 * GIBIBYTE;
pub const AUDIO_TRACK_REMUX_CONCURRENCY: usize = 2;

type AvailableSpaceProbe = fn(&Path) -> io::Result<u64>;

#[derive(Clone)]
struct AudioTrackCacheCoordinator {
    inner: Arc<AudioTrackCacheCoordinatorInner>,
}

struct AudioTrackCacheCoordinatorInner {
    remux_slots: Arc<Semaphore>,
    maintenance: AsyncMutex<()>,
    state: Mutex<AudioTrackCacheState>,
    available_space_probe: AvailableSpaceProbe,
}

#[derive(Default)]
struct AudioTrackCacheState {
    initialized_roots: HashSet<PathBuf>,
    reserved_bytes: u64,
    active_temporary_paths: HashSet<PathBuf>,
}

pub struct AudioTrackCacheReservation {
    coordinator: AudioTrackCacheCoordinator,
    temporary_path: PathBuf,
    reserved_bytes: u64,
    _admission: AudioTrackRemuxAdmission,
}

pub struct AudioTrackRemuxAdmission {
    _remux_slot: OwnedSemaphorePermit,
}

impl Drop for AudioTrackCacheReservation {
    fn drop(&mut self) {
        let mut state = self
            .coordinator
            .inner
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.reserved_bytes = state.reserved_bytes.saturating_sub(self.reserved_bytes);
        state.active_temporary_paths.remove(&self.temporary_path);
    }
}

#[derive(Debug)]
struct CacheEntry {
    path: PathBuf,
    size: u64,
    modified: SystemTime,
}

impl AudioTrackCacheCoordinator {
    fn new(remux_concurrency: usize) -> Self {
        Self::new_with_space_probe(remux_concurrency, available_cache_space_bytes)
    }

    fn new_with_space_probe(
        remux_concurrency: usize,
        available_space_probe: AvailableSpaceProbe,
    ) -> Self {
        Self {
            inner: Arc::new(AudioTrackCacheCoordinatorInner {
                remux_slots: Arc::new(Semaphore::new(remux_concurrency.max(1))),
                maintenance: AsyncMutex::new(()),
                state: Mutex::new(AudioTrackCacheState::default()),
                available_space_probe,
            }),
        }
    }

    fn try_admit(&self) -> io::Result<AudioTrackRemuxAdmission> {
        let remux_slot = self
            .inner
            .remux_slots
            .clone()
            .try_acquire_owned()
            .map_err(|error| match error {
                tokio::sync::TryAcquireError::NoPermits => io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "audio-track remux capacity is busy",
                ),
                tokio::sync::TryAcquireError::Closed => {
                    io::Error::other("audio-track remux coordinator is closed")
                }
            })?;
        Ok(AudioTrackRemuxAdmission {
            _remux_slot: remux_slot,
        })
    }

    async fn ensure_initialized(&self, cache_root: &Path, quota_bytes: u64) -> io::Result<()> {
        {
            let state = self
                .inner
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if state.initialized_roots.contains(cache_root) {
                return Ok(());
            }
        }

        let _maintenance = self.inner.maintenance.lock().await;
        {
            let state = self
                .inner
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if state.initialized_roots.contains(cache_root) {
                return Ok(());
            }
        }

        prune_audio_track_cache_to_limit(cache_root, quota_bytes, &HashSet::new()).await?;

        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.initialized_roots.insert(cache_root.to_path_buf());
        Ok(())
    }

    async fn reserve(
        &self,
        cache_root: &Path,
        temporary_path: &Path,
        requested_bytes: u64,
        quota_bytes: u64,
        headroom_bytes: u64,
        admission: AudioTrackRemuxAdmission,
    ) -> io::Result<AudioTrackCacheReservation> {
        if requested_bytes == 0 || requested_bytes > quota_bytes {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "audio-track cache reservation exceeds its total quota",
            ));
        }

        self.ensure_initialized(cache_root, quota_bytes).await?;

        let _maintenance = self.inner.maintenance.lock().await;
        let (reserved_bytes, active_temporary_paths) = {
            let state = self
                .inner
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            (state.reserved_bytes, state.active_temporary_paths.clone())
        };
        let retained_limit = quota_bytes
            .checked_sub(reserved_bytes)
            .and_then(|remaining| remaining.checked_sub(requested_bytes))
            .ok_or_else(|| {
                io::Error::other("audio-track cache capacity is temporarily exhausted")
            })?;

        let retained_bytes =
            prune_audio_track_cache_to_limit(cache_root, retained_limit, &active_temporary_paths)
                .await?;
        ensure_cache_volume_capacity(
            cache_root,
            retained_bytes,
            reserved_bytes,
            requested_bytes,
            headroom_bytes,
            &active_temporary_paths,
            self.inner.available_space_probe,
        )
        .await?;

        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.reserved_bytes = state
            .reserved_bytes
            .checked_add(requested_bytes)
            .ok_or_else(|| io::Error::other("audio-track cache reservation overflow"))?;
        state
            .active_temporary_paths
            .insert(temporary_path.to_path_buf());

        Ok(AudioTrackCacheReservation {
            coordinator: self.clone(),
            temporary_path: temporary_path.to_path_buf(),
            reserved_bytes: requested_bytes,
            _admission: admission,
        })
    }
}

fn global_coordinator() -> &'static AudioTrackCacheCoordinator {
    static COORDINATOR: OnceLock<AudioTrackCacheCoordinator> = OnceLock::new();
    COORDINATOR.get_or_init(|| AudioTrackCacheCoordinator::new(AUDIO_TRACK_REMUX_CONCURRENCY))
}

/// Enforces the process-wide audio-track cache quota before the server starts
/// accepting requests. Failure is fatal so an existing uncontrolled cache is
/// never silently carried into a running process.
pub async fn initialize_audio_track_cache(cache_root: &Path) -> io::Result<()> {
    global_coordinator()
        .ensure_initialized(cache_root, AUDIO_TRACK_CACHE_TOTAL_MAX_BYTES)
        .await
}

/// Rejects excess work immediately instead of allowing HTTP requests to queue
/// behind long-running remuxes.
pub fn try_admit_audio_track_remux() -> io::Result<AudioTrackRemuxAdmission> {
    global_coordinator().try_admit()
}

/// Returns the maximum bytes this remux is allowed to publish. Sources whose
/// upper bound would exceed the absolute artifact ceiling are rejected before
/// FFmpeg starts, because `-fs` may otherwise report success for a truncated
/// output.
pub fn audio_track_remux_output_limit(source_size: u64) -> Option<u64> {
    source_size
        .checked_add(REMUX_OVERHEAD_ALLOWANCE_BYTES)
        .filter(|limit| *limit <= AUDIO_TRACK_CACHE_ARTIFACT_MAX_BYTES)
}

pub async fn reserve_audio_track_cache(
    cache_root: &Path,
    temporary_path: &Path,
    requested_bytes: u64,
    admission: AudioTrackRemuxAdmission,
) -> io::Result<AudioTrackCacheReservation> {
    global_coordinator()
        .reserve(
            cache_root,
            temporary_path,
            requested_bytes,
            AUDIO_TRACK_CACHE_TOTAL_MAX_BYTES,
            AUDIO_TRACK_CACHE_DISK_HEADROOM_BYTES,
            admission,
        )
        .await
}

pub async fn cache_artifact_is_usable(path: &Path) -> io::Result<bool> {
    cache_artifact_is_usable_with_limit(path, AUDIO_TRACK_CACHE_ARTIFACT_MAX_BYTES).await
}

pub fn generated_artifact_size_is_complete(generated_bytes: u64, output_limit: u64) -> bool {
    generated_bytes > 0 && generated_bytes < output_limit
}

async fn cache_artifact_is_usable_with_limit(path: &Path, max_bytes: u64) -> io::Result<bool> {
    match fs::metadata(path).await {
        Ok(metadata) => Ok(metadata.is_file() && metadata.len() > 0 && metadata.len() <= max_bytes),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

fn required_available_space(
    reserved_bytes: u64,
    requested_bytes: u64,
    headroom_bytes: u64,
) -> io::Result<u64> {
    headroom_bytes
        .checked_add(reserved_bytes)
        .and_then(|required| required.checked_add(requested_bytes))
        .ok_or_else(|| io::Error::other("audio-track cache disk reservation overflow"))
}

async fn ensure_cache_volume_capacity(
    cache_root: &Path,
    retained_bytes: u64,
    reserved_bytes: u64,
    requested_bytes: u64,
    headroom_bytes: u64,
    protected_paths: &HashSet<PathBuf>,
    available_space_probe: AvailableSpaceProbe,
) -> io::Result<()> {
    let required_available =
        required_available_space(reserved_bytes, requested_bytes, headroom_bytes)?;
    let available = available_space_probe(cache_root)?;
    if available >= required_available {
        return Ok(());
    }

    let bytes_to_free = required_available - available;
    let retained_limit = retained_bytes.saturating_sub(bytes_to_free);
    prune_audio_track_cache_to_limit(cache_root, retained_limit, protected_paths).await?;

    let available_after_eviction = available_space_probe(cache_root)?;
    if available_after_eviction < required_available {
        return Err(io::Error::other(format!(
            "audio-track cache volume has {} bytes available but {} bytes are required",
            available_after_eviction, required_available
        )));
    }

    Ok(())
}

#[cfg(unix)]
fn available_cache_space_bytes(path: &Path) -> io::Result<u64> {
    let path = CString::new(path.as_os_str().as_bytes()).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "cache path contains an embedded null byte",
        )
    })?;
    let mut stats = MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: `path` is a valid NUL-terminated C string and `stats` points to
    // writable storage for one `statvfs` value. We only assume initialization
    // after libc reports success.
    if unsafe { libc::statvfs(path.as_ptr(), stats.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the successful `statvfs` call initialized the output value.
    let stats = unsafe { stats.assume_init() };
    let available_blocks = u128::from(stats.f_bavail);
    let fragment_size = u128::from(stats.f_frsize);
    u64::try_from(available_blocks.saturating_mul(fragment_size))
        .map_err(|_| io::Error::other("cache volume available space exceeds u64"))
}

#[cfg(not(unix))]
fn available_cache_space_bytes(_path: &Path) -> io::Result<u64> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "audio-track cache disk capacity checks require statvfs",
    ))
}

async fn collect_audio_track_cache_entries(
    cache_root: &Path,
    protected_paths: &HashSet<PathBuf>,
) -> io::Result<Vec<CacheEntry>> {
    let libraries_root = cache_root.join("libraries");
    let mut libraries = match fs::read_dir(&libraries_root).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut entries = Vec::new();

    while let Some(library) = libraries.next_entry().await? {
        if !library.file_type().await?.is_dir() {
            continue;
        }
        let audio_track_dir = library.path().join("audio-tracks");
        let mut files = match fs::read_dir(&audio_track_dir).await {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };

        while let Some(file) = files.next_entry().await? {
            let path = file.path();
            if protected_paths.contains(&path) || !file.file_type().await?.is_file() {
                continue;
            }
            let metadata = file.metadata().await?;
            entries.push(CacheEntry {
                path,
                size: metadata.len(),
                modified: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            });
        }
    }

    Ok(entries)
}

async fn prune_audio_track_cache_to_limit(
    cache_root: &Path,
    retained_limit: u64,
    protected_paths: &HashSet<PathBuf>,
) -> io::Result<u64> {
    let mut entries = collect_audio_track_cache_entries(cache_root, protected_paths).await?;
    let mut retained_bytes = entries
        .iter()
        .fold(0_u64, |total, entry| total.saturating_add(entry.size));
    entries.sort_by(|left, right| {
        left.modified
            .cmp(&right.modified)
            .then_with(|| left.path.cmp(&right.path))
    });

    for entry in entries {
        if retained_bytes <= retained_limit {
            break;
        }
        match fs::remove_file(&entry.path).await {
            Ok(()) => {
                retained_bytes = retained_bytes.saturating_sub(entry.size);
                tracing::debug!(
                    cache_path = %entry.path.display(),
                    cache_bytes = entry.size,
                    "evicted audio-track cache artifact"
                );
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                retained_bytes = retained_bytes.saturating_sub(entry.size);
            }
            Err(error) => return Err(error),
        }
    }

    if retained_bytes > retained_limit {
        return Err(io::Error::other(
            "audio-track cache quota cannot be satisfied",
        ));
    }

    Ok(retained_bytes)
}

#[cfg(test)]
mod tests {
    use super::{
        audio_track_remux_output_limit, cache_artifact_is_usable_with_limit,
        generated_artifact_size_is_complete, prune_audio_track_cache_to_limit,
        required_available_space, AudioTrackCacheCoordinator, AUDIO_TRACK_CACHE_ARTIFACT_MAX_BYTES,
        REMUX_OVERHEAD_ALLOWANCE_BYTES,
    };
    use std::{
        collections::HashSet,
        fs::{FileTimes, OpenOptions},
        path::Path,
        time::{Duration, SystemTime},
    };
    use uuid::Uuid;

    fn test_cache_root() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("mova-audio-cache-test-{}", Uuid::new_v4()))
    }

    fn ample_space(_path: &Path) -> std::io::Result<u64> {
        Ok(u64::MAX)
    }

    fn eight_bytes_available(_path: &Path) -> std::io::Result<u64> {
        Ok(8)
    }

    async fn write_cache_file(root: &Path, name: &str, size: usize, age_seconds: u64) {
        let directory = root.join("libraries/1/audio-tracks");
        tokio::fs::create_dir_all(&directory).await.unwrap();
        let path = directory.join(name);
        tokio::fs::write(&path, vec![b'x'; size]).await.unwrap();
        let modified = SystemTime::now()
            .checked_sub(Duration::from_secs(age_seconds))
            .unwrap();
        OpenOptions::new()
            .write(true)
            .open(path)
            .unwrap()
            .set_times(FileTimes::new().set_modified(modified))
            .unwrap();
    }

    #[test]
    fn remux_output_limit_rejects_sources_that_cannot_fit_below_the_artifact_ceiling() {
        assert_eq!(
            audio_track_remux_output_limit(1024),
            Some(1024 + REMUX_OVERHEAD_ALLOWANCE_BYTES)
        );
        assert_eq!(
            audio_track_remux_output_limit(
                AUDIO_TRACK_CACHE_ARTIFACT_MAX_BYTES - REMUX_OVERHEAD_ALLOWANCE_BYTES
            ),
            Some(AUDIO_TRACK_CACHE_ARTIFACT_MAX_BYTES)
        );
        assert_eq!(
            audio_track_remux_output_limit(
                AUDIO_TRACK_CACHE_ARTIFACT_MAX_BYTES - REMUX_OVERHEAD_ALLOWANCE_BYTES + 1
            ),
            None
        );
        assert_eq!(audio_track_remux_output_limit(u64::MAX), None);
    }

    #[test]
    fn generated_artifact_must_not_reach_the_ffmpeg_file_size_boundary() {
        assert!(!generated_artifact_size_is_complete(0, 100));
        assert!(generated_artifact_size_is_complete(99, 100));
        assert!(!generated_artifact_size_is_complete(100, 100));
        assert!(!generated_artifact_size_is_complete(101, 100));
    }

    #[test]
    fn disk_capacity_requirement_includes_headroom_and_in_flight_reservations() {
        assert_eq!(required_available_space(6, 3, 5).unwrap(), 14);
        assert!(required_available_space(u64::MAX, 1, 0).is_err());
    }

    #[tokio::test]
    async fn artifact_validation_rejects_empty_and_oversized_files() {
        let root = test_cache_root();
        let empty = root.join("empty.mp4");
        let valid = root.join("valid.mp4");
        let oversized = root.join("oversized.mp4");
        tokio::fs::create_dir_all(&root).await.unwrap();
        tokio::fs::write(&empty, []).await.unwrap();
        tokio::fs::write(&valid, [1_u8; 4]).await.unwrap();
        tokio::fs::write(&oversized, [1_u8; 5]).await.unwrap();

        assert!(!cache_artifact_is_usable_with_limit(&empty, 4)
            .await
            .unwrap());
        assert!(cache_artifact_is_usable_with_limit(&valid, 4)
            .await
            .unwrap());
        assert!(!cache_artifact_is_usable_with_limit(&oversized, 4)
            .await
            .unwrap());

        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn quota_pruning_removes_oldest_artifacts_and_stale_temporary_files() {
        let root = test_cache_root();
        write_cache_file(&root, "old.mp4", 4, 30).await;
        write_cache_file(&root, ".stale.tmp.mp4", 4, 20).await;
        write_cache_file(&root, "new.mp4", 4, 10).await;

        let retained = prune_audio_track_cache_to_limit(&root, 4, &HashSet::new())
            .await
            .unwrap();

        assert_eq!(retained, 4);
        assert!(
            tokio::fs::metadata(root.join("libraries/1/audio-tracks/old.mp4"))
                .await
                .is_err()
        );
        assert!(
            tokio::fs::metadata(root.join("libraries/1/audio-tracks/.stale.tmp.mp4"))
                .await
                .is_err()
        );
        assert!(
            tokio::fs::metadata(root.join("libraries/1/audio-tracks/new.mp4"))
                .await
                .is_ok()
        );

        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn quota_pruning_never_removes_an_active_temporary_file() {
        let root = test_cache_root();
        write_cache_file(&root, "old.mp4", 4, 30).await;
        write_cache_file(&root, ".active.tmp.mp4", 4, 20).await;
        let active = root.join("libraries/1/audio-tracks/.active.tmp.mp4");

        let retained = prune_audio_track_cache_to_limit(&root, 0, &HashSet::from([active.clone()]))
            .await
            .unwrap();

        assert_eq!(retained, 0);
        assert!(tokio::fs::metadata(active).await.is_ok());
        assert!(
            tokio::fs::metadata(root.join("libraries/1/audio-tracks/old.mp4"))
                .await
                .is_err()
        );

        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn startup_initialization_prunes_an_existing_cache_to_its_quota() {
        let root = test_cache_root();
        write_cache_file(&root, "old.mp4", 4, 20).await;
        write_cache_file(&root, "new.mp4", 4, 10).await;
        let coordinator = AudioTrackCacheCoordinator::new_with_space_probe(1, ample_space);

        coordinator.ensure_initialized(&root, 4).await.unwrap();

        assert!(
            tokio::fs::metadata(root.join("libraries/1/audio-tracks/old.mp4"))
                .await
                .is_err()
        );
        assert!(
            tokio::fs::metadata(root.join("libraries/1/audio-tracks/new.mp4"))
                .await
                .is_ok()
        );

        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn coordinator_rejects_excess_remux_work_without_queueing() {
        let coordinator = AudioTrackCacheCoordinator::new_with_space_probe(1, ample_space);
        let first = coordinator.try_admit().unwrap();

        let error = coordinator
            .try_admit()
            .err()
            .expect("second admission must be rejected immediately");
        assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock);

        drop(first);
        let second = coordinator.try_admit().unwrap();
        drop(second);
    }

    #[tokio::test]
    async fn coordinator_accounts_for_in_flight_reservations_against_the_total_quota() {
        let root = test_cache_root();
        let coordinator = AudioTrackCacheCoordinator::new_with_space_probe(2, ample_space);
        let first_admission = coordinator.try_admit().unwrap();
        let first = coordinator
            .reserve(
                &root,
                &root.join(".first.tmp.mp4"),
                6,
                8,
                0,
                first_admission,
            )
            .await
            .unwrap();

        let second_admission = coordinator.try_admit().unwrap();
        let error = coordinator
            .reserve(
                &root,
                &root.join(".second.tmp.mp4"),
                3,
                8,
                0,
                second_admission,
            )
            .await
            .err()
            .expect("second reservation must exceed the shared quota");
        assert_eq!(error.kind(), std::io::ErrorKind::Other);

        drop(first);
        let second_admission = coordinator.try_admit().unwrap();
        let second = coordinator
            .reserve(
                &root,
                &root.join(".second.tmp.mp4"),
                3,
                8,
                0,
                second_admission,
            )
            .await
            .unwrap();
        drop(second);

        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn coordinator_rejects_a_reservation_that_would_consume_disk_headroom() {
        let root = test_cache_root();
        let coordinator =
            AudioTrackCacheCoordinator::new_with_space_probe(1, eight_bytes_available);
        let admission = coordinator.try_admit().unwrap();

        let error = coordinator
            .reserve(&root, &root.join(".audio.tmp.mp4"), 4, 8, 5, admission)
            .await
            .err()
            .expect("five bytes of headroom plus four reserved bytes need nine bytes");
        assert_eq!(error.kind(), std::io::ErrorKind::Other);

        let _ = tokio::fs::remove_dir_all(root).await;
    }
}
