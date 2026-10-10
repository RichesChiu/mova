//! Library storage checks.
//!
//! A scan may treat a catalog path as deleted only after confirming that the
//! storage behind the library root is connected. Each library records the
//! mounts it lives on: the mount holding its root and every mount below it.
//!
//! Only the library root gets the benefit of the doubt. Its mount is judged
//! against the record: healthy when the filesystem and source are unchanged,
//! or changed storage still holds the library's known files, and the root is
//! readable within the probe timeout; removed when the mount point is gone
//! from the container's mount table; abnormal otherwise. Abnormal network
//! storage holding the root makes the library unavailable: nothing is scanned
//! and nothing is deleted until it is back or an admin deletes the library.
//!
//! Everything below the root, including separately mounted folders, is
//! judged by what can be read now: a mount below the root that cannot be read
//! in time, and any path that fails to read, is treated as nonexistent, so
//! the media on it is removed. A library whose every recorded mount was
//! removed from the deployment is deleted at startup.

use crate::error::{
    ApplicationError, ApplicationResult, BusinessError, BusinessErrorKind, BusinessErrorParams,
};
use mova_db::LibraryStorageMountRecord;
use mova_domain::{Library, LibraryStorageIssue};
use mova_scan::{
    catalog_path_presence, known_file_presence, probe_directory, CatalogPathPresence,
    DiscoveryFailureDecision, MountEntry, MountTable,
};
use serde_json::Value;
use sqlx::postgres::PgPool;
use std::{
    collections::BTreeMap,
    io,
    path::{Path, PathBuf},
    time::Duration,
};

pub const STORAGE_NOT_CONNECTED: &str = "storage_not_connected";
pub const STORAGE_UNREADABLE: &str = "storage_unreadable";
pub const STORAGE_TIMEOUT: &str = "storage_timeout";
pub const STORAGE_UNVERIFIED: &str = "storage_unverified";
pub const MOUNT_TABLE_UNAVAILABLE: &str = "mount_table_unavailable";
pub const STORAGE_REMOVED_FROM_DEPLOYMENT: &str = "storage_removed_from_deployment";
pub const LIBRARY_STORAGE_UNAVAILABLE_ERROR: &str = "library_storage_unavailable";

const STORAGE_PROBE_TIMEOUT: Duration = Duration::from_secs(20);
const MISSING_PATH_LOOKUP_TIMEOUT: Duration = Duration::from_secs(600);
const KNOWN_FILE_SAMPLE_LIMIT: i64 = 16;

/// Where storage facts come from. Production reads the process mount table;
/// tests describe mounts over real temporary directories.
pub trait LibraryStorageEnvironment: Send + Sync {
    fn mount_table(&self) -> io::Result<MountTable>;
    fn in_container(&self) -> bool;
}

#[derive(Debug, Default)]
pub struct HostLibraryStorageEnvironment;

impl LibraryStorageEnvironment for HostLibraryStorageEnvironment {
    fn mount_table(&self) -> io::Result<MountTable> {
        MountTable::read_current()
    }

    fn in_container(&self) -> bool {
        mova_scan::running_in_container()
    }
}

/// The outcome of a storage check that callers outside this crate act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LibraryStorageCheck {
    Available,
    Unavailable(LibraryStorageIssue),
    /// Every folder the library was scanned from is gone from the deployment.
    RemovedFromDeployment {
        mount_points: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RegionVerdict {
    Healthy,
    Removed,
    Abnormal(LibraryStorageIssue),
}

impl RegionVerdict {
    fn is_dead(&self) -> bool {
        !matches!(self, Self::Healthy)
    }

    fn kind(&self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Removed => "removed",
            Self::Abnormal(_) => "abnormal",
        }
    }
}

/// One mount the library lives on, as recorded and as currently observed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StorageRegion {
    pub(crate) mount_point: PathBuf,
    pub(crate) recorded: Option<MountEntry>,
    pub(crate) current: Option<MountEntry>,
    pub(crate) holds_root: bool,
    pub(crate) verdict: RegionVerdict,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StorageAssessment {
    Unavailable(LibraryStorageIssue),
    RemovedFromDeployment { mount_points: Vec<String> },
    Available(StoragePlan),
}

