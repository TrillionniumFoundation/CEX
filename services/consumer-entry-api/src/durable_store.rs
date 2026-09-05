use serde::{de::DeserializeOwned, Serialize};
use std::{
    ffi::OsStr,
    fs::{self, File, Metadata, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

static TEMP_NONCE: AtomicU64 = AtomicU64::new(0);
const TEMP_CREATE_ATTEMPTS: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FileIdentity {
    key_a: u64,
    key_b: u64,
    length: u64,
    time_a: u64,
    time_b: u64,
    links: u64,
    attributes: u64,
}

struct StoreLock {
    _file: File,
}

/// Read one bounded JSON state while holding the same cross-process lock used by writers.
pub(super) fn read_json<T>(path: &Path, max_bytes: u64) -> io::Result<Option<T>>
where
    T: DeserializeOwned,
{
    let _lock = acquire_store_lock(path)?;
    let Some(bytes) = read_stable_regular_file(path, max_bytes)? else {
        return Ok(None);
    };
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|error| invalid_data(format!("invalid durable JSON state: {error}")))
}

/// Lock, reload, mutate and atomically publish one JSON state snapshot.
///
/// Every cooperating process serializes through a sibling lock file, so concurrent replay and
/// rate-limit mutations cannot overwrite each other with stale in-memory snapshots.  A corrupt or
/// unsafe existing file fails closed and is never replaced with an empty default.
pub(super) fn update_json<T, R, F>(
    path: &Path,
    max_bytes: u64,
    default: T,
    update: F,
) -> io::Result<(T, R)>
where
    T: DeserializeOwned + Serialize,
    F: FnOnce(&mut T) -> R,
{
    let _lock = acquire_store_lock(path)?;
    let mut state = match read_stable_regular_file(path, max_bytes)? {
        Some(bytes) => serde_json::from_slice(&bytes)
            .map_err(|error| invalid_data(format!("invalid durable JSON state: {error}")))?,
        None => default,
    };
    let result = update(&mut state);
    let bytes = serde_json::to_vec(&state)
        .map_err(|error| invalid_data(format!("failed to encode durable JSON state: {error}")))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > max_bytes {
        return Err(invalid_data("durable JSON state exceeds configured size limit"));
    }
    atomic_write(path, &bytes)?;
    Ok((state, result))
}

/// Publish one complete state snapshot without exposing a partially-written destination.
///
/// The temporary file is created in the destination directory, written with `create_new`,
/// flushed and fsynced, then atomically replaces the destination. Existing symbolic links,
/// reparse points, non-regular files and multiply-linked files are rejected before publication.
/// On Unix the containing directory is fsynced after rename; on Windows `MoveFileExW` is used
/// with replacement and write-through semantics.
pub(super) fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let (parent, file_name) = split_destination(path)?;
    fs::create_dir_all(parent)?;
    reject_unsafe_destination(path)?;

    for _ in 0..TEMP_CREATE_ATTEMPTS {
        let temporary = temporary_path(parent, file_name);
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);

        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }

        let mut file = match options.open(&temporary) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        };

        let result = (|| -> io::Result<()> {
            file.write_all(bytes)?;
            file.flush()?;
            file.sync_all()?;
            drop(file);

            // Re-check immediately before publication. Replacing a path is atomic, but refusing a
            // newly introduced special file keeps configuration and attack failures explicit.
            reject_unsafe_destination(path)?;
            replace_file(&temporary, path)?;
            sync_parent_directory(parent)?;
            Ok(())
        })();

        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        return result;
    }

    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not allocate a unique durable-store temporary file",
    ))
}

fn acquire_store_lock(path: &Path) -> io::Result<StoreLock> {
    let (parent, file_name) = split_destination(path)?;
    fs::create_dir_all(parent)?;
    let lock_path = parent.join(format!(".{}.cex-lock", file_name.to_string_lossy()));
    let file = open_or_create_lock_file(&lock_path)?;
    file.lock()?;
    if !path_matches_file(&lock_path, &file)? {
        return Err(invalid_data("durable store lock path changed during acquisition"));
    }
    Ok(StoreLock { _file: file })
}

fn open_or_create_lock_file(path: &Path) -> io::Result<File> {
    let mut create = OpenOptions::new();
    create.create_new(true).read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        create.mode(0o600);
    }

    match create.open(path) {
        Ok(file) => {
            validate_metadata(&file.metadata()?)?;
            Ok(file)
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            open_existing_regular(path, true)
        }
        Err(error) => Err(error),
    }
}

