//! Which kind of volume a watched folder lives on.
//!
//! It decides how the folder is watched. A folder on this machine's own disk
//! has a filesystem watch that reports every change to it, whoever made it. On
//! macOS and Linux a folder on a network volume does not: the events a client
//! mount produces there are the ones this machine causes, so a change made on
//! the server or by another client arrives nowhere. Watching such a folder
//! would be claiming a reliability the mount cannot give, so it is checked
//! instead. Windows' `ReadDirectoryChangesW` does report a server's changes on
//! an SMB share, but a Windows network drive is checked the same way.

use std::path::Path;

use tracing::{debug, warn};

/// How often a watched folder on a network volume is checked, and so how long
/// a change made there from elsewhere can go unnoticed. Named here because the list tells the user
/// this number and it must be the one the coordinator actually uses.
pub(crate) const CHECK_PERIOD: std::time::Duration = std::time::Duration::from_secs(15 * 60);

/// The same interval in whole minutes, for the line the list shows.
pub fn check_period_minutes() -> u32 {
    (CHECK_PERIOD.as_secs() / 60) as u32
}

/// Where a watched folder's files actually are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VolumeKind {
    /// A disk attached to this machine. Its filesystem watch reports every
    /// change to it, so the watch is the change source.
    Local,
    /// A volume served over the network. On macOS and Linux its filesystem
    /// watch reports only what this machine does to it, so the folder is
    /// checked on a schedule instead — cheaply, by asking each directory
    /// whether it has been touched. Windows is checked the same way.
    Network,
}

/// The volume `path` lives on, or `Local` where this platform will not say.
///
/// Only a network folder is checked on a schedule: a local one's watch reports
/// every change, and says so when it loses track. So a network volume read as
/// local is a folder whose changes made elsewhere go unnoticed, and each
/// platform below answers from what the system says of the volume rather than
/// from how its path is spelled wherever it can.
///
/// Asking can block for as long as a network mount takes to answer — a share
/// that dropped off can take minutes — so the question is put on a blocking
/// thread. A question that panics is reported to `diagnostics` and answered
/// `Local`, as a platform that will not say is.
pub(crate) async fn volume_kind(
    path: &Path,
    diagnostics: &bae_diagnostics::Diagnostics,
) -> VolumeKind {
    let path = path.to_path_buf();
    match diagnostics
        .blocking("read which volume a folder is on", move || platform::volume_kind(&path))
        .await
    {
        Ok(kind) => kind,
        // Reported as it failed; the folder is read as a platform that will
        // not say is.
        Err(failed) => {
            debug!("{failed}; reading the folder as local");
            VolumeKind::Local
        }
    }
}

/// When this directory was last touched, in nanoseconds since the epoch, or
/// `None` where the platform will not say.
///
/// As fine as the filesystem will report, because the comparison is always
/// between two readings of the same directory on the same filesystem — and
/// rounding to the second would hide every change made in the same second as
/// the walk that recorded it, which is a change nothing would then notice.
pub(crate) fn directory_modified_at(path: &Path) -> Option<i64> {
    let modified = match std::fs::metadata(path).and_then(|metadata| metadata.modified()) {
        Ok(modified) => modified,
        // A directory that is gone has no time; the caller counts it touched.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(error) => {
            warn!(
                "can't read when {} was last touched, so it counts as touched: {error}",
                path.display()
            );
            return None;
        }
    };
    match modified.duration_since(std::time::UNIX_EPOCH) {
        Ok(since) => i64::try_from(since.as_nanos()).ok(),
        // A timestamp before 1970 is nothing a scan should reason about, but it
        // is a real reading and has to compare equal to itself.
        Err(before) => i64::try_from(before.duration().as_nanos()).ok().map(|nanos| -nanos),
    }
}

/// Which of the directories a walk recorded — each a path and the mtime it
/// had — have been touched since, or `None` when nothing was recorded.
///
/// A directory's mtime moves when a file in it is created, removed or renamed,
/// and creating a directory moves its parent's, so the recorded set answers for
/// the whole tree below the root. A directory that is gone is touched. An empty
/// set is a root nothing was recorded for, which is not an answer at all, and
/// the caller walks.
pub(crate) fn changed_directories(
    directories: &[(std::path::PathBuf, i64)],
) -> Option<Vec<std::path::PathBuf>> {
    if directories.is_empty() {
        return None;
    }
    Some(
        directories
            .iter()
            .filter(|(path, recorded)| directory_modified_at(path) != Some(*recorded))
            .map(|(path, _)| path.clone())
            .collect(),
    )
}