/// What a scan may trust about the library's storage this run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StoragePlan {
    root_path: PathBuf,
    regions: Vec<StorageRegion>,
    root_missing: bool,
}

impl StoragePlan {
    fn root_region(&self) -> &StorageRegion {
        self.regions
            .iter()
            .find(|region| region.holds_root)
            .expect("a storage plan always has a root region")
    }

    /// The type that decides whether failures under the root are network
    /// outages: the recorded one when the library has a record.
    pub(crate) fn root_is_network(&self) -> bool {
        let root_region = self.root_region();
        root_region
            .recorded
            .as_ref()
            .or(root_region.current.as_ref())
            .is_some_and(MountEntry::is_network)
    }

    /// Nothing can be traversed when local storage holding the root is
    /// abnormal, or when the root folder itself is gone from healthy storage.
    pub(crate) fn skips_traversal(&self) -> bool {
        self.root_missing || matches!(self.root_region().verdict, RegionVerdict::Abnormal(_))
    }

    /// Dead mounts below the root, skipped as nonexistent during traversal.
    pub(crate) fn excluded_directories(&self) -> Vec<PathBuf> {
        self.regions
            .iter()
            .filter(|region| !region.holds_root && region.verdict.is_dead())
            .map(|region| region.mount_point.clone())
            .collect()
    }

    fn region_holding(&self, path: &Path) -> &StorageRegion {
        self.regions
            .iter()
            .filter(|region| path.starts_with(&region.mount_point))
            .max_by_key(|region| region.mount_point.components().count())
            .unwrap_or_else(|| self.root_region())
    }

    /// Whether a catalog path lives on storage treated as nonexistent.
    pub(crate) fn is_dead_path(&self, path: &Path) -> bool {
        self.region_holding(path).verdict.is_dead()
    }

    /// Only a failure to read the library root itself on connected network
    /// storage is an outage: the scan stops and nothing is deleted. Any path
    /// below the root that fails to read is treated as nonexistent.
    pub(crate) fn failure_decision(&self, path: &Path) -> DiscoveryFailureDecision {
        if path == self.root_path
            && self.root_region().verdict == RegionVerdict::Healthy
            && self.root_is_network()
        {
            DiscoveryFailureDecision::Abort
        } else {
            DiscoveryFailureDecision::SkipPath
        }
    }

    pub(crate) fn outage_issue(&self, path: &Path, error: &io::Error) -> LibraryStorageIssue {
        let root_region = self.root_region();
        storage_issue(
            STORAGE_UNREADABLE,
            &root_region.mount_point,
            root_region.recorded.as_ref(),
            root_region.current.as_ref(),
            Some(format!("{}: {error}", path.display())),
        )
    }

    /// The record a successful scan stores: the storage it actually read.
    /// Dead mounts below the root are dropped with their media. An abnormal
    /// local root keeps its previous record, so its return is recognized.
    pub(crate) fn storage_record(&self) -> Vec<LibraryStorageMountRecord> {
        let root_abnormal = matches!(self.root_region().verdict, RegionVerdict::Abnormal(_));
        let current_root = self
            .regions
            .iter()
            .filter(|region| region.current.is_some())
            .filter(|region| self.root_path.starts_with(&region.mount_point))
            .max_by_key(|region| region.mount_point.components().count())
            .map(|region| region.mount_point.clone());
        self.regions
            .iter()
            .filter_map(|region| {
                let below_root = region.mount_point != self.root_path
                    && region.mount_point.starts_with(&self.root_path);
                if region.holds_root && root_abnormal {
                    region.recorded.as_ref()
                } else if region.verdict == RegionVerdict::Healthy
                    && (below_root
                        || (!root_abnormal && current_root.as_ref() == Some(&region.mount_point)))
                {
                    region.current.as_ref()
                } else {
                    None
                }
            })
            .map(|entry| LibraryStorageMountRecord {
                mount_point: entry.mount_point.to_string_lossy().into_owned(),
                fs_type: entry.fs_type.clone(),
                source: entry.source.clone(),
            })
            .collect()
    }

