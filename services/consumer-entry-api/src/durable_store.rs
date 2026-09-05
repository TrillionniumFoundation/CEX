use std::{
    ffi::OsStr,
    fs::{self, File, Metadata, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

static TEMP_NONCE: AtomicU64 = AtomicU64::new(0);
const TEMP_CREATE_ATTEMPTS: usize = 64;

/// Publish one complete state snapshot without exposing a partially-written destination.
///
/// The temporary file is created in the destination directory, written with `create_new`,
/// flushed and fsynced, then atomically replaces the destination.  Existing symbolic links,
/// reparse points, non-regular files and multiply-linked files are rejected before publication.
/// On Unix the containing directory is fsynced after rename; on Windows `MoveFileExW` is used
/// with replacement and write-through semantics.
pub(super) fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
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

    fs::create_dir_all(parent)?;
    reject_unsafe_destination(path)?;

    for _ in 0..TEMP_CREATE_ATTEMPTS {
        let temporary = temporary_path(parent, file_name);
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);

        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options
                .mode(0o600)
                .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW);
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

            // Re-check the destination immediately before publication.  Replacing a path is safe,
            // but refusing a newly-introduced special file keeps the contract explicit and makes
            // configuration mistakes fail closed on every write.
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

    if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "durable store destination must be a regular non-link file",
        ));
    }

    reject_platform_specific_destination(&metadata)
}

#[cfg(unix)]
fn reject_platform_specific_destination(metadata: &Metadata) -> io::Result<()> {
    use std::os::unix::fs::MetadataExt;

    if metadata.nlink() != 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "durable store destination must have exactly one hard link",
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn reject_platform_specific_destination(metadata: &Metadata) -> io::Result<()> {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "durable store destination must not be a reparse point",
        ));
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn reject_platform_specific_destination(_metadata: &Metadata) -> io::Result<()> {
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

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