/// The filesystem `path` is on, as `statfs` describes it, or `None` when the
/// call fails.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn statfs(path: &Path) -> Option<libc::statfs> {
    let c_path = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).ok()?;
    // SAFETY: `c_path` is a NUL-terminated path that outlives the call, and
    // `stats` is a zeroed `statfs` the call fills in. Nothing is read from
    // it unless the call reported success.
    unsafe {
        let mut stats: libc::statfs = std::mem::zeroed();
        (libc::statfs(c_path.as_ptr(), &mut stats) == 0).then_some(stats)
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::{statfs, VolumeKind};
    use std::path::Path;

    /// Darwin's `statfs` sets `MNT_LOCAL` for a volume on hardware attached to
    /// this machine, and leaves it clear for everything mounted over a network
    /// — SMB, AFP, NFS, WebDAV alike. One call, no filesystem-name table to
    /// keep up to date.
    pub(super) fn volume_kind(path: &Path) -> VolumeKind {
        let Some(stats) = statfs(path) else {
            return VolumeKind::Local;
        };
        if stats.f_flags & libc::MNT_LOCAL as u32 == 0 {
            VolumeKind::Network
        } else {
            VolumeKind::Local
        }
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::{statfs, VolumeKind};
    use std::path::Path;

    /// Linux has no "is this local" flag, so the filesystem's own magic number
    /// is what answers. Typed as the field it is compared against rather than
    /// cast to a width of its own, so the comparison stays whatever `statfs`
    /// says it is. These are the network filesystems a music library is
    /// plausibly kept on; anything not listed reads as local, which costs a
    /// folder nothing but the cheaper check.
    const NETWORK_MAGICS: &[libc::__fsword_t] = &[
        0xFF53_4D42, // CIFS / SMB1
        0xFE53_4D42, // SMB2
        0x6969,      // NFS
        0x5346_4145, // AFS
        0x7461_636f, // OCFS2
        0x0187,      // AUTOFS — a mount point standing in for a remote one
        0x0173_4950, // 9P
        0x012F_F7B7, // Coda
        0x6573_5546, // FUSE — sshfs, rclone and the like mount shares this way;
                     // a local disk behind FUSE only gains the cheap check.
    ];

    pub(super) fn volume_kind(path: &Path) -> VolumeKind {
        let Some(stats) = statfs(path) else {
            return VolumeKind::Local;
        };
        if NETWORK_MAGICS.contains(&stats.f_type) {
            VolumeKind::Network
        } else {
            VolumeKind::Local
        }
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use super::VolumeKind;
    use std::path::Path;

    /// A UNC path names a server and a share, so it is network by spelling.
    /// A drive letter is whatever Windows says its drive is — a share mapped
    /// to a letter spells like a local disk, and is only told apart by asking.
    pub(super) fn volume_kind(path: &Path) -> VolumeKind {
        use std::os::windows::ffi::OsStrExt;
        use std::path::{Component, Prefix};

        let Some(Component::Prefix(prefix)) = path.components().next() else {
            return VolumeKind::Local;
        };
        let letter = match prefix.kind() {
            Prefix::UNC(..) | Prefix::VerbatimUNC(..) => return VolumeKind::Network,
            Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => letter,
            Prefix::Verbatim(_) | Prefix::DeviceNS(_) => return VolumeKind::Local,
        };
        let drive_root: Vec<u16> = std::ffi::OsStr::new(&format!("{}:\\", letter as char))
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        // SAFETY: `drive_root` is a NUL-terminated wide string that outlives
        // the call, which only reads it.
        let kind =
            unsafe { windows_sys::Win32::Storage::FileSystem::GetDriveTypeW(drive_root.as_ptr()) };
        if kind == windows_sys::Win32::System::WindowsProgramming::DRIVE_REMOTE {
            VolumeKind::Network
        } else {
            VolumeKind::Local
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
mod platform {
    use super::VolumeKind;
    use std::path::Path;

    pub(super) fn volume_kind(_path: &Path) -> VolumeKind {
        VolumeKind::Local
    }
}

#[cfg(test)]
#[path = "volume_tests.rs"]
mod tests;
