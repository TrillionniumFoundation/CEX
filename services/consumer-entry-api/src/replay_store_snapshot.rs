use std::{
    fs::{self, File, Metadata},
    io::Read,
    path::Path,
};

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

pub(super) fn read_stable_regular_file(
    path: &str,
    max_bytes: u64,
) -> Result<Vec<u8>, SnapshotError> {
    if path.is_empty() || max_bytes == 0 {
        return Err(SnapshotError);
    }
    let path = Path::new(path);
    let path_before = fs::symlink_metadata(path).map_err(|_| SnapshotError)?;
    let expected = snapshot_identity(&path_before).ok_or(SnapshotError)?;
    if expected.length > max_bytes {
        return Err(SnapshotError);
    }

    let mut file = File::open(path).map_err(|_| SnapshotError)?;
    let handle_before = file
        .metadata()
        .map_err(|_| SnapshotError)
        .and_then(|metadata| snapshot_identity(&metadata).ok_or(SnapshotError))?;
    if handle_before != expected {
        return Err(SnapshotError);
    }

    let capacity = usize::try_from(expected.length).map_err(|_| SnapshotError)?;
    let limit = max_bytes.checked_add(1).ok_or(SnapshotError)?;
    let mut bytes = Vec::with_capacity(capacity);
    file.by_ref()
        .take(limit)
        .read_to_end(&mut bytes)
        .map_err(|_| SnapshotError)?;
    if u64::try_from(bytes.len()).map_err(|_| SnapshotError)? > max_bytes {
        return Err(SnapshotError);
    }

    let handle_after = file
        .metadata()
        .map_err(|_| SnapshotError)
        .and_then(|metadata| snapshot_identity(&metadata).ok_or(SnapshotError))?;
    let path_after = fs::symlink_metadata(path)
        .map_err(|_| SnapshotError)
        .and_then(|metadata| snapshot_identity(&metadata).ok_or(SnapshotError))?;
    if handle_after != expected || path_after != expected {
        return Err(SnapshotError);
    }
    Ok(bytes)
}

#[cfg(unix)]
fn snapshot_identity(metadata: &Metadata) -> Option<SnapshotIdentity> {
    use std::os::unix::fs::MetadataExt;

    if !metadata.file_type().is_file() || metadata.nlink() != 1 {
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

#[cfg(windows)]
fn snapshot_identity(metadata: &Metadata) -> Option<SnapshotIdentity> {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    if !metadata.file_type().is_file()
        || metadata.file_type().is_symlink()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    {
        return None;
    }
    Some(SnapshotIdentity {
        key_a: metadata.creation_time(),
        key_b: 0,
        length: metadata.file_size(),
        write_time_a: metadata.last_write_time(),
        write_time_b: 0,
        change_time_a: 0,
        change_time_b: 0,
        links: 1,
        attributes: u64::from(metadata.file_attributes()),
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
}
