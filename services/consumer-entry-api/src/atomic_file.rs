use crate::real_std::{
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

#[cfg(unix)]
use crate::real_std::os::unix::fs::OpenOptionsExt;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);

fn validate_parent_chain(parent: &Path) -> io::Result<()> {
    let mut cursor = Some(parent);
    while let Some(path) = cursor {
        if path.as_os_str().is_empty() {
            break;
        }
        match fs::symlink_metadata(path) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "atomic publication parent must be a real directory",
                    ));
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        cursor = path.parent();
    }
    Ok(())
}

fn temporary_path(path: &Path, sequence: u64) -> io::Result<PathBuf> {
    let parent = path
        .parent()
        .filter(|value| !value.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = path.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "atomic publication path has no file name",
        )
    })?;
    let mut temporary = OsString::from(".");
    temporary.push(name);
    temporary.push(format!(
        ".cex-atomic-{}-{sequence}.tmp",
        crate::real_std::process::id()
    ));
    Ok(parent.join(temporary))
}

fn create_temporary(path: &Path) -> io::Result<(PathBuf, File)> {
    for _ in 0..64 {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temporary = temporary_path(path, sequence)?;
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        match options.open(&temporary) {
            Ok(file) => return Ok((temporary, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "atomic publication could not allocate a unique temporary file",
    ))
}

#[cfg(unix)]
fn publish(temporary: &Path, destination: &Path, parent: &Path) -> io::Result<()> {
    fs::rename(temporary, destination)?;
    File::open(parent)?.sync_all()
}

#[cfg(windows)]
fn publish(temporary: &Path, destination: &Path, _parent: &Path) -> io::Result<()> {
    use crate::real_std::os::windows::ffi::OsStrExt;

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x0000_0001;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x0000_0008;

    #[link(name = "Kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(existing: *const u16, destination: *const u16, flags: u32) -> i32;
    }

    fn wide(path: &Path) -> Vec<u16> {
        path.as_os_str().encode_wide().chain(Some(0)).collect()
    }

    let existing = wide(temporary);
    let destination = wide(destination);
    // SAFETY: both buffers are live, NUL-terminated UTF-16 paths.
    let result = unsafe {
        MoveFileExW(
            existing.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn publish(_temporary: &Path, _destination: &Path, _parent: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "atomic durable publication is unsupported on this platform",
    ))
}

pub fn atomic_write<P: AsRef<Path>, C: AsRef<[u8]>>(path: P, contents: C) -> io::Result<()> {
    let path = path.as_ref();
    if path.as_os_str().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "atomic publication path is empty",
        ));
    }
    let parent = path
        .parent()
        .filter(|value| !value.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));

    validate_parent_chain(parent)?;
    fs::create_dir_all(parent)?;
    validate_parent_chain(parent)?;

    let (temporary, mut file) = create_temporary(path)?;
    let result = (|| {
        file.write_all(contents.as_ref())?;
        file.flush()?;
        file.sync_all()?;
        drop(file);
        publish(&temporary, path, parent)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary_root(name: &str) -> PathBuf {
        crate::real_std::env::temp_dir().join(format!(
            "consumer-entry-atomic-file-{name}-{}-{}",
            crate::real_std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn replaces_complete_contents() {
        let root = temporary_root("replace");
        let path = root.join("snapshot.json");
        atomic_write(&path, b"first").expect("first publication");
        atomic_write(&path, b"second").expect("second publication");
        assert_eq!(fs::read(&path).expect("read publication"), b"second");
        let entries = fs::read_dir(&root).expect("read directory").count();
        assert_eq!(entries, 1, "temporary files must be removed");
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinked_parent_directories() {
        use crate::real_std::os::unix::fs::symlink;
        let root = temporary_root("symlink-parent");
        let real = root.join("real");
        let linked = root.join("linked");
        fs::create_dir_all(&real).expect("create real directory");
        symlink(&real, &linked).expect("create symlink");
        assert!(atomic_write(linked.join("snapshot.json"), b"data").is_err());
        let _ = fs::remove_dir_all(root);
    }
}