    /// Two plans agree on the root when the storage holding it has the same
    /// observation and verdict and the root folder is equally present. A scan
    /// whose root storage changed while it ran must not delete anything;
    /// mounts below the root are judged again from the later plan.
    pub(crate) fn same_root_storage_as(&self, other: &StoragePlan) -> bool {
        let signature = |plan: &StoragePlan| {
            let root_region = plan.root_region();
            (
                root_region.mount_point.clone(),
                root_region.current.clone(),
                root_region.verdict.kind(),
                plan.root_missing,
            )
        };
        signature(self) == signature(other)
    }
}

/// Checks a library's storage and records the result on the library.
pub async fn check_library_storage(
    pool: &PgPool,
    library: &Library,
    environment: &dyn LibraryStorageEnvironment,
) -> ApplicationResult<LibraryStorageCheck> {
    let assessment = assess_library_storage(pool, library, environment).await?;
    record_storage_assessment(pool, library.id, &assessment).await?;
    Ok(match assessment {
        StorageAssessment::Unavailable(issue) => LibraryStorageCheck::Unavailable(issue),
        StorageAssessment::RemovedFromDeployment { mount_points } => {
            LibraryStorageCheck::RemovedFromDeployment { mount_points }
        }
        StorageAssessment::Available(_) => LibraryStorageCheck::Available,
    })
}

pub(crate) async fn record_storage_assessment(
    pool: &PgPool,
    library_id: i64,
    assessment: &StorageAssessment,
) -> ApplicationResult<()> {
    match assessment {
        StorageAssessment::Unavailable(issue) => {
            if mova_db::mark_library_storage_unavailable(pool, library_id, issue)
                .await
                .map_err(ApplicationError::from)?
            {
                tracing::warn!(
                    library_id,
                    reason_code = %issue.reason_code,
                    mount_point = %issue.mount_point,
                    diagnostic = issue.diagnostic_message.as_deref().unwrap_or_default(),
                    "library storage became unavailable"
                );
            }
        }
        StorageAssessment::Available(_) | StorageAssessment::RemovedFromDeployment { .. } => {
            if mova_db::mark_library_storage_available(pool, library_id)
                .await
                .map_err(ApplicationError::from)?
            {
                tracing::info!(library_id, "library storage is available again");
            }
        }
    }
    Ok(())
}

/// The error a scan request receives while the library is unavailable.
pub fn library_storage_unavailable_error(
    library_id: i64,
    issue: &LibraryStorageIssue,
) -> ApplicationError {
    let mut params = BusinessErrorParams::from([
        ("library_id".to_string(), Value::from(library_id)),
        (
            "reason_code".to_string(),
            Value::from(issue.reason_code.clone()),
        ),
        (
            "mount_point".to_string(),
            Value::from(issue.mount_point.clone()),
        ),
    ]);
    for (key, value) in [
        ("expected_fs_type", &issue.expected_fs_type),
        ("expected_source", &issue.expected_source),
        ("actual_fs_type", &issue.actual_fs_type),
        ("actual_source", &issue.actual_source),
    ] {
        if let Some(value) = value {
            params.insert(key.to_string(), Value::from(value.clone()));
        }
    }
    BusinessError::new(
        BusinessErrorKind::Conflict,
        LIBRARY_STORAGE_UNAVAILABLE_ERROR,
        params,
        format!(
            "library {library_id} storage is unavailable ({}) at {}",
            issue.reason_code, issue.mount_point
        ),
    )
    .into()
}

