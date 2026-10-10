//! Storage facts a scan needs before it may treat a missing path as deleted:
//! which mount holds a path, what filesystem that mount is, and whether a path
//! the catalog knows still names a file.

use std::{
    fs,
    io::{self, ErrorKind},
    path::{Path, PathBuf},
};

const MOUNTINFO_PATH: &str = "/proc/self/mountinfo";
const ELOOP: i32 = 40;

/// Filesystems whose failures usually come from the network between Mova and
/// the storage rather than from the storage itself.
const NETWORK_FILESYSTEM_TYPES: &[&str] = &[
    "cifs",
    "smb3",
    "smbfs",
    "nfs",
    "nfs4",
    "fuse.rclone",
    "fuse.sshfs",
];

/// One mount as the kernel reports it in this process's mount namespace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MountEntry {
    pub mount_point: PathBuf,
    pub fs_type: String,
    pub source: String,
}

impl MountEntry {
    pub fn is_network(&self) -> bool {
        is_network_filesystem(&self.fs_type)
    }

    /// Two observations name the same storage when the filesystem and its
    /// source are unchanged; the mount options and the bound subdirectory are
    /// deployment details that do not decide what a path resolves to.
    pub fn same_storage_as(&self, other: &MountEntry) -> bool {
        self.fs_type == other.fs_type && self.source == other.source
    }
}

/// The mount table in kernel order: later entries are mounted on top of
/// earlier ones.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MountTable {
    entries: Vec<MountEntry>,
}

impl MountTable {
    pub fn read_current() -> io::Result<Self> {
        let text = fs::read_to_string(MOUNTINFO_PATH)?;
        let table = Self::parse(&text);
        if table.entries.is_empty() {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "the mount table has no parsable entries",
            ));
        }
        Ok(table)
    }

    pub fn from_entries(entries: Vec<MountEntry>) -> Self {
        Self { entries }
    }

    /// Parses `/proc/self/mountinfo`. Lines that do not follow the documented
    /// layout are skipped instead of failing the whole table.
    pub fn parse(text: &str) -> Self {
        let entries = text.lines().filter_map(parse_mountinfo_line).collect();
        Self { entries }
    }

    pub fn entries(&self) -> &[MountEntry] {
        &self.entries
    }

    /// The topmost mount whose mount point is exactly `path`.
    pub fn mount_at(&self, path: &Path) -> Option<&MountEntry> {
        self.entries
            .iter()
            .rev()
            .find(|entry| entry.mount_point == path)
    }

    /// The mount that holds `path`: the deepest mount point that is `path` or
    /// one of its ancestors, topmost first when mounts are stacked.
    pub fn mount_holding(&self, path: &Path) -> Option<&MountEntry> {
        let mut holding: Option<&MountEntry> = None;
        for entry in self.entries.iter().rev() {
            if !path.starts_with(&entry.mount_point) {
                continue;
            }
            let deeper = holding.is_none_or(|current| {
                entry.mount_point.components().count() > current.mount_point.components().count()
            });
            if deeper {
                holding = Some(entry);
            }
        }
        holding
    }

    /// Mounts strictly below `path`, one per mount point, in path order.
    pub fn mounts_below(&self, path: &Path) -> Vec<&MountEntry> {
        let mut below = self
            .entries
            .iter()
            .filter(|entry| entry.mount_point != path && entry.mount_point.starts_with(path))
            .map(|entry| &entry.mount_point)
            .collect::<Vec<_>>();
        below.sort();
        below.dedup();
        below
            .into_iter()
            .filter_map(|mount_point| self.mount_at(mount_point))
            .collect()
    }
}

/// `id parent major:minor root mount_point options [optional...] - fstype source super_options`
fn parse_mountinfo_line(line: &str) -> Option<MountEntry> {
    let mut fields = line.split(' ');
    let mount_point = fields.nth(4)?;
    let mut fields = fields.skip_while(|field| *field != "-");
    fields.next()?;
    let fs_type = fields.next()?;
    let source = fields.next()?;
    if mount_point.is_empty() || fs_type.is_empty() {
        return None;
    }

    Some(MountEntry {
        mount_point: PathBuf::from(unescape_mountinfo_field(mount_point)),
        fs_type: unescape_mountinfo_field(fs_type),
        source: unescape_mountinfo_field(source),
    })
}