fn read_stable_regular_file(path: &Path, max_bytes: u64) -> io::Result<Option<Vec<u8>>> {
    if max_bytes == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "durable store size limit must be positive",
        ));
    }

    let mut file = match open_existing_regular(path, false) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let expected = file_identity(&file.metadata()?)?;
    if expected.length > max_bytes {
        return Err(invalid_data("durable store exceeds configured size limit"));
    }

    let mut bytes = Vec::with_capacity(usize::try_from(expected.length).map_err(|_| {
        invalid_data("durable store length cannot be represented on this platform")
    })?);
    file.by_ref()
        .take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) != expected.length {
        return Err(invalid_data("durable store changed while it was read"));
    }
    if file_identity(&file.metadata()?)? != expected || !path_matches_file(path, &file)? {
        return Err(invalid_data("durable store changed while it was read"));
    }
    Ok(Some(bytes))
}

fn open_existing_regular(path: &Path, writable: bool) -> io::Result<File> {
    let before = fs::symlink_metadata(path)?;
    validate_metadata(&before)?;
    let before_identity = file_identity(&before)?;

    let mut options = OpenOptions::new();
    options.read(true).write(writable);
    configure_safe_open(&mut options);
    let file = options.open(path)?;
    validate_metadata(&file.metadata()?)?;
    let opened_identity = file_identity(&file.metadata()?)?;
    let after = fs::symlink_metadata(path)?;
    validate_metadata(&after)?;
    if before_identity != opened_identity || file_identity(&after)? != opened_identity {
        return Err(invalid_data("durable store path changed while it was opened"));
    }
    Ok(file)
}

#[cfg(unix)]
fn configure_safe_open(options: &mut OpenOptions) {
    use std::os::unix::fs::OpenOptionsExt;
    options.custom_flags(unix_nonblock_flag());
}

#[cfg(windows)]
fn configure_safe_open(options: &mut OpenOptions) {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
}

#[cfg(not(any(unix, windows)))]
fn configure_safe_open(_options: &mut OpenOptions) {}

#[cfg(any(target_os = "linux", target_os = "android"))]
const fn unix_nonblock_flag() -> i32 {
    0o4000
}

#[cfg(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd",
    target_os = "dragonfly",
    target_os = "openbsd",
    target_os = "netbsd"
))]
const fn unix_nonblock_flag() -> i32 {
    0x0000_0004
}

#[cfg(any(target_os = "solaris", target_os = "illumos"))]
const fn unix_nonblock_flag() -> i32 {
    0x0000_0080
}

#[cfg(all(
    unix,
    not(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "dragonfly",
        target_os = "openbsd",
        target_os = "netbsd",
        target_os = "solaris",
        target_os = "illumos"
    ))
))]
const fn unix_nonblock_flag() -> i32 {
    0
}

fn path_matches_file(path: &Path, file: &File) -> io::Result<bool> {
    let opened = file_identity(&file.metadata()?)?;
    let mut options = OpenOptions::new();
    options.read(true);
    configure_safe_open(&mut options);
    let current = options.open(path)?;
    validate_metadata(&current.metadata()?)?;
    Ok(file_identity(&current.metadata()?)? == opened)
}

fn split_destination(path: &Path) -> io::Result<(&Path, &OsStr)> {
    if path.as_os_str().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "durable store path must not be empty",
        ));
    }
    let file_name = path
        .file_name()
        .filter(|name| !name.is_empty())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "durable store path must name a file",
            )
        })?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    Ok((parent, file_name))
}

fn temporary_path(parent: &Path, file_name: &OsStr) -> PathBuf {
    let nonce = TEMP_NONCE.fetch_add(1, Ordering::Relaxed);
    parent.join(format!(
        ".{}.cex-tmp-{}-{nonce}",
        file_name.to_string_lossy(),
        std::process::id()
    ))
}

fn reject_unsafe_destination(path: &Path) -> io::Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    validate_metadata(&metadata)
}

fn validate_metadata(metadata: &Metadata) -> io::Result<()> {
    if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
        return Err(invalid_data(
            "durable store path must be a regular non-link file",
        ));
    }
    reject_platform_specific_destination(metadata)
}