pub(crate) async fn assess_library_storage(
    pool: &PgPool,
    library: &Library,
    environment: &dyn LibraryStorageEnvironment,
) -> ApplicationResult<StorageAssessment> {
    let root_path = PathBuf::from(&library.root_path);
    let table = match environment.mount_table() {
        Ok(table) => table,
        Err(error) => {
            return Ok(StorageAssessment::Unavailable(storage_issue(
                MOUNT_TABLE_UNAVAILABLE,
                &root_path,
                None,
                None,
                Some(error.to_string()),
            )));
        }
    };
    let recorded = mova_db::list_library_storage_mounts(pool, library.id)
        .await
        .map_err(ApplicationError::from)?;
    let has_record = !recorded.is_empty();
    let in_container = environment.in_container();
    let mut regions = build_regions(&root_path, &table, &recorded);
    if !regions.iter().any(|region| region.holds_root) {
        return Ok(StorageAssessment::Unavailable(storage_issue(
            MOUNT_TABLE_UNAVAILABLE,
            &root_path,
            None,
            None,
            Some("no mount holds the library root".to_string()),
        )));
    }

    let region_mount_points = regions
        .iter()
        .map(|region| region.mount_point.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let mut root_missing = false;
    for region in &mut regions {
        // Only the root is compared with its record; mounts below it are
        // judged by whether they can be read now.
        let needs_known_files = region.holds_root
            && match (&region.recorded, &region.current) {
                (Some(recorded), Some(current)) => !recorded.same_storage_as(current),
                (None, Some(_)) => !has_record,
                _ => false,
            };
        let known_files = if needs_known_files {
            let mount_point = if has_record {
                region.mount_point.to_string_lossy().into_owned()
            } else {
                "/".to_string()
            };
            let deeper = region_mount_points
                .iter()
                .filter(|other| {
                    has_record
                        && Path::new(other.as_str()) != region.mount_point
                        && Path::new(other.as_str()).starts_with(&region.mount_point)
                })
                .cloned()
                .collect::<Vec<_>>();
            mova_db::sample_library_media_file_paths(
                pool,
                library.id,
                &mount_point,
                &deeper,
                KNOWN_FILE_SAMPLE_LIMIT,
            )
            .await
            .map_err(ApplicationError::from)?
        } else {
            Vec::new()
        };

        let probe = RegionProbe {
            mount_point: region.mount_point.clone(),
            recorded: region.recorded.clone(),
            current: region.current.clone(),
            actual_holder: table.mount_holding(&region.mount_point).cloned(),
            holds_root: region.holds_root,
            root_path: root_path.clone(),
            needs_known_files,
            known_files,
            has_record,
            in_container,
        };
        let outcome = run_with_timeout(move || probe.run()).await;
        let (verdict, missing) = outcome.unwrap_or_else(|| {
            (
                RegionVerdict::Abnormal(storage_issue(
                    STORAGE_TIMEOUT,
                    &region.mount_point,
                    region.recorded.as_ref(),
                    region.current.as_ref(),
                    Some(format!(
                        "the storage did not answer within {} seconds",
                        STORAGE_PROBE_TIMEOUT.as_secs()
                    )),
                )),
                false,
            )
        });
        if region.holds_root {
            root_missing = missing;
        }
        region.verdict = verdict;
    }

    Ok(decide(root_path, regions, root_missing, in_container))
}

fn build_regions(
    root_path: &Path,
    table: &MountTable,
    recorded: &[LibraryStorageMountRecord],
) -> Vec<StorageRegion> {
    let mut regions = BTreeMap::<PathBuf, StorageRegion>::new();
    let mut insert = |mount_point: PathBuf, recorded: Option<MountEntry>| {
        let current = table.mount_at(&mount_point).cloned();
        let region = regions
            .entry(mount_point.clone())
            .or_insert_with(|| StorageRegion {
                mount_point,
                recorded: None,
                current,
                holds_root: false,
                verdict: RegionVerdict::Healthy,
            });
        if recorded.is_some() {
            region.recorded = recorded;
        }
    };
    for record in recorded {
        insert(
            PathBuf::from(&record.mount_point),
            Some(MountEntry {
                mount_point: PathBuf::from(&record.mount_point),
                fs_type: record.fs_type.clone(),
                source: record.source.clone(),
            }),
        );
    }
    let current_root = table.mount_holding(root_path);
    if let Some(current_root) = current_root {
        insert(current_root.mount_point.clone(), None);
    }
    for mount in table.mounts_below(root_path) {
        insert(mount.mount_point.clone(), None);
    }

    // The record decides which storage the root belongs to; without one the
    // current mount table does.
    let root_mount_point = recorded
        .iter()
        .map(|record| PathBuf::from(&record.mount_point))
        .filter(|mount_point| root_path.starts_with(mount_point))
        .max_by_key(|mount_point| mount_point.components().count())
        .or_else(|| current_root.map(|mount| mount.mount_point.clone()));
    if let Some(root_mount_point) = root_mount_point {
        if let Some(region) = regions.get_mut(&root_mount_point) {
            region.holds_root = true;
        }
    }

    regions.into_values().collect()
}

struct RegionProbe {
    mount_point: PathBuf,
    recorded: Option<MountEntry>,
    current: Option<MountEntry>,
    actual_holder: Option<MountEntry>,
    holds_root: bool,
    root_path: PathBuf,
    needs_known_files: bool,
    known_files: Vec<String>,
    has_record: bool,
    in_container: bool,
}

impl RegionProbe {
    /// Returns the verdict and, for the root region, whether the root folder
    /// is missing from otherwise healthy storage.
    fn run(self) -> (RegionVerdict, bool) {
        let Some(current) = self.current.as_ref() else {
            let removed_from_deployment = self.in_container && self.mount_point != Path::new("/");
            if removed_from_deployment {
                return (RegionVerdict::Removed, false);
            }
            return (
                self.abnormal(STORAGE_NOT_CONNECTED, self.actual_holder.as_ref(), None),
                false,
            );
        };

        if self.needs_known_files && !self.known_files.is_empty() {
            match self.known_files_present() {
                Ok(true) => {}
                Ok(false) => {
                    let reason_code = if self.has_record {
                        STORAGE_NOT_CONNECTED
                    } else {
                        STORAGE_UNVERIFIED
                    };
                    return (self.abnormal(reason_code, Some(current), None), false);
                }
                Err(error) => {
                    return (
                        self.abnormal(STORAGE_UNREADABLE, Some(current), Some(error)),
                        false,
                    );
                }
            }
        }

        let probe_path = if self.holds_root {
            &self.root_path
        } else {
            &self.mount_point
        };
        match probe_directory(probe_path) {
            Ok(()) => (RegionVerdict::Healthy, false),
            Err(error) if self.holds_root && error.kind() == io::ErrorKind::NotFound => {
                // The library folder is gone, but is the storage there?
                match probe_directory(&self.mount_point) {
                    Ok(()) => (RegionVerdict::Healthy, true),
                    Err(error) => (
                        self.abnormal(STORAGE_UNREADABLE, Some(current), Some(error)),
                        false,
                    ),
                }
            }
            Err(error) => (
                self.abnormal(STORAGE_UNREADABLE, Some(current), Some(error)),
                false,
            ),
        }
    }

    fn known_files_present(&self) -> io::Result<bool> {
        let mut last_failure = None;
        for file_path in &self.known_files {
            match known_file_presence(Path::new(file_path)) {
                CatalogPathPresence::Present => return Ok(true),
                CatalogPathPresence::Absent => {}
                CatalogPathPresence::Failed(error) => last_failure = Some(error),
            }
        }
        match last_failure {
            Some(error) => Err(error),
            None => Ok(false),
        }
    }

    fn abnormal(
        &self,
        reason_code: &str,
        actual: Option<&MountEntry>,
        error: Option<io::Error>,
    ) -> RegionVerdict {
        RegionVerdict::Abnormal(storage_issue(
            reason_code,
            &self.mount_point,
            self.recorded.as_ref(),
            actual,
            error.map(|error| error.to_string()),
        ))
    }
}

fn decide(
    root_path: PathBuf,
    regions: Vec<StorageRegion>,
    root_missing: bool,
    in_container: bool,
) -> StorageAssessment {
    let plan = StoragePlan {
        root_path,
        regions,
        root_missing,
    };
    // Without a record nothing proves what the root should be, so any
    // abnormality there keeps the catalog, whatever the storage type.
    let root_region = plan.root_region();
    if let RegionVerdict::Abnormal(issue) = &root_region.verdict {
        if plan.root_is_network() || root_region.recorded.is_none() {
            return StorageAssessment::Unavailable(issue.clone());
        }
    }

    let configured = plan
        .regions
        .iter()
        .filter(|region| region.recorded.is_some() && region.mount_point != Path::new("/"))
        .collect::<Vec<_>>();
    if in_container
        && !configured.is_empty()
        && configured
            .iter()
            .all(|region| region.verdict == RegionVerdict::Removed)
    {
        return StorageAssessment::RemovedFromDeployment {
            mount_points: configured
                .iter()
                .map(|region| region.mount_point.to_string_lossy().into_owned())
                .collect(),
        };
    }

    StorageAssessment::Available(plan)
}

fn storage_issue(
    reason_code: &str,
    mount_point: &Path,
    expected: Option<&MountEntry>,
    actual: Option<&MountEntry>,
    diagnostic_message: Option<String>,
) -> LibraryStorageIssue {
    LibraryStorageIssue {
        reason_code: reason_code.to_string(),
        mount_point: mount_point.to_string_lossy().into_owned(),
        expected_fs_type: expected.map(|entry| entry.fs_type.clone()),
        expected_source: expected.map(|entry| entry.source.clone()),
        actual_fs_type: actual.map(|entry| entry.fs_type.clone()),
        actual_source: actual.map(|entry| entry.source.clone()),
        diagnostic_message,
    }
}

pub(crate) enum MissingPathVerification {
    /// Catalog paths the traversal did not report that are still regular
    /// files inside the library root are kept. `plan` is the storage judged
    /// right before deletion; its record replaces the stored one.
    Verified {
        present_paths: Vec<String>,
        plan: Box<StoragePlan>,
    },
    /// The root storage is not what the traversal ran on; nothing may be
    /// deleted.
    StorageChanged {
        detail: String,
        unavailable: Option<LibraryStorageIssue>,
    },
}

/// Confirms the root storage again and looks up every catalog path the
/// traversal did not report. Paths on mounts treated as nonexistent need no
/// look-up, and a path whose look-up fails is treated as nonexistent, unless
/// the library root itself can no longer be read.
pub(crate) async fn verify_missing_media_paths(
    pool: &PgPool,
    library: &Library,
    environment: &dyn LibraryStorageEnvironment,
    plan: &StoragePlan,
    observed_paths: &[String],
) -> ApplicationResult<MissingPathVerification> {
    let current = match assess_library_storage(pool, library, environment).await? {
        StorageAssessment::Available(current) if current.same_root_storage_as(plan) => current,
        StorageAssessment::Unavailable(issue) => {
            return Ok(MissingPathVerification::StorageChanged {
                detail: format!("{} at {}", issue.reason_code, issue.mount_point),
                unavailable: Some(issue),
            });
        }
        _ => {
            return Ok(MissingPathVerification::StorageChanged {
                detail: "the storage holding the library root differs from the start of the scan"
                    .to_string(),
                unavailable: None,
            });
        }
    };

    let observed = observed_paths
        .iter()
        .map(String::as_str)
        .collect::<std::collections::HashSet<_>>();
    let candidates = mova_db::list_library_media_file_paths(pool, library.id)
        .await
        .map_err(ApplicationError::from)?
        .into_iter()
        .filter(|path| !observed.contains(path.as_str()))
        .filter(|path| !current.is_dead_path(Path::new(path)))
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return Ok(MissingPathVerification::Verified {
            present_paths: Vec::new(),
            plan: Box::new(current),
        });
    }

    let root_path = PathBuf::from(&library.root_path);
    let lookup = tokio::time::timeout(
        MISSING_PATH_LOOKUP_TIMEOUT,
        tokio::task::spawn_blocking(move || {
            let canonical_root =
                std::fs::canonicalize(&root_path).unwrap_or_else(|_| root_path.clone());
            let mut present_paths = Vec::new();
            let mut failed_lookups = 0usize;
            for path in candidates {
                match catalog_path_presence(Path::new(&path), &canonical_root) {
                    CatalogPathPresence::Present => present_paths.push(path),
                    CatalogPathPresence::Absent => {}
                    CatalogPathPresence::Failed(error) => {
                        failed_lookups += 1;
                        tracing::warn!(
                            path,
                            error = %error,
                            "treating an unreadable catalog path as removed"
                        );
                    }
                }
            }
            // Failed look-ups are removals only while the root still answers.
            let root_failure = if failed_lookups > 0 {
                probe_directory(&root_path).err()
            } else {
                None
            };
            (present_paths, root_failure)
        }),
    )
    .await;

    Ok(match lookup {
        Ok(Ok((present_paths, None))) => MissingPathVerification::Verified {
            present_paths,
            plan: Box::new(current),
        },
        Ok(Ok((present_paths, Some(error)))) => {
            if current.failure_decision(&current.root_path) == DiscoveryFailureDecision::Abort {
                let issue = current.outage_issue(&current.root_path, &error);
                MissingPathVerification::StorageChanged {
                    detail: format!("{} at {}", issue.reason_code, issue.mount_point),
                    unavailable: Some(issue),
                }
            } else {
                MissingPathVerification::Verified {
                    present_paths,
                    plan: Box::new(current),
                }
            }
        }
        Ok(Err(error)) => {
            return Err(ApplicationError::Unexpected(anyhow::anyhow!(
                "The missing media look-up worker exited unexpectedly: {error}"
            )));
        }
        Err(_) => MissingPathVerification::StorageChanged {
            detail: format!(
                "looking up missing media did not finish within {} seconds",
                MISSING_PATH_LOOKUP_TIMEOUT.as_secs()
            ),
            unavailable: None,
        },
    })
}

