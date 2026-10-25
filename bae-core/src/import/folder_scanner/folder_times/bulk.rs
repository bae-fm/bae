//! The dates of every entry in one directory, read in one batch with
//! `getattrlistbulk(2)`: one round trip for a whole listing rather than one
//! per folder, which on a network share is what the dates cost.

use super::FolderTimes;
use std::collections::HashMap;
use std::ffi::OsString;
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::ffi::OsStringExt;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// `ATTR_CMN_ERROR` from `<sys/attr.h>`, which `libc` does not define: the
/// error that kept an entry's attributes from being read, returned right
/// after `ATTR_CMN_RETURNED_ATTRS`.
const ATTR_CMN_ERROR: libc::attrgroup_t = 0x2000_0000;

/// What is asked of each entry, in the order `getattrlistbulk` packs it
/// after the entry's length and its returned set: its error, its name, then
/// its created, modified and added dates.
const ASKED: libc::attrgroup_t = libc::ATTR_CMN_RETURNED_ATTRS
    | ATTR_CMN_ERROR
    | libc::ATTR_CMN_NAME
    | libc::ATTR_CMN_CRTIME
    | libc::ATTR_CMN_MODTIME
    | libc::ATTR_CMN_ADDEDTIME;

/// The dates of every entry in `directory` the system could read them for,
/// by name. A date the file system doesn't keep, such as date added on an
/// SMB share, is `None`; an entry whose attributes could not be read is
/// left out.
pub(super) fn read(directory: &Path) -> io::Result<HashMap<OsString, FolderTimes>> {
    let opened = std::fs::File::open(directory)?;
    let mut attributes = libc::attrlist {
        bitmapcount: libc::ATTR_BIT_MAP_COUNT,
        reserved: 0,
        commonattr: ASKED,
        volattr: 0,
        dirattr: 0,
        fileattr: 0,
        forkattr: 0,
    };
    let mut buffer = vec![0u8; 64 * 1024];
    let mut entries = HashMap::new();
    loop {
        // SAFETY: the descriptor is open for the call; attributes and buffer
        // are writable for the full sizes supplied and outlive the call.
        let count = unsafe {
            libc::getattrlistbulk(
                opened.as_raw_fd(),
                std::ptr::from_mut(&mut attributes).cast(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                0,
            )
        };
        if count < 0 {
            return Err(io::Error::last_os_error());
        }
        if count == 0 {
            return Ok(entries);
        }
        let mut rest = buffer.as_slice();
        for _ in 0..count {
            let (length, entry) = decode_entry(rest)?;
            if let Some((name, times)) = entry {
                entries.insert(name, times);
            }
            rest = &rest[length..];
        }
    }
}

/// The first entry packed in `buffer`: its length, and its name and dates,
/// or `None` when the system could not read its attributes.
fn decode_entry(buffer: &[u8]) -> io::Result<(usize, Option<(OsString, FolderTimes)>)> {
    let invalid = |what: &str| io::Error::new(io::ErrorKind::InvalidData, what.to_string());
    let length = u32_at(buffer, 0).ok_or_else(|| invalid("a bulk entry has no length"))? as usize;
    let entry = buffer
        .get(..length)
        .ok_or_else(|| invalid("a bulk entry runs past the buffer"))?;
    let returned =
        u32_at(entry, 4).ok_or_else(|| invalid("a bulk entry has no returned attributes"))?;
    // Past the length and the five groups of the returned set.
    let mut at = 4 + std::mem::size_of::<libc::attribute_set_t>();
    if returned & ATTR_CMN_ERROR != 0 {
        let error = u32_at(entry, at).ok_or_else(|| invalid("a bulk entry's error is cut off"))?;
        at += 4;
        if error != 0 {
            tracing::debug!(
                error = %io::Error::from_raw_os_error(error as i32),
                "a folder's dates are unreadable in its batch"
            );
            return Ok((length, None));
        }
    }
    if returned & libc::ATTR_CMN_NAME == 0 {
        return Ok((length, None));
    }
    let name = {
        let offset = u32_at(entry, at).ok_or_else(|| invalid("a bulk entry's name is cut off"))?
            as i32 as isize;
        let size = u32_at(entry, at + 4)
            .ok_or_else(|| invalid("a bulk entry's name is cut off"))? as usize;
        // The offset counts from the reference itself.
        let start = at
            .checked_add_signed(offset)
            .ok_or_else(|| invalid("a bulk entry's name is before it"))?;
        let bytes = entry
            .get(start..start.saturating_add(size))
            .ok_or_else(|| invalid("a bulk entry's name runs past it"))?;
        let bytes = bytes.strip_suffix(&[0]).unwrap_or(bytes);
        OsString::from_vec(bytes.to_vec())
    };
    at += std::mem::size_of::<libc::attrreference_t>();
    let mut date = |attribute: libc::attrgroup_t| -> io::Result<Option<SystemTime>> {
        if returned & attribute == 0 {
            return Ok(None);
        }
        let seconds = i64_at(entry, at).ok_or_else(|| invalid("a bulk entry's date is cut off"))?;
        let nanos = i64_at(entry, at + 8).ok_or_else(|| invalid("a bulk entry's date is cut off"))?;
        // A timespec, packed on a four-byte boundary like every attribute.
        at += 16;
        Ok(system_time(seconds, nanos))
    };
    let created = date(libc::ATTR_CMN_CRTIME)?;
    let modified = date(libc::ATTR_CMN_MODTIME)?;
    let added = date(libc::ATTR_CMN_ADDEDTIME)?;
    Ok((
        length,
        Some((
            name,
            FolderTimes {
                added,
                created,
                modified,
            },
        )),
    ))
}

fn u32_at(buffer: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_ne_bytes(buffer.get(at..at + 4)?.try_into().ok()?))
}

