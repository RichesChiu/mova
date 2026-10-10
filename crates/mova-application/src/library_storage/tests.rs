use super::{
    build_regions, decide, RegionProbe, RegionVerdict, StorageAssessment, StoragePlan,
    StorageRegion, STORAGE_NOT_CONNECTED, STORAGE_UNREADABLE, STORAGE_UNVERIFIED,
};
use mova_db::LibraryStorageMountRecord;
use mova_scan::{DiscoveryFailureDecision, MountEntry, MountTable};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn entry(mount_point: &str, fs_type: &str, source: &str) -> MountEntry {
    MountEntry {
        mount_point: PathBuf::from(mount_point),
        fs_type: fs_type.to_string(),
        source: source.to_string(),
    }
}

fn record(mount_point: &str, fs_type: &str, source: &str) -> LibraryStorageMountRecord {
    LibraryStorageMountRecord {
        mount_point: mount_point.to_string(),
        fs_type: fs_type.to_string(),
        source: source.to_string(),
    }
}

fn abnormal(reason_code: &str) -> RegionVerdict {
    RegionVerdict::Abnormal(super::storage_issue(
        reason_code,
        Path::new("/x"),
        None,
        None,
        None,
    ))
}

fn region(
    mount_point: &str,
    recorded: Option<MountEntry>,
    current: Option<MountEntry>,
    holds_root: bool,
    verdict: RegionVerdict,
) -> StorageRegion {
    StorageRegion {
        mount_point: PathBuf::from(mount_point),
        recorded,
        current,
        holds_root,
        verdict,
    }
}

fn plan(regions: Vec<StorageRegion>) -> StoragePlan {
    StoragePlan {
        root_path: PathBuf::from("/media/TV"),
        regions,
        root_missing: false,
    }
}

