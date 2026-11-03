//! A folder's dates as macOS's attribute calls give them: `getattrlist(2)`
//! for one folder's date added, and `getattrlistbulk(2)` for the dates of
//! every entry in a directory in one batch — one round trip for a whole
//! listing rather than one per folder, which on a network share is what the
//! dates cost. Both pack their answers the same way, and [`decode`] reads
//! them.

use super::FolderTimes;
use std::collections::HashMap;
use std::ffi::OsString;
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// `ATTR_CMN_ERROR` from `<sys/attr.h>`, which `libc` does not define: the
/// error that kept an entry's attributes from being read, which
/// `getattrlistbulk` packs right after `ATTR_CMN_RETURNED_ATTRS`.
const ATTR_CMN_ERROR: libc::attrgroup_t = 0x2000_0000;

/// What a batch asks of each entry: its error, its name, and its created,
/// modified and added dates.
const ASKED_OF_EACH: libc::attrgroup_t = libc::ATTR_CMN_RETURNED_ATTRS
    | ATTR_CMN_ERROR
    | libc::ATTR_CMN_NAME
    | libc::ATTR_CMN_CRTIME
    | libc::ATTR_CMN_MODTIME
    | libc::ATTR_CMN_ADDEDTIME;

fn attribute_list(common: libc::attrgroup_t) -> libc::attrlist {
    libc::attrlist {
        bitmapcount: libc::ATTR_BIT_MAP_COUNT,
        reserved: 0,
        commonattr: common,
        volattr: 0,
        dirattr: 0,
        fileattr: 0,
        forkattr: 0,
    }
}

/// When the folder at `path` was added to the folder it is in; `None` where
/// its file system keeps no date added.
pub(super) fn added(path: &Path) -> io::Result<Option<SystemTime>> {
    let name = std::ffi::CString::new(path.as_os_str().as_bytes())?;
    let mut attributes = attribute_list(libc::ATTR_CMN_RETURNED_ATTRS | libc::ATTR_CMN_ADDEDTIME);
    // The length, the returned set and one timespec. Packing even what the
    // file system doesn't keep holds the layout fixed; the returned set
    // says whether the date is one.
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
    let added = decode(&buffer)?.times.added;
    if added.is_none() {
        tracing::debug!(path = %path.display(), "folder Date Added is unavailable");
    }
    Ok(added)
}

/// The dates of every entry in `directory` the system could read them for,
/// by name. A date the file system doesn't keep, such as date added on an
/// SMB share, is `None`; an entry whose attributes could not be read is
/// left out.
pub(super) fn read_directory(directory: &Path) -> io::Result<HashMap<OsString, FolderTimes>> {
    let opened = std::fs::File::open(directory)?;
    let mut attributes = attribute_list(ASKED_OF_EACH);
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
            let packed = decode(rest)?;
            match packed {
                Packed { error: Some(error), .. } => tracing::debug!(
                    error = %io::Error::from_raw_os_error(error),
                    "a folder's dates are unreadable in its batch"
                ),
                Packed { name: Some(name), times, .. } => {
                    entries.insert(name, times);
                }
                Packed { name: None, .. } => {}
            }
            rest = &rest[packed.length..];
        }
    }
}

/// One answer as the attribute calls pack it.
#[derive(Debug, PartialEq)]
struct Packed {
    /// How many bytes it takes.
    length: usize,
    /// Why its attributes could not be read, when they could not.
    error: Option<i32>,
    name: Option<OsString>,
    times: FolderTimes,
}

/// The answer packed first in `buffer`: its length, then the attributes the
/// system returned, by the returned set — its error, name, and created,
/// modified and added dates, in that order, each there only when returned.
/// A date no `SystemTime` holds, or an answer cut off, is invalid data.
fn decode(buffer: &[u8]) -> io::Result<Packed> {
    let invalid = |what: &str| io::Error::new(io::ErrorKind::InvalidData, what.to_string());
    let length =
        u32_at(buffer, 0).ok_or_else(|| invalid("an attribute answer has no length"))? as usize;
    // Past the length and the five groups of the returned set.
    let mut at = 4 + std::mem::size_of::<libc::attribute_set_t>();
    if length < at {
        return Err(invalid("an attribute answer is shorter than its returned set"));
    }
    let entry =
        buffer.get(..length).ok_or_else(|| invalid("an attribute answer runs past the buffer"))?;
    let returned = u32_at(entry, 4).expect("the length covers the returned set");
    let mut error = None;
    if returned & ATTR_CMN_ERROR != 0 {
        let code = u32_at(entry, at).ok_or_else(|| invalid("an answer's error is cut off"))?;
        at += 4;
        if code != 0 {
            error = Some(code as i32);
        }
    }
    let mut name = None;
    if returned & libc::ATTR_CMN_NAME != 0 {
        let cut_off = || invalid("an answer's name is cut off");
        let offset = u32_at(entry, at).ok_or_else(cut_off)? as i32 as isize;
        let size = u32_at(entry, at + 4).ok_or_else(cut_off)? as usize;
        // The offset counts from the reference itself.
        let start = at
            .checked_add_signed(offset)
            .ok_or_else(|| invalid("an answer's name is before it"))?;
        let bytes = entry
            .get(start..start.saturating_add(size))
            .ok_or_else(|| invalid("an answer's name runs past it"))?;
        let bytes = bytes.strip_suffix(&[0]).unwrap_or(bytes);
        name = Some(OsString::from_vec(bytes.to_vec()));
        at += std::mem::size_of::<libc::attrreference_t>();
    }
    let mut date = |attribute: libc::attrgroup_t| -> io::Result<Option<SystemTime>> {
        if returned & attribute == 0 {
            return Ok(None);
        }
        let cut_off = || invalid("an answer's date is cut off");
        let seconds = i64_at(entry, at).ok_or_else(cut_off)?;
        let nanos = i64_at(entry, at + 8).ok_or_else(cut_off)?;
        // A timespec, packed on a four-byte boundary like every attribute.
        at += 16;
        system_time(seconds, nanos).map(Some).ok_or_else(|| invalid("an answer's date is no time"))
    };
    let created = date(libc::ATTR_CMN_CRTIME)?;
    let modified = date(libc::ATTR_CMN_MODTIME)?;
    let added = date(libc::ATTR_CMN_ADDEDTIME)?;
    Ok(Packed { length, error, name, times: FolderTimes { added, created, modified } })
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
#[path = "attributes_tests.rs"]
mod tests;
