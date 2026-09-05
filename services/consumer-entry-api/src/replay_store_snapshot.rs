use std::{
    fs::{File, OpenOptions},
    io::Read,
    path::Path,
};

#[cfg(not(windows))]
use std::fs::Metadata;
#[cfg(any(test, not(any(unix, windows))))]
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SnapshotIdentity {
    key_a: u64,
    key_b: u64,
    length: u64,
    write_time_a: u64,
    write_time_b: u64,
    change_time_a: u64,
    change_time_b: u64,
    links: u64,
    attributes: u64,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct SnapshotError;

/// Open and read one immutable view of a regular file.
///
/// A pre-open path identity, the open handle, and independently-opened path handles before and
/// after the read must all identify the same object. Unix opens use `O_NONBLOCK`, preventing a
/// path substitution with a FIFO from hanging the endpoint. Windows opens the reparse point itself
/// and rejects it rather than following it.
pub(super) fn read_stable_regular_file(
    path: &str,
    max_bytes: u64,
) -> Result<Vec<u8>, SnapshotError> {
    if path.is_empty() || max_bytes == 0 {
        return Err(SnapshotError);
    }
    let path = Path::new(path);

    let before = path_identity(path)?;
    let mut file = open_snapshot(path)?;
    let expected = file_identity(&file)?;
    if expected != before || expected.length > max_bytes {
        return Err(SnapshotError);
    }

    let path_before = open_snapshot(path)?;
    if file_identity(&path_before)? != expected {
        return Err(SnapshotError);
    }
    drop(path_before);

    let capacity = usize::try_from(expected.length).map_err(|_| SnapshotError)?;
    let limit = max_bytes.checked_add(1).ok_or(SnapshotError)?;
    let mut bytes = Vec::with_capacity(capacity);
    file.by_ref()
        .take(limit)
        .read_to_end(&mut bytes)
        .map_err(|_| SnapshotError)?;
    let observed_length = u64::try_from(bytes.len()).map_err(|_| SnapshotError)?;
    if observed_length > max_bytes || observed_length != expected.length {
        return Err(SnapshotError);
    }

    if file_identity(&file)? != expected || path_identity(path)? != expected {
        return Err(SnapshotError);
    }
    let path_after = open_snapshot(path)?;
    if file_identity(&path_after)? != expected {
        return Err(SnapshotError);
    }
    Ok(bytes)
}

fn open_snapshot(path: &Path) -> Result<File, SnapshotError> {
    let mut options = OpenOptions::new();
    options.read(true);
    configure_safe_open(&mut options);
    let file = options.open(path).map_err(|_| SnapshotError)?;
    // Validate the opened object before reading any bytes.
    file_identity(&file)?;
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

#[cfg(unix)]
fn path_identity(path: &Path) -> Result<SnapshotIdentity, SnapshotError> {
    snapshot_identity(&std::fs::symlink_metadata(path).map_err(|_| SnapshotError)?)
        .ok_or(SnapshotError)
}

#[cfg(windows)]
fn path_identity(path: &Path) -> Result<SnapshotIdentity, SnapshotError> {
    let file = open_snapshot(path)?;
    file_identity(&file)
}

#[cfg(not(any(unix, windows)))]
fn path_identity(path: &Path) -> Result<SnapshotIdentity, SnapshotError> {
    snapshot_identity(&std::fs::symlink_metadata(path).map_err(|_| SnapshotError)?)
        .ok_or(SnapshotError)
}

#[cfg(unix)]
fn file_identity(file: &File) -> Result<SnapshotIdentity, SnapshotError> {
    snapshot_identity(&file.metadata().map_err(|_| SnapshotError)?).ok_or(SnapshotError)
}

#[cfg(windows)]
fn file_identity(file: &File) -> Result<SnapshotIdentity, SnapshotError> {
    use std::{ffi::c_void, mem::MaybeUninit, os::windows::io::AsRawHandle};

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct FileTime {
        low: u32,
        high: u32,
    }

    #[repr(C)]
    struct ByHandleFileInformation {
        file_attributes: u32,
        creation_time: FileTime,
        last_access_time: FileTime,
        last_write_time: FileTime,
        volume_serial_number: u32,
        file_size_high: u32,
        file_size_low: u32,
        number_of_links: u32,
        file_index_high: u32,
        file_index_low: u32,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GetFileInformationByHandle(
            file: *mut c_void,
            information: *mut ByHandleFileInformation,
        ) -> i32;
    }

    const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x0000_0010;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;

    let mut information = MaybeUninit::<ByHandleFileInformation>::uninit();
    let status = unsafe {
        GetFileInformationByHandle(file.as_raw_handle().cast(), information.as_mut_ptr())
    };
    if status == 0 {
        return Err(SnapshotError);
    }
    let information = unsafe { information.assume_init() };
    if information.file_attributes & (FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT) != 0
        || information.number_of_links != 1
    {
        return Err(SnapshotError);
    }

    Ok(SnapshotIdentity {
        key_a: u64::from(information.volume_serial_number),
        key_b: combine_u32(information.file_index_high, information.file_index_low),
        length: combine_u32(information.file_size_high, information.file_size_low),
        write_time_a: combine_u32(
            information.last_write_time.high,
            information.last_write_time.low,
        ),
        write_time_b: 0,
        change_time_a: combine_u32(
            information.creation_time.high,
            information.creation_time.low,
        ),
        change_time_b: 0,
        links: u64::from(information.number_of_links),
        attributes: u64::from(information.file_attributes),
    })
}

#[cfg(windows)]
const fn combine_u32(high: u32, low: u32) -> u64 {
    (u64::from(high) << 32) | u64::from(low)
}

#[cfg(not(any(unix, windows)))]
fn file_identity(file: &File) -> Result<SnapshotIdentity, SnapshotError> {
    snapshot_identity(&file.metadata().map_err(|_| SnapshotError)?).ok_or(SnapshotError)
}

#[cfg(unix)]
fn snapshot_identity(metadata: &Metadata) -> Option<SnapshotIdentity> {
    use std::os::unix::fs::MetadataExt;

    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() || metadata.nlink() != 1
    {
        return None;
    }
    Some(SnapshotIdentity {
        key_a: metadata.dev(),
        key_b: metadata.ino(),
        length: metadata.size(),
        write_time_a: metadata.mtime() as u64,
        write_time_b: metadata.mtime_nsec() as u64,
        change_time_a: metadata.ctime() as u64,
        change_time_b: metadata.ctime_nsec() as u64,
        links: metadata.nlink(),
        attributes: u64::from(metadata.mode()),
    })
}

#[cfg(not(any(unix, windows)))]
fn snapshot_identity(metadata: &Metadata) -> Option<SnapshotIdentity> {
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return None;
    }
    let (modified_seconds, modified_nanos) = system_time_parts(metadata.modified().ok()?);
    let (created_seconds, created_nanos) = metadata
        .created()
        .ok()
        .map(system_time_parts)
        .unwrap_or_default();
    Some(SnapshotIdentity {
        key_a: created_seconds,
        key_b: u64::from(created_nanos),
        length: metadata.len(),
        write_time_a: modified_seconds,
        write_time_b: u64::from(modified_nanos),
        change_time_a: 0,
        change_time_b: 0,
        links: 1,
        attributes: 0,
    })
}

#[cfg(not(any(unix, windows)))]
fn system_time_parts(time: SystemTime) -> (u64, u32) {
    let duration = time.duration_since(UNIX_EPOCH).unwrap_or_default();
    (duration.as_secs(), duration.subsec_nanos())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::PathBuf};

    fn temporary_path(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "cex-replay-snapshot-{}-{nonce}-{name}",
            std::process::id()
        ))
    }

    #[test]
    fn reads_one_stable_regular_file() {
        let path = temporary_path("stable.json");
        fs::write(&path, br#"{"seen":{}}"#).expect("write fixture");
        let bytes = read_stable_regular_file(path.to_str().expect("UTF-8 path"), 1024)
            .expect("stable file");
        assert_eq!(bytes, br#"{"seen":{}}"#);
        fs::remove_file(path).expect("remove fixture");
    }

    #[test]
    fn rejects_oversized_input() {
        let path = temporary_path("oversized.json");
        fs::write(&path, b"123").expect("write fixture");
        assert!(read_stable_regular_file(path.to_str().expect("UTF-8 path"), 2).is_err());
        fs::remove_file(path).expect("remove fixture");
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symbolic_and_hard_linked_inputs() {
        use std::os::unix::fs::symlink;

        let target = temporary_path("target.json");
        let symbolic = temporary_path("symbolic.json");
        let hard = temporary_path("hard.json");
        fs::write(&target, br#"{"seen":{}}"#).expect("write target");
        symlink(&target, &symbolic).expect("create symbolic link");
        assert!(
            read_stable_regular_file(symbolic.to_str().expect("UTF-8 path"), 1024).is_err()
        );
        fs::hard_link(&target, &hard).expect("create hard link");
        assert!(read_stable_regular_file(target.to_str().expect("UTF-8 path"), 1024).is_err());
        fs::remove_file(symbolic).expect("remove symbolic link");
        fs::remove_file(hard).expect("remove hard link");
        fs::remove_file(target).expect("remove target");
    }

    #[cfg(unix)]
    #[test]
    fn rejects_fifo_without_blocking() {
        use std::process::Command;

        let fifo = temporary_path("fifo");
        let status = Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .expect("execute mkfifo");
        assert!(status.success(), "mkfifo failed with {status}");
        assert!(read_stable_regular_file(fifo.to_str().expect("UTF-8 path"), 1024).is_err());
        fs::remove_file(fifo).expect("remove FIFO");
    }
}