#[test]
fn regions_follow_the_record_for_the_root_and_add_new_mounts_below_it() {
    let table = MountTable::from_entries(vec![
        entry("/", "overlay", "overlay"),
        entry("/media", "ext4", "/dev/sda2"),
        entry("/media/TV/disk3", "ext4", "/dev/sdd1"),
        entry("/srv", "ext4", "/dev/sde1"),
    ]);
    let regions = build_regions(
        Path::new("/media/TV"),
        &table,
        &[
            record("/media", "cifs", "//nas/media"),
            record("/media/TV/disk2", "nfs4", "nas:/disk2"),
        ],
    );

    let summary = regions
        .iter()
        .map(|region| {
            (
                region.mount_point.to_string_lossy().into_owned(),
                region.recorded.as_ref().map(|entry| entry.fs_type.clone()),
                region.current.as_ref().map(|entry| entry.fs_type.clone()),
                region.holds_root,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        summary,
        vec![
            (
                "/media".to_string(),
                Some("cifs".to_string()),
                Some("ext4".to_string()),
                true
            ),
            (
                "/media/TV/disk2".to_string(),
                Some("nfs4".to_string()),
                None,
                false
            ),
            (
                "/media/TV/disk3".to_string(),
                None,
                Some("ext4".to_string()),
                false
            ),
        ]
    );
}

#[test]
fn without_a_record_the_current_mount_holds_the_root() {
    let table = MountTable::from_entries(vec![entry("/", "overlay", "overlay")]);
    let regions = build_regions(Path::new("/media/TV"), &table, &[]);

    assert_eq!(regions.len(), 1);
    assert_eq!(regions[0].mount_point, PathBuf::from("/"));
    assert!(regions[0].holds_root);
}

#[test]
fn an_abnormal_network_root_makes_the_library_unavailable() {
    let assessment = decide(
        PathBuf::from("/media/TV"),
        vec![region(
            "/media",
            Some(entry("/media", "cifs", "//nas/media")),
            Some(entry("/media", "ext4", "/dev/sda2")),
            true,
            abnormal(STORAGE_NOT_CONNECTED),
        )],
        false,
        true,
    );

    assert!(matches!(
        assessment,
        StorageAssessment::Unavailable(issue) if issue.reason_code == STORAGE_NOT_CONNECTED
    ));
}

#[test]
fn an_abnormal_root_without_a_record_is_unavailable_whatever_its_type() {
    let assessment = decide(
        PathBuf::from("/media/TV"),
        vec![region(
            "/",
            None,
            Some(entry("/", "overlay", "overlay")),
            true,
            abnormal(STORAGE_UNVERIFIED),
        )],
        false,
        true,
    );

    assert!(matches!(assessment, StorageAssessment::Unavailable(_)));
}

#[test]
fn an_abnormal_local_root_is_traversed_as_nothing_and_keeps_its_record() {
    let StorageAssessment::Available(plan) = decide(
        PathBuf::from("/media/TV"),
        vec![region(
            "/media",
            Some(entry("/media", "ext4", "/dev/sdb1")),
            Some(entry("/media", "ext4", "/dev/sda2")),
            true,
            abnormal(STORAGE_NOT_CONNECTED),
        )],
        false,
        true,
    ) else {
        panic!("an abnormal local root is treated as nonexistent, not unavailable");
    };

    assert!(plan.skips_traversal());
    assert!(plan.is_dead_path(Path::new("/media/TV/a.mkv")));
    assert_eq!(
        plan.storage_record(),
        vec![record("/media", "ext4", "/dev/sdb1")]
    );
}

#[test]
fn every_configured_mount_removed_in_a_container_removes_the_library() {
    let regions = || {
        vec![
            region(
                "/",
                Some(entry("/", "overlay", "overlay")),
                Some(entry("/", "overlay", "overlay")),
                true,
                RegionVerdict::Healthy,
            ),
            region(
                "/media/TV/disk1",
                Some(entry("/media/TV/disk1", "ext4", "/dev/sdb1")),
                None,
                false,
                RegionVerdict::Removed,
            ),
        ]
    };

    assert_eq!(
        decide(PathBuf::from("/media/TV"), regions(), false, true),
        StorageAssessment::RemovedFromDeployment {
            mount_points: vec!["/media/TV/disk1".to_string()],
        }
    );
    assert!(matches!(
        decide(PathBuf::from("/media/TV"), regions(), false, false),
        StorageAssessment::Available(_)
    ));
}

#[test]
fn only_failing_to_read_a_network_root_itself_is_an_outage() {
    let network_root = plan(vec![
        region(
            "/media",
            Some(entry("/media", "cifs", "//nas/media")),
            Some(entry("/media", "cifs", "//nas/media")),
            true,
            RegionVerdict::Healthy,
        ),
        region(
            "/media/TV/disk2",
            Some(entry("/media/TV/disk2", "cifs", "//nas2/disk2")),
            Some(entry("/media/TV/disk2", "cifs", "//nas2/disk2")),
            false,
            RegionVerdict::Healthy,
        ),
    ]);
    assert_eq!(
        network_root.failure_decision(Path::new("/media/TV")),
        DiscoveryFailureDecision::Abort
    );
    // Folders and files below the root, on the same share or another mount,
    // are treated as nonexistent when they cannot be read.
    for path in [
        "/media/TV/Show",
        "/media/TV/Show/S01E01.mkv",
        "/media/TV/#recycle",
        "/media/TV/disk2",
        "/media/TV/disk2/Show/S01E01.mkv",
    ] {
        assert_eq!(
            network_root.failure_decision(Path::new(path)),
            DiscoveryFailureDecision::SkipPath,
            "{path}"
        );
    }
    assert_eq!(
        network_root
            .outage_issue(
                Path::new("/media/TV"),
                &std::io::Error::from_raw_os_error(5)
            )
            .reason_code,
        STORAGE_UNREADABLE
    );

    let local_root = plan(vec![region(
        "/media",
        Some(entry("/media", "ext4", "/dev/sdb1")),
        Some(entry("/media", "ext4", "/dev/sdb1")),
        true,
        RegionVerdict::Healthy,
    )]);
    assert_eq!(
        local_root.failure_decision(Path::new("/media/TV")),
        DiscoveryFailureDecision::SkipPath
    );
}

#[test]
fn dead_mounts_below_the_root_are_excluded_and_dropped_from_the_record() {
    let plan = plan(vec![
        region(
            "/media",
            Some(entry("/media", "cifs", "//nas/media")),
            Some(entry("/media", "cifs", "//nas/media")),
            true,
            RegionVerdict::Healthy,
        ),
        region(
            "/media/TV/disk2",
            Some(entry("/media/TV/disk2", "ext4", "/dev/sdc1")),
            Some(entry("/media/TV/disk2", "ext4", "/dev/sda2")),
            false,
            abnormal(STORAGE_NOT_CONNECTED),
        ),
        region(
            "/media/TV/disk3",
            None,
            Some(entry("/media/TV/disk3", "ext4", "/dev/sdd1")),
            false,
            RegionVerdict::Healthy,
        ),
    ]);

    assert!(!plan.skips_traversal());
    assert_eq!(
        plan.excluded_directories(),
        vec![PathBuf::from("/media/TV/disk2")]
    );
    assert!(plan.is_dead_path(Path::new("/media/TV/disk2/a.mkv")));
    assert!(!plan.is_dead_path(Path::new("/media/TV/disk3/a.mkv")));
    assert!(!plan.is_dead_path(Path::new("/media/TV/a.mkv")));
    assert_eq!(
        plan.storage_record(),
        vec![
            record("/media", "cifs", "//nas/media"),
            record("/media/TV/disk3", "ext4", "/dev/sdd1"),
        ]
    );
}

#[test]
fn plans_agree_when_the_root_storage_is_unchanged() {
    let with_disk2 = |disk2_verdict: RegionVerdict| {
        plan(vec![
            region(
                "/media",
                Some(entry("/media", "cifs", "//nas/media")),
                Some(entry("/media", "cifs", "//nas/media")),
                true,
                RegionVerdict::Healthy,
            ),
            region(
                "/media/TV/disk2",
                None,
                Some(entry("/media/TV/disk2", "cifs", "//nas2/disk2")),
                false,
                disk2_verdict,
            ),
        ])
    };
    let healthy = with_disk2(RegionVerdict::Healthy);
    let mut changed_root = with_disk2(RegionVerdict::Healthy);
    changed_root.regions[0].current = Some(entry("/media", "ext4", "/dev/sda2"));
    let mut missing_root = with_disk2(RegionVerdict::Healthy);
    missing_root.root_missing = true;

    assert!(healthy.same_root_storage_as(&with_disk2(RegionVerdict::Healthy)));
    // A mount below the root changing state does not block the scan.
    assert!(healthy.same_root_storage_as(&with_disk2(abnormal(STORAGE_UNREADABLE))));
    assert!(!healthy.same_root_storage_as(&changed_root));
    assert!(!healthy.same_root_storage_as(&missing_root));
}

struct ProbeFolder {
    base: PathBuf,
}

impl ProbeFolder {
    fn new() -> Self {
        let base = std::env::temp_dir().join(format!("mova-probe-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(base.join("share/TV")).unwrap();
        Self { base }
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.base.join(relative)
    }

    fn probe(&self, current: Option<MountEntry>, known_files: Vec<String>) -> RegionProbe {
        RegionProbe {
            mount_point: self.path("share"),
            recorded: Some(entry("/share", "ext4", "/dev/sdb1")),
            current,
            actual_holder: Some(entry("/", "overlay", "overlay")),
            holds_root: true,
            root_path: self.path("share/TV"),
            needs_known_files: true,
            known_files,
            has_record: true,
            in_container: true,
        }
    }
}

impl Drop for ProbeFolder {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

#[test]
fn changed_storage_is_healthy_only_when_it_holds_known_files() {
    let folder = ProbeFolder::new();
    let known = folder.path("share/TV/a.mkv");
    fs::write(&known, b"x").unwrap();
    let changed = Some(entry("/share", "ext4", "/dev/sdc1"));

    let (verdict, missing) = folder
        .probe(changed.clone(), vec![known.to_string_lossy().into_owned()])
        .run();
    assert_eq!((verdict, missing), (RegionVerdict::Healthy, false));

    fs::remove_file(&known).unwrap();
    let (verdict, _) = folder
        .probe(changed, vec![known.to_string_lossy().into_owned()])
        .run();
    assert!(matches!(
        verdict,
        RegionVerdict::Abnormal(issue)
            if issue.reason_code == STORAGE_NOT_CONNECTED
                && issue.expected_source.as_deref() == Some("/dev/sdb1")
                && issue.actual_source.as_deref() == Some("/dev/sdc1")
    ));
}

#[test]
fn a_missing_mount_is_removed_in_a_container_and_not_connected_elsewhere() {
    let folder = ProbeFolder::new();

    let (verdict, _) = folder.probe(None, Vec::new()).run();
    assert_eq!(verdict, RegionVerdict::Removed);

    let mut outside = folder.probe(None, Vec::new());
    outside.in_container = false;
    let (verdict, _) = outside.run();
    assert!(matches!(
        verdict,
        RegionVerdict::Abnormal(issue)
            if issue.reason_code == STORAGE_NOT_CONNECTED
                && issue.actual_fs_type.as_deref() == Some("overlay")
    ));
}

#[test]
fn a_missing_root_folder_on_readable_storage_is_healthy_but_missing() {
    let folder = ProbeFolder::new();
    fs::remove_dir_all(folder.path("share/TV")).unwrap();
    let mut probe = folder.probe(Some(entry("/share", "ext4", "/dev/sdb1")), Vec::new());
    probe.needs_known_files = false;

    assert_eq!(probe.run(), (RegionVerdict::Healthy, true));
}

#[test]
fn a_mount_below_the_root_is_judged_only_by_whether_it_can_be_read() {
    let folder = ProbeFolder::new();
    let below = |mount_point: PathBuf| RegionProbe {
        mount_point,
        recorded: Some(entry("/share/TV/disk2", "cifs", "//nas2/disk2")),
        current: Some(entry("/share/TV/disk2", "ext4", "/dev/sda2")),
        actual_holder: Some(entry("/", "overlay", "overlay")),
        holds_root: false,
        root_path: folder.path("share/TV"),
        needs_known_files: false,
        known_files: Vec::new(),
        has_record: true,
        in_container: true,
    };

    // Another filesystem than recorded, but readable: healthy.
    fs::create_dir_all(folder.path("share/TV/disk2")).unwrap();
    assert_eq!(
        below(folder.path("share/TV/disk2")).run(),
        (RegionVerdict::Healthy, false)
    );

    // Cannot be read: treated as nonexistent.
    let (verdict, _) = below(folder.path("share/TV/gone")).run();
    assert!(matches!(
        verdict,
        RegionVerdict::Abnormal(issue) if issue.reason_code == STORAGE_UNREADABLE
    ));
}
