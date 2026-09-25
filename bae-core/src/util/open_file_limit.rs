//! The process's soft limit on open file descriptors.
//!
//! A macOS GUI app starts under launchd's soft limit of 256. A library app
//! holds the store and its WAL, a pool of SQLite connections, sockets for sync
//! and discovery, folder watchers, and the files it is reading, so bae raises
//! the soft limit to the most the system allows it at startup, as media apps
//! do. The hard limit is left as it is.

use tracing::{info, warn};

/// Raise the soft `RLIMIT_NOFILE` to the hard limit (on Apple platforms, to
/// the per-process ceiling the kernel enforces below an unlimited hard limit),
/// logging the limit before and after. A limit that cannot be read or raised
/// is logged and left as it was; the process runs under it.
pub(crate) fn raise_open_file_limit() {
    let mut limit = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: getrlimit writes the limit into the struct we pass.
    if unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) } != 0 {
        warn!(
            error = %std::io::Error::last_os_error(),
            "could not read the open-file limit"
        );
        return;
    }
    let ceiling = match soft_ceiling(limit.rlim_max) {
        Ok(ceiling) => ceiling,
        Err(error) => {
            warn!(
                soft = limit.rlim_cur,
                hard = limit.rlim_max,
                %error,
                "could not read the per-process open-file ceiling; the soft limit stays"
            );
            return;
        }
    };
    if limit.rlim_cur >= ceiling {
        info!(
            soft = limit.rlim_cur,
            hard = limit.rlim_max,
            "open-file soft limit is already at its ceiling"
        );
        return;
    }
    let raised = libc::rlimit {
        rlim_cur: ceiling,
        rlim_max: limit.rlim_max,
    };
    // SAFETY: setrlimit reads the struct we pass.
    if unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &raised) } != 0 {
        warn!(
            soft = limit.rlim_cur,
            requested = ceiling,
            hard = limit.rlim_max,
            error = %std::io::Error::last_os_error(),
            "could not raise the open-file soft limit"
        );
        return;
    }
    info!(
        before = limit.rlim_cur,
        after = ceiling,
        hard = limit.rlim_max,
        "raised the open-file soft limit"
    );
}

/// The highest soft limit the kernel accepts under `hard`. On Apple platforms
/// that is also capped by the per-process descriptor ceiling: `setrlimit`
/// refuses a soft limit above it even when the hard limit is unlimited.
#[cfg(any(target_os = "macos", target_os = "ios"))]
fn soft_ceiling(hard: libc::rlim_t) -> std::io::Result<libc::rlim_t> {
    let mut per_process: libc::c_int = 0;
    let mut size = std::mem::size_of::<libc::c_int>();
    // SAFETY: the name is NUL-terminated and `per_process` is `size` bytes.
    let status = unsafe {
        libc::sysctlbyname(
            c"kern.maxfilesperproc".as_ptr(),
            (&mut per_process as *mut libc::c_int).cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if status != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(hard.min(per_process as libc::rlim_t))
}

/// Elsewhere the hard limit is the ceiling.
#[cfg(not(any(target_os = "macos", target_os = "ios")))]
fn soft_ceiling(hard: libc::rlim_t) -> std::io::Result<libc::rlim_t> {
    Ok(hard)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn current_limit() -> libc::rlimit {
        let mut limit = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        // SAFETY: getrlimit writes the limit into the struct we pass.
        assert_eq!(
            unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) },
            0
        );
        limit
    }

    /// Starting below its ceiling, the soft limit is raised to exactly the
    /// ceiling. The test process's limit is lowered to a value every other
    /// test in it still fits under, then raised again.
    #[test]
    fn a_soft_limit_below_its_ceiling_is_raised_to_it() {
        let hard = current_limit().rlim_max;
        let ceiling = soft_ceiling(hard).expect("soft ceiling");
        let lowered = libc::rlimit {
            rlim_cur: ceiling.min(4096) - 1,
            rlim_max: hard,
        };
        // SAFETY: setrlimit reads the struct we pass.
        assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &lowered) }, 0);

        raise_open_file_limit();

        assert_eq!(current_limit().rlim_cur, ceiling);
    }
}
