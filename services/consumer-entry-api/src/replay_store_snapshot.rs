use std::{
    fs::{File, Metadata, OpenOptions},
    io::{self, Read},
    path::{Component, Path},
    time::UNIX_EPOCH,
};

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
#[cfg(windows)]
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SnapshotIdentity {
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
    #[cfg(windows)]
    creation_time: u64,
    size: u64,
    modified_nanos: u128,
}

#[derive(Debug)]
pub enum SnapshotError {
    Io(io::Error),
    NonRegular,
    LinkedFile,
    Oversized,
    Changed,
    UnsafePath,
    UnsupportedPlatform,
}

impl From<io::Error> for SnapshotError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl std::fmt::Display for SnapshotError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "{error}"),
            Self::NonRegular => formatter.write_str("replay store is not a regular file"),
            Self::LinkedFile => {
                formatter.write_str("replay store has an unsafe link count or reparse identity")
            }
            Self::Oversized => {
                formatter.write_str("replay store exceeds its bounded snapshot budget")
            }
            Self::Changed => formatter.write_str("replay store changed while it was being read"),
            Self::UnsafePath => formatter.write_str("replay store path is not descriptor-safe"),
            Self::UnsupportedPlatform => formatter
                .write_str("descriptor-safe replay snapshots are unsupported on this platform"),
        }
    }
}

impl std::error::Error for SnapshotError {}

fn modified_nanos(metadata: &Metadata) -> u128 {
    metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|value| value.as_nanos())
        .unwrap_or_default()
}

fn identity(metadata: &Metadata) -> SnapshotIdentity {
    SnapshotIdentity {
        #[cfg(unix)]
        device: metadata.dev(),
        #[cfg(unix)]
        inode: metadata.ino(),
        #[cfg(windows)]
        creation_time: metadata.creation_time(),
        size: metadata.len(),
        modified_nanos: modified_nanos(metadata),
    }
}

fn validate_regular(metadata: &Metadata) -> Result<(), SnapshotError> {
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(SnapshotError::NonRegular);
    }
    #[cfg(unix)]
    if metadata.nlink() != 1 {
        return Err(SnapshotError::LinkedFile);
    }
    #[cfg(windows)]
    if metadata.file_attributes() & 0x0000_0400 != 0 {
        return Err(SnapshotError::LinkedFile);
    }
    Ok(())
}

#[cfg(any(target_os = "linux", target_os = "android"))]
mod descriptor_open {
    use super::{Component, File, Path, SnapshotError};
    use std::{
        ffi::CString,
        os::{
            raw::{c_char, c_int},
            unix::{
                ffi::OsStrExt,
                io::{AsRawFd, FromRawFd},
            },
        },
    };

    const O_RDONLY: c_int = 0;
    const O_NONBLOCK: c_int = 0x0000_0800;
    const O_DIRECTORY: c_int = 0x0001_0000;
    const O_NOFOLLOW: c_int = 0x0002_0000;
    const O_CLOEXEC: c_int = 0x0008_0000;

    unsafe extern "C" {
        fn openat(directory_fd: c_int, path: *const c_char, flags: c_int, ...) -> c_int;
    }

    fn openat_component(
        directory: &File,
        component: &std::ffi::OsStr,
        flags: c_int,
    ) -> Result<File, SnapshotError> {
        let component =
            CString::new(component.as_bytes()).map_err(|_| SnapshotError::UnsafePath)?;
        // SAFETY: directory is a live descriptor and the component is NUL-terminated.
        let descriptor = unsafe { openat(directory.as_raw_fd(), component.as_ptr(), flags) };
        if descriptor < 0 {
            return Err(SnapshotError::Io(std::io::Error::last_os_error()));
        }
        // SAFETY: successful openat returns a newly owned descriptor.
        Ok(unsafe { File::from_raw_fd(descriptor) })
    }

    pub(super) fn open(path: &Path) -> Result<File, SnapshotError> {
        let mut directory = if path.is_absolute() {
            File::open("/")?
        } else {
            File::open(".")?
        };
        let mut components = Vec::new();
        for component in path.components() {
            match component {
                Component::RootDir | Component::CurDir => {}
                Component::Normal(value) => components.push(value.to_os_string()),
                Component::ParentDir | Component::Prefix(_) => {
                    return Err(SnapshotError::UnsafePath)
                }
            }
        }
        let leaf = components.pop().ok_or(SnapshotError::UnsafePath)?;
        for component in components {
            directory = openat_component(
                &directory,
                &component,
                O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC,
            )?;
        }
        openat_component(
            &directory,
            &leaf,
            O_RDONLY | O_NONBLOCK | O_NOFOLLOW | O_CLOEXEC,
        )
    }
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
mod descriptor_open {
    use super::{Component, File, Path, SnapshotError};
    use std::{
        ffi::CString,
        os::{
            raw::{c_char, c_int},
            unix::{
                ffi::OsStrExt,
                io::{AsRawFd, FromRawFd},
            },
        },
    };

    const O_RDONLY: c_int = 0;
    const O_NONBLOCK: c_int = 0x0000_0004;
    const O_NOFOLLOW: c_int = 0x0000_0100;
    const O_DIRECTORY: c_int = 0x0010_0000;
    const O_CLOEXEC: c_int = 0x0100_0000;

    unsafe extern "C" {
        fn openat(directory_fd: c_int, path: *const c_char, flags: c_int, ...) -> c_int;
    }