#[cfg(unix)]
fn reject_platform_specific_destination(metadata: &Metadata) -> io::Result<()> {
    use std::os::unix::fs::MetadataExt;
    if metadata.nlink() != 1 {
        return Err(invalid_data(
            "durable store path must have exactly one hard link",
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn reject_platform_specific_destination(metadata: &Metadata) -> io::Result<()> {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(invalid_data("durable store path must not be a reparse point"));
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn reject_platform_specific_destination(_metadata: &Metadata) -> io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn file_identity(metadata: &Metadata) -> io::Result<FileIdentity> {
    use std::os::unix::fs::MetadataExt;
    validate_metadata(metadata)?;
    Ok(FileIdentity {
        key_a: metadata.dev(),
        key_b: metadata.ino(),
        length: metadata.size(),
        time_a: metadata.mtime() as u64,
        time_b: metadata.mtime_nsec() as u64,
        links: metadata.nlink(),
        attributes: u64::from(metadata.mode()),
    })
}

#[cfg(windows)]
fn file_identity(metadata: &Metadata) -> io::Result<FileIdentity> {
    use std::os::windows::fs::MetadataExt;
    validate_metadata(metadata)?;
    Ok(FileIdentity {
        key_a: metadata.creation_time(),
        key_b: metadata.last_write_time(),
        length: metadata.file_size(),
        time_a: metadata.last_access_time(),
        time_b: metadata.last_write_time(),
        links: 1,
        attributes: u64::from(metadata.file_attributes()),
    })
}

#[cfg(not(any(unix, windows)))]
fn file_identity(metadata: &Metadata) -> io::Result<FileIdentity> {
    use std::time::UNIX_EPOCH;
    validate_metadata(metadata)?;
    let modified = metadata
        .modified()?
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let created = metadata
        .created()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .unwrap_or_default();
    Ok(FileIdentity {
        key_a: created.as_secs(),
        key_b: u64::from(created.subsec_nanos()),
        length: metadata.len(),
        time_a: modified.as_secs(),
        time_b: u64::from(modified.subsec_nanos()),
        links: 1,
        attributes: 0,
    })
}

#[cfg(unix)]
fn replace_file(temporary: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(temporary, destination)
}

#[cfg(windows)]
fn replace_file(temporary: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x0000_0001;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x0000_0008;

    #[link(name = "kernel32")]
    extern "system" {
        fn MoveFileExW(
            existing_file_name: *const u16,
            new_file_name: *const u16,
            flags: u32,
        ) -> i32;
    }

    let temporary = temporary
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();

    let status = unsafe {
        MoveFileExW(
            temporary.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if status == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(any(unix, windows)))]
fn replace_file(temporary: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(temporary, destination)
}

#[cfg(unix)]
fn sync_parent_directory(parent: &Path) -> io::Result<()> {
    File::open(parent)?.sync_all()
}

#[cfg(not(unix))]
fn sync_parent_directory(_parent: &Path) -> io::Result<()> {
    Ok(())
}

fn invalid_data(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[derive(Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
    struct Counter {
        value: u64,
    }

    fn temporary_directory(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "cex-durable-store-{}-{nonce}-{name}",
            std::process::id()
        ));
        fs::create_dir_all(&directory).expect("create temporary directory");
        directory
    }

    #[test]
    fn publishes_complete_replacements() {
        let directory = temporary_directory("replace");
        let destination = directory.join("state.json");

        atomic_write(&destination, br#"{"generation":1}"#).expect("first publication");
        assert_eq!(
            fs::read(&destination).expect("read first publication"),
            br#"{"generation":1}"#
        );
        atomic_write(&destination, br#"{"generation":2,"complete":true}"#)
            .expect("replacement publication");
        assert_eq!(
            fs::read(&destination).expect("read replacement publication"),
            br#"{"generation":2,"complete":true}"#
        );

        fs::remove_dir_all(directory).expect("remove temporary directory");
    }

    #[test]
    fn locked_updates_reload_the_latest_generation() {
        let directory = temporary_directory("locked-update");
        let destination = directory.join("state.json");

        let (first, ()) = update_json(&destination, 1024, Counter::default(), |state| {
            state.value += 1;
        })
        .expect("first update");
        let (second, ()) = update_json(&destination, 1024, Counter::default(), |state| {
            state.value += 1;
        })
        .expect("second update");
        assert_eq!(first.value, 1);
        assert_eq!(second.value, 2);
        assert_eq!(
            read_json::<Counter>(&destination, 1024)
                .expect("read state")
                .expect("state exists")
                .value,
            2
        );

        fs::remove_dir_all(directory).expect("remove temporary directory");
    }

    #[test]
    fn corrupt_state_fails_closed_without_replacement() {
        let directory = temporary_directory("corrupt");
        let destination = directory.join("state.json");
        fs::write(&destination, b"not-json").expect("write corrupt fixture");
        assert!(update_json(&destination, 1024, Counter::default(), |state| {
            state.value += 1;
        })
        .is_err());
        assert_eq!(fs::read(&destination).expect("read corrupt fixture"), b"not-json");
        fs::remove_dir_all(directory).expect("remove temporary directory");
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symbolic_and_hard_link_destinations() {
        use std::os::unix::fs::symlink;

        let directory = temporary_directory("links");
        let target = directory.join("target.json");
        let symbolic = directory.join("symbolic.json");
        let hard = directory.join("hard.json");
        fs::write(&target, b"original").expect("write target");

        symlink(&target, &symbolic).expect("create symbolic link");
        assert!(atomic_write(&symbolic, b"replacement").is_err());
        assert_eq!(fs::read(&target).expect("read target"), b"original");

        fs::hard_link(&target, &hard).expect("create hard link");
        assert!(atomic_write(&hard, b"replacement").is_err());
        assert_eq!(fs::read(&target).expect("read target"), b"original");

        fs::remove_dir_all(directory).expect("remove temporary directory");
    }
}