/// The kernel writes space, tab, newline and backslash as three-digit octal
/// escapes such as `\040`.
fn unescape_mountinfo_field(field: &str) -> String {
    let bytes = field.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\\' && index + 3 < bytes.len() {
            let digits = &bytes[index + 1..index + 4];
            if digits.iter().all(|digit| (b'0'..=b'7').contains(digit)) {
                let value = digits
                    .iter()
                    .fold(0u32, |value, digit| value * 8 + u32::from(digit - b'0'));
                if let Ok(byte) = u8::try_from(value) {
                    decoded.push(byte);
                    index += 4;
                    continue;
                }
            }
        }
        decoded.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

pub fn is_network_filesystem(fs_type: &str) -> bool {
    NETWORK_FILESYSTEM_TYPES.contains(&fs_type)
}

/// Whether this process runs inside a container, where every bind mount comes
/// from the deployment configuration. Outside a container a missing mount is
/// indistinguishable from storage that is not connected yet.
pub fn running_in_container() -> bool {
    Path::new("/.dockerenv").exists() || Path::new("/run/.containerenv").exists()
}

/// Errors that mean the entry itself does not exist: it was removed, a path
/// component is not a directory, or it is a link that cannot be followed.
/// Every other error, including a denied permission, is a storage failure.
pub fn is_absent_entry_error(error: &io::Error) -> bool {
    matches!(error.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory)
        || error.raw_os_error() == Some(ELOOP)
}

/// Reads the first entry of a directory so a lazily connected network mount
/// has to answer, not just accept the open.
pub fn probe_directory(path: &Path) -> io::Result<()> {
    let mut entries = fs::read_dir(path)?;
    if let Some(entry) = entries.next() {
        entry?;
    }
    Ok(())
}

/// What a direct look-up says about a path the catalog believes exists.
#[derive(Debug)]
pub enum CatalogPathPresence {
    /// A regular file inside the library root.
    Present,
    /// Removed, a link that cannot be followed, outside the library root, or
    /// not a regular file.
    Absent,
    /// The storage did not answer; nothing can be concluded.
    Failed(io::Error),
}

pub fn catalog_path_presence(path: &Path, canonical_root: &Path) -> CatalogPathPresence {
    let canonical_path = match fs::canonicalize(path) {
        Ok(canonical_path) => canonical_path,
        Err(error) if is_absent_entry_error(&error) => return CatalogPathPresence::Absent,
        Err(error) => return CatalogPathPresence::Failed(error),
    };
    if !canonical_path.starts_with(canonical_root) {
        return CatalogPathPresence::Absent;
    }

    match fs::metadata(&canonical_path) {
        Ok(metadata) if metadata.is_file() => CatalogPathPresence::Present,
        Ok(_) => CatalogPathPresence::Absent,
        Err(error) if is_absent_entry_error(&error) => CatalogPathPresence::Absent,
        Err(error) => CatalogPathPresence::Failed(error),
    }
}

