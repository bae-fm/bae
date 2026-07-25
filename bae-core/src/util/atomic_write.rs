use std::ffi::OsString;
use std::io::{Error, Write};
use std::path::Path;

/// A failed write, tagged with whether the write had already committed (the
/// rename landed and readers see the new bytes; only post-commit durability
/// work failed) or not (the target file is untouched).
#[derive(Debug)]
pub(crate) enum WriteError<E> {
    BeforeCommit(E),
    AfterCommit(E),
}

impl<E> WriteError<E> {
    pub(crate) fn committed(&self) -> bool {
        matches!(self, Self::AfterCommit(_))
    }

    pub(crate) fn into_inner(self) -> E {
        match self {
            Self::BeforeCommit(e) | Self::AfterCommit(e) => e,
        }
    }

    /// Convert the payload, preserving the commit phase.
    pub(crate) fn map<F>(self, f: impl FnOnce(E) -> F) -> WriteError<F> {
        match self {
            Self::BeforeCommit(e) => WriteError::BeforeCommit(f(e)),
            Self::AfterCommit(e) => WriteError::AfterCommit(f(e)),
        }
    }
}

pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), WriteError<std::io::Error>> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let file_name = path.file_name().ok_or_else(|| {
        WriteError::BeforeCommit(Error::new(
            std::io::ErrorKind::InvalidInput,
            "atomic write target has no file name",
        ))
    })?;

    let mut temp_prefix = OsString::from(".");
    temp_prefix.push(file_name);
    temp_prefix.push(".");

    let mut temp = tempfile::Builder::new()
        .prefix(&temp_prefix)
        .suffix(".tmp")
        .tempfile_in(parent)
        .map_err(WriteError::BeforeCommit)?;
    temp.write_all(bytes).map_err(WriteError::BeforeCommit)?;
    temp.as_file()
        .sync_all()
        .map_err(WriteError::BeforeCommit)?;
    temp.persist(path)
        .map_err(|e| WriteError::BeforeCommit(e.error))?;
    sync_parent_dir(parent).map_err(WriteError::AfterCommit)
}

pub(crate) fn write_atomic_io(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    write_atomic(path, bytes).map_err(WriteError::into_inner)
}

#[cfg(unix)]
fn sync_parent_dir(parent: &Path) -> std::io::Result<()> {
    std::fs::File::open(parent)?.sync_all()
}

#[cfg(windows)]
fn sync_parent_dir(_parent: &Path) -> std::io::Result<()> {
    // The POSIX parent-directory fsync idiom does not translate: FlushFileBuffers
    // on a directory handle needs write access, which a read-only open lacks, so
    // the previous implementation failed every atomic write with ERROR_ACCESS_DENIED
    // — os error 5 with no path, the launch-blocking "create library" failure.
    // NTFS journals metadata itself; durability of the rename does not hang on a
    // directory flush the way POSIX rename durability does, which is why database
    // engines skip directory syncing on Windows entirely.
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn sync_parent_dir(_parent: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn write_atomic_replaces_file_bytes() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config.yaml");
        std::fs::write(&path, b"old config").unwrap();

        write_atomic(&path, b"new config").unwrap();

        assert_eq!(std::fs::read(&path).unwrap(), b"new config");
    }

    #[test]
    fn write_atomic_requires_existing_parent() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("missing").join("config.yaml");

        let err = write_atomic(&path, b"new config").unwrap_err().into_inner();

        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
        assert!(!path.exists());
    }

    #[test]
    fn write_atomic_failure_leaves_target_and_removes_temp() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config.yaml");
        std::fs::create_dir(&path).unwrap();

        let err = write_atomic(&path, b"new config").unwrap_err().into_inner();

        // The rename onto a path already held by a directory fails, but the kind
        // is the OS's call: `AlreadyExists`/`IsADirectory` on Unix, and
        // `PermissionDenied` on Windows (`MoveFileEx` returns ERROR_ACCESS_DENIED,
        // os error 5, when the destination is a directory).
        assert!(matches!(
            err.kind(),
            std::io::ErrorKind::AlreadyExists
                | std::io::ErrorKind::IsADirectory
                | std::io::ErrorKind::PermissionDenied
        ));
        assert!(path.is_dir());
        let temp_names: Vec<_> = std::fs::read_dir(tmp.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .filter(|name| name.to_string_lossy().starts_with(".config.yaml."))
            .collect();
        assert!(temp_names.is_empty());
    }
}
