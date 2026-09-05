//! Filesystem dates suitable for ordering discovered folders. Modification,
//! access and metadata-change times do not describe when a folder arrived.

use std::{io, path::Path};

/// Unix epoch milliseconds, with the filesystem fact they describe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderDate {
    AddedToDirectory(i64),
    Created(i64),
}

impl FolderDate {
    pub fn read(path: &Path) -> Result<Option<Self>, super::FolderScanError> {
        read(path).map_err(|error| super::FolderScanError::io(path, error))
    }

    pub(crate) fn columns(self) -> (i64, &'static str) {
        match self {
            Self::AddedToDirectory(at) => (at, "added_to_directory"),
            Self::Created(at) => (at, "created"),
        }
    }
}

fn read(path: &Path) -> io::Result<Option<FolderDate>> {
    #[cfg(target_os = "macos")]
    if let Some(at) = added_to_directory(path)? {
        return Ok(Some(FolderDate::AddedToDirectory(at)));
    }
    let metadata = std::fs::metadata(path)?;
    creation_date(path, metadata.created())
}

fn creation_date(
    path: &Path,
    created: io::Result<std::time::SystemTime>,
) -> io::Result<Option<FolderDate>> {
    match created {
        Ok(at) => Ok(Some(FolderDate::Created(
            chrono::DateTime::<chrono::Utc>::from(at).timestamp_millis(),
        ))),
        Err(error) if error.kind() == io::ErrorKind::Unsupported => {
            tracing::debug!(path = %path.display(), "folder creation time is unavailable");
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

#[cfg(target_os = "macos")]
fn added_to_directory(path: &Path) -> io::Result<Option<i64>> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};

    let name = CString::new(path.as_os_str().as_bytes())?;
    let mut attributes = libc::attrlist {
        bitmapcount: libc::ATTR_BIT_MAP_COUNT,
        reserved: 0,
        commonattr: libc::ATTR_CMN_RETURNED_ATTRS | libc::ATTR_CMN_ADDEDTIME,
        volattr: 0,
        dirattr: 0,
        fileattr: 0,
        forkattr: 0,
    };
    // getattrlist packs fields on four-byte boundaries, including timespec.
    // Requesting the returned mask distinguishes an unsupported date from
    // the zero-filled value FSOPT_PACK_INVAL_ATTRS puts in its place.
    let mut buffer = [0u8; 4 + 20 + 16];
    // SAFETY: name is NUL-terminated; attributes and buffer are writable for
    // the full sizes supplied and remain alive until the syscall returns.
    let result = unsafe {
        libc::getattrlist(
            name.as_ptr(),
            std::ptr::from_mut(&mut attributes).cast(),
            buffer.as_mut_ptr().cast(),
            buffer.len(),
            libc::FSOPT_PACK_INVAL_ATTRS,
        )
    };
    if result != 0 {
        let error = io::Error::last_os_error();
        if matches!(error.raw_os_error(), Some(libc::ENOTSUP | libc::EINVAL)) {
            tracing::debug!(path = %path.display(), %error, "folder Date Added is unsupported");
            return Ok(None);
        }
        return Err(error);
    }
    let date = decode_added_date(&buffer)?;
    if date.is_none() {
        tracing::debug!(path = %path.display(), "folder Date Added is unavailable");
    }
    Ok(date)
}

#[cfg(target_os = "macos")]
fn decode_added_date(buffer: &[u8; 40]) -> io::Result<Option<i64>> {
    let length = u32::from_ne_bytes(buffer[0..4].try_into().unwrap());
    if length != buffer.len() as u32 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid folder date attribute length",
        ));
    }
    let returned = u32::from_ne_bytes(buffer[4..8].try_into().unwrap());
    if returned & libc::ATTR_CMN_ADDEDTIME == 0 {
        return Ok(None);
    }
    let seconds = i64::from_ne_bytes(buffer[24..32].try_into().unwrap());
    let nanos = i64::from_ne_bytes(buffer[32..40].try_into().unwrap());
    let at = u32::try_from(nanos)
        .ok()
        .filter(|nanos| *nanos < 1_000_000_000)
        .and_then(|nanos| chrono::DateTime::from_timestamp(seconds, nanos))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid folder Date Added"))?;
    Ok(Some(at.timestamp_millis()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_creation_time_is_absent_but_io_errors_propagate() {
        let path = Path::new("folder");
        assert_eq!(
            creation_date(path, Err(io::ErrorKind::Unsupported.into())).unwrap(),
            None
        );
        assert_eq!(
            creation_date(path, Err(io::ErrorKind::PermissionDenied.into()))
                .unwrap_err()
                .kind(),
            io::ErrorKind::PermissionDenied
        );
        assert_eq!(
            creation_date(path, Ok(std::time::UNIX_EPOCH)).unwrap(),
            Some(FolderDate::Created(0))
        );
    }

    #[test]
    fn reads_an_existing_folder_and_reports_a_missing_one() {
        let directory = tempfile::tempdir().unwrap();
        FolderDate::read(directory.path()).unwrap();
        assert!(FolderDate::read(&directory.path().join("missing")).is_err());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn returned_mask_distinguishes_missing_date_from_epoch() {
        let mut buffer = [0u8; 40];
        buffer[..4].copy_from_slice(&40u32.to_ne_bytes());
        assert_eq!(decode_added_date(&buffer).unwrap(), None);
        buffer[4..8].copy_from_slice(&libc::ATTR_CMN_ADDEDTIME.to_ne_bytes());
        assert_eq!(decode_added_date(&buffer).unwrap(), Some(0));
        buffer[24..32].copy_from_slice(&123i64.to_ne_bytes());
        buffer[32..40].copy_from_slice(&456_000_000i64.to_ne_bytes());
        assert_eq!(decode_added_date(&buffer).unwrap(), Some(123_456));
        buffer[32..40].copy_from_slice(&1_000_000_000i64.to_ne_bytes());
        assert!(decode_added_date(&buffer).is_err());
        buffer[..4].copy_from_slice(&24u32.to_ne_bytes());
        assert!(decode_added_date(&buffer).is_err());
    }
}