    fn openat_component(
        directory: &File,
        component: &std::ffi::OsStr,
        flags: c_int,
    ) -> Result<File, SnapshotError> {
        let component =
            CString::new(component.as_bytes()).map_err(|_| SnapshotError::UnsafePath)?;
        // SAFETY: directory is a live descriptor and the component is NUL-terminated.
        let descriptor = unsafe { openat(directory.as_raw_fd(), component.as_ptr(), flags) };
        if descriptor < 0 {
            return Err(SnapshotError::Io(std::io::Error::last_os_error()));
        }
        // SAFETY: successful openat returns a newly owned descriptor.
        Ok(unsafe { File::from_raw_fd(descriptor) })
    }

    pub(super) fn open(path: &Path) -> Result<File, SnapshotError> {
        let mut directory = if path.is_absolute() {
            File::open("/")?
        } else {
            File::open(".")?
        };
        let mut components = Vec::new();
        for component in path.components() {
            match component {
                Component::RootDir | Component::CurDir => {}
                Component::Normal(value) => components.push(value.to_os_string()),
                Component::ParentDir | Component::Prefix(_) => {
                    return Err(SnapshotError::UnsafePath)
                }
            }
        }
        let leaf = components.pop().ok_or(SnapshotError::UnsafePath)?;
        for component in components {
            directory = openat_component(
                &directory,
                &component,
                O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC,
            )?;
        }
        openat_component(
            &directory,
            &leaf,
            O_RDONLY | O_NONBLOCK | O_NOFOLLOW | O_CLOEXEC,
        )
    }
}

#[cfg(windows)]
fn open_descriptor(path: &Path) -> Result<File, SnapshotError> {
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const FILE_FLAG_SEQUENTIAL_SCAN: u32 = 0x0800_0000;
    Ok(OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_SEQUENTIAL_SCAN)
        .open(path)?)
}

#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    target_os = "ios"
))]
fn open_descriptor(path: &Path) -> Result<File, SnapshotError> {
    descriptor_open::open(path)
}

#[cfg(all(
    unix,
    not(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios"
    ))
))]
fn open_descriptor(_path: &Path) -> Result<File, SnapshotError> {
    Err(SnapshotError::UnsupportedPlatform)
}

#[cfg(not(any(unix, windows)))]
fn open_descriptor(_path: &Path) -> Result<File, SnapshotError> {
    Err(SnapshotError::UnsupportedPlatform)
}

pub fn read_stable_regular_file(path: &Path, max_bytes: usize) -> Result<Vec<u8>, SnapshotError> {
    let mut file = open_descriptor(path)?;
    let before = file.metadata()?;
    validate_regular(&before)?;
    if before.len() > max_bytes as u64 {
        return Err(SnapshotError::Oversized);
    }

    let mut bytes = Vec::with_capacity((before.len() as usize).min(max_bytes));
    file.by_ref()
        .take(max_bytes.saturating_add(1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > max_bytes {
        return Err(SnapshotError::Oversized);
    }

    let after = file.metadata()?;
    validate_regular(&after)?;
    if identity(&before) != identity(&after) || after.len() != bytes.len() as u64 {
        return Err(SnapshotError::Changed);
    }

    let reopened = open_descriptor(path)?;
    let rebound = reopened.metadata()?;
    validate_regular(&rebound)?;
    if identity(&after) != identity(&rebound) {
        return Err(SnapshotError::Changed);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::PathBuf};

    fn temporary_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "consumer-entry-replay-snapshot-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ))
    }

    #[test]
    fn reads_a_stable_regular_file() {
        let path = temporary_path("stable");
        fs::write(&path, b"snapshot").expect("write fixture");
        assert_eq!(
            read_stable_regular_file(&path, 64).expect("read snapshot"),
            b"snapshot"
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn rejects_oversized_files() {
        let path = temporary_path("oversized");
        fs::write(&path, vec![0_u8; 65]).expect("write fixture");
        assert!(matches!(
            read_stable_regular_file(&path, 64),
            Err(SnapshotError::Oversized)
        ));
        let _ = fs::remove_file(path);
    }

    #[cfg(unix)]
    #[test]
    fn rejects_leaf_symlinks() {
        use std::os::unix::fs::symlink;
        let target = temporary_path("target");
        let link = temporary_path("link");
        fs::write(&target, b"snapshot").expect("write target");
        symlink(&target, &link).expect("create link");
        assert!(read_stable_regular_file(&link, 64).is_err());
        let _ = fs::remove_file(link);
        let _ = fs::remove_file(target);
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinked_ancestors() {
        use std::os::unix::fs::symlink;
        let root = temporary_path("ancestor-root");
        let real = root.join("real");
        let linked = root.join("linked");
        fs::create_dir_all(&real).expect("create real directory");
        fs::write(real.join("snapshot.json"), b"snapshot").expect("write fixture");
        symlink(&real, &linked).expect("create ancestor link");
        assert!(read_stable_regular_file(&linked.join("snapshot.json"), 64).is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn rejects_hard_links() {
        let target = temporary_path("hard-target");
        let link = temporary_path("hard-link");
        fs::write(&target, b"snapshot").expect("write target");
        fs::hard_link(&target, &link).expect("create hard link");
        assert!(matches!(
            read_stable_regular_file(&link, 64),
            Err(SnapshotError::LinkedFile)
        ));
        let _ = fs::remove_file(link);
        let _ = fs::remove_file(target);
    }
}