/// Runs blocking storage I/O off the async runtime. A hard network mount can
/// block a thread indefinitely; the caller stops waiting and treats the
/// storage as not answering, while the stuck thread is left behind.
pub(crate) async fn run_with_timeout<T, F>(work: F) -> Option<T>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    match tokio::time::timeout(STORAGE_PROBE_TIMEOUT, tokio::task::spawn_blocking(work)).await {
        Ok(Ok(value)) => Some(value),
        Ok(Err(error)) => {
            tracing::error!(error = ?error, "library storage probe worker exited unexpectedly");
            None
        }
        Err(_) => None,
    }
}

/// A mount table described by a test over real temporary directories.
#[cfg(test)]
pub(crate) struct TestStorageEnvironment {
    entries: std::sync::Mutex<Vec<MountEntry>>,
    in_container: bool,
}

#[cfg(test)]
impl TestStorageEnvironment {
    /// `entries` are `(mount_point, fs_type, source)`; `/` is always present.
    pub(crate) fn new(entries: &[(&Path, &str, &str)], in_container: bool) -> Self {
        let environment = Self {
            entries: std::sync::Mutex::new(Vec::new()),
            in_container,
        };
        environment.set(entries);
        environment
    }

    pub(crate) fn set(&self, entries: &[(&Path, &str, &str)]) {
        let mut table = vec![MountEntry {
            mount_point: PathBuf::from("/"),
            fs_type: "overlay".to_string(),
            source: "overlay".to_string(),
        }];
        table.extend(
            entries
                .iter()
                .map(|(mount_point, fs_type, source)| MountEntry {
                    mount_point: mount_point.to_path_buf(),
                    fs_type: fs_type.to_string(),
                    source: source.to_string(),
                }),
        );
        *self.entries.lock().unwrap() = table;
    }
}

#[cfg(test)]
impl LibraryStorageEnvironment for TestStorageEnvironment {
    fn mount_table(&self) -> io::Result<MountTable> {
        Ok(MountTable::from_entries(
            self.entries.lock().unwrap().clone(),
        ))
    }

    fn in_container(&self) -> bool {
        self.in_container
    }
}

#[cfg(test)]
#[path = "library_storage/tests.rs"]
mod tests;