fn i64_at(buffer: &[u8], at: usize) -> Option<i64> {
    Some(i64::from_ne_bytes(buffer.get(at..at + 8)?.try_into().ok()?))
}

/// A timespec's instant; `None` for one no `SystemTime` holds.
fn system_time(seconds: i64, nanos: i64) -> Option<SystemTime> {
    let nanos = u32::try_from(nanos).ok().filter(|nanos| *nanos < 1_000_000_000)?;
    match u64::try_from(seconds) {
        Ok(seconds) => UNIX_EPOCH.checked_add(Duration::new(seconds, nanos)),
        Err(_) => UNIX_EPOCH
            .checked_sub(Duration::from_secs(seconds.unsigned_abs()))?
            .checked_add(Duration::from_nanos(nanos.into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One entry as `getattrlistbulk` packs it: returned set, then the
    /// attributes it names in order, the name's bytes at the end.
    fn packed(error: Option<u32>, name: &str, dates: &[(libc::attrgroup_t, i64, i64)]) -> Vec<u8> {
        let mut returned = libc::ATTR_CMN_RETURNED_ATTRS | libc::ATTR_CMN_NAME;
        let mut body = Vec::new();
        if let Some(error) = error {
            returned |= ATTR_CMN_ERROR;
            body.extend_from_slice(&error.to_ne_bytes());
        }
        let reference_at = body.len();
        body.extend_from_slice(&[0; 8]);
        for (attribute, seconds, nanos) in dates {
            returned |= attribute;
            body.extend_from_slice(&seconds.to_ne_bytes());
            body.extend_from_slice(&nanos.to_ne_bytes());
        }
        let name_at = body.len();
        body.extend_from_slice(name.as_bytes());
        body.push(0);
        while body.len() % 4 != 0 {
            body.push(0);
        }
        let offset = (name_at - reference_at) as u32;
        body[reference_at..reference_at + 4].copy_from_slice(&offset.to_ne_bytes());
        body[reference_at + 4..reference_at + 8]
            .copy_from_slice(&(name.len() as u32 + 1).to_ne_bytes());
        let mut entry = Vec::new();
        entry.extend_from_slice(&((4 + 20 + body.len()) as u32).to_ne_bytes());
        entry.extend_from_slice(&returned.to_ne_bytes());
        entry.extend_from_slice(&[0; 16]);
        entry.extend_from_slice(&body);
        entry
    }

    #[test]
    fn decodes_the_dates_an_entry_returns_and_none_for_the_rest() {
        let entry = packed(
            Some(0),
            "Folder",
            &[
                (libc::ATTR_CMN_CRTIME, 20, 0),
                (libc::ATTR_CMN_MODTIME, 30, 500),
            ],
        );
        let (length, decoded) = decode_entry(&entry).unwrap();
        assert_eq!(length, entry.len());
        let (name, times) = decoded.unwrap();
        assert_eq!(name, "Folder");
        assert_eq!(
            times,
            FolderTimes {
                added: None,
                created: Some(UNIX_EPOCH + Duration::from_secs(20)),
                modified: Some(UNIX_EPOCH + Duration::new(30, 500)),
            }
        );
    }

    #[test]
    fn an_entry_with_an_error_has_no_dates() {
        let entry = packed(Some(libc::EACCES as u32), "Folder", &[]);
        assert_eq!(decode_entry(&entry).unwrap(), (entry.len(), None));
    }

    #[test]
    fn a_cut_off_entry_is_invalid() {
        let entry = packed(None, "Folder", &[(libc::ATTR_CMN_ADDEDTIME, 1, 0)]);
        assert!(decode_entry(&entry[..entry.len() - 8]).is_err());
    }

    #[test]
    fn reads_the_names_and_dates_the_listing_holds() {
        let directory = tempfile::tempdir().unwrap();
        let mut names: Vec<String> = (0..300).map(|index| format!("Folder {index:03}")).collect();
        for name in &names {
            std::fs::create_dir(directory.path().join(name)).unwrap();
        }
        std::fs::write(directory.path().join("file.txt"), b"x").unwrap();
        names.push("file.txt".to_string());
        let entries = read(directory.path()).unwrap();
        let mut read_names: Vec<String> = entries
            .keys()
            .map(|name| name.to_string_lossy().into_owned())
            .collect();
        read_names.sort();
        names.sort();
        assert_eq!(read_names, names);
        let metadata = std::fs::metadata(directory.path().join("Folder 000")).unwrap();
        let times = entries[std::ffi::OsStr::new("Folder 000")];
        assert_eq!(times.created, metadata.created().ok());
        assert_eq!(times.modified, metadata.modified().ok());
        assert!(times.added.is_some());
    }

    #[test]
    fn a_missing_directory_fails() {
        let directory = tempfile::tempdir().unwrap();
        assert!(read(&directory.path().join("missing")).is_err());
    }
}