/// Whether a known catalog file is reachable, without the library-root
/// boundary: used to tell whether changed storage still holds the library.
pub fn known_file_presence(path: &Path) -> CatalogPathPresence {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => CatalogPathPresence::Present,
        Ok(_) => CatalogPathPresence::Absent,
        Err(error) if is_absent_entry_error(&error) => CatalogPathPresence::Absent,
        Err(error) => CatalogPathPresence::Failed(error),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        catalog_path_presence, is_absent_entry_error, unescape_mountinfo_field,
        CatalogPathPresence, MountEntry, MountTable,
    };
    use std::{fs, io, path::Path};

    const MOUNTINFO: &str = "\
24 1 8:2 / / rw,relatime shared:1 - ext4 /dev/sda2 rw
460 441 0:65 / /media ro,nosuid,nodev,relatime - cifs //192.168.50.3/media rw,vers=3.0
461 460 0:66 / /media/TV\\040Shows/disk2 ro,relatime shared:9 master:3 - nfs4 nas:/export/tv rw
462 441 0:67 / /media ro,relatime - tmpfs tmpfs rw
463 441 0:68 / /mediaother rw - ext4 /dev/sdb1 rw
broken line
";

    fn entry(mount_point: &str, fs_type: &str, source: &str) -> MountEntry {
        MountEntry {
            mount_point: mount_point.into(),
            fs_type: fs_type.to_string(),
            source: source.to_string(),
        }
    }

    #[test]
    fn mountinfo_lines_parse_with_optional_fields_and_escapes() {
        let table = MountTable::parse(MOUNTINFO);

        assert_eq!(table.entries().len(), 5);
        assert_eq!(
            table.entries()[2],
            entry("/media/TV Shows/disk2", "nfs4", "nas:/export/tv")
        );
        assert_eq!(unescape_mountinfo_field("a\\134b\\011c"), "a\\b\tc");
        assert_eq!(unescape_mountinfo_field("trailing\\04"), "trailing\\04");
        assert_eq!(unescape_mountinfo_field("电影\\040A"), "电影 A");
    }

    #[test]
    fn the_topmost_and_deepest_mount_holds_a_path() {
        let table = MountTable::parse(MOUNTINFO);

        assert_eq!(
            table.mount_at(Path::new("/media")),
            Some(&entry("/media", "tmpfs", "tmpfs"))
        );
        assert_eq!(
            table.mount_holding(Path::new("/media/TV Shows/disk2/Show/S01E01.mkv")),
            Some(&entry("/media/TV Shows/disk2", "nfs4", "nas:/export/tv"))
        );
        assert_eq!(
            table.mount_holding(Path::new("/media/电影/a.mkv")),
            Some(&entry("/media", "tmpfs", "tmpfs"))
        );
        assert_eq!(
            table.mount_holding(Path::new("/mediaother/a.mkv")),
            Some(&entry("/mediaother", "ext4", "/dev/sdb1"))
        );
        assert_eq!(
            table.mount_holding(Path::new("/srv/a.mkv")),
            Some(&entry("/", "ext4", "/dev/sda2"))
        );
        assert_eq!(
            table.mounts_below(Path::new("/media")),
            vec![&entry("/media/TV Shows/disk2", "nfs4", "nas:/export/tv")]
        );
        assert!(table
            .mounts_below(Path::new("/media/TV Shows/disk2"))
            .is_empty());
    }

    #[test]
    fn network_filesystems_are_recognized_by_type() {
        assert!(entry("/media", "cifs", "//nas/media").is_network());
        assert!(entry("/media", "nfs4", "nas:/media").is_network());
        assert!(entry("/media", "fuse.rclone", "remote:").is_network());
        assert!(!entry("/media", "ext4", "/dev/sdb1").is_network());
        assert!(!entry("/", "overlay", "overlay").is_network());
    }

    #[test]
    fn storage_identity_ignores_mount_point_and_compares_type_and_source() {
        let recorded = entry("/media", "cifs", "//nas/media");

        assert!(recorded.same_storage_as(&entry("/media", "cifs", "//nas/media")));
        assert!(!recorded.same_storage_as(&entry("/media", "ext4", "/dev/sda2")));
        assert!(!recorded.same_storage_as(&entry("/media", "cifs", "//nas2/media")));
    }

    #[test]
    fn absent_entries_are_told_apart_from_storage_failures() {
        assert!(is_absent_entry_error(&io::Error::from(
            io::ErrorKind::NotFound
        )));
        assert!(!is_absent_entry_error(&io::Error::from(
            io::ErrorKind::PermissionDenied
        )));
        assert!(is_absent_entry_error(&io::Error::from_raw_os_error(40)));
        assert!(!is_absent_entry_error(&io::Error::from_raw_os_error(5)));
        assert!(!is_absent_entry_error(&io::Error::from_raw_os_error(112)));
    }

    #[test]
    fn catalog_paths_are_present_only_as_regular_files_inside_the_root() {
        let root = std::env::temp_dir().join(format!("mova-storage-{}", uuid::Uuid::new_v4()));
        let outside = std::env::temp_dir().join(format!("mova-outside-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("dir")).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(root.join("film.mkv"), b"x").unwrap();
        fs::write(outside.join("film.mkv"), b"x").unwrap();
        std::os::unix::fs::symlink(outside.join("film.mkv"), root.join("escape.mkv")).unwrap();
        std::os::unix::fs::symlink(root.join("gone.mkv"), root.join("dangling.mkv")).unwrap();
        let canonical_root = fs::canonicalize(&root).unwrap();

        let presence = |name: &str| catalog_path_presence(&root.join(name), &canonical_root);
        assert!(matches!(presence("film.mkv"), CatalogPathPresence::Present));
        assert!(matches!(
            presence("missing.mkv"),
            CatalogPathPresence::Absent
        ));
        assert!(matches!(
            presence("dangling.mkv"),
            CatalogPathPresence::Absent
        ));
        assert!(matches!(
            presence("escape.mkv"),
            CatalogPathPresence::Absent
        ));
        assert!(matches!(presence("dir"), CatalogPathPresence::Absent));
        assert!(matches!(
            presence("film.mkv/child.mkv"),
            CatalogPathPresence::Absent
        ));

        fs::remove_dir_all(&root).unwrap();
        fs::remove_dir_all(&outside).unwrap();
    }
}
