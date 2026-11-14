//! The clock a screen's frames are timed on, read in nanoseconds.
//!
//! Core stamps a position it hands a screen with this clock, and the screen's
//! native view works out where the position has got to from the time its
//! display frame reports on the same clock: macOS's `CADisplayLink` timestamps
//! (`CACurrentMediaTime`, `CLOCK_UPTIME_RAW`), Android's `Choreographer` frame
//! time (`CLOCK_MONOTONIC`) and Windows' `QueryPerformanceCounter`. It only
//! moves forward and ignores changes to the wall clock.

/// Now, on the frame clock, in nanoseconds.
pub fn frame_clock_ns() -> u64 {
    now()
}

/// The clock the platform's frames are timed on: `CLOCK_UPTIME_RAW` on
/// Apple platforms, `CLOCK_MONOTONIC` elsewhere.
#[cfg(target_vendor = "apple")]
const CLOCK: libc::clockid_t = libc::CLOCK_UPTIME_RAW;
#[cfg(all(unix, not(target_vendor = "apple")))]
const CLOCK: libc::clockid_t = libc::CLOCK_MONOTONIC;

#[cfg(unix)]
fn now() -> u64 {
    let mut time = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: clock_gettime writes the time into the struct we pass.
    let read = unsafe { libc::clock_gettime(CLOCK, &mut time) };
    assert_eq!(read, 0, "the frame clock is always readable");
    u64::try_from(time.tv_sec).expect("the frame clock is not negative") * 1_000_000_000
        + u64::try_from(time.tv_nsec).expect("nanoseconds are not negative")
}

#[cfg(windows)]
fn now() -> u64 {
    use windows_sys::Win32::System::Performance::{
        QueryPerformanceCounter, QueryPerformanceFrequency,
    };
    let (mut ticks, mut per_second) = (0_i64, 0_i64);
    // SAFETY: each call writes one i64 into the variable we pass.
    let read = unsafe {
        QueryPerformanceFrequency(&mut per_second) != 0 && QueryPerformanceCounter(&mut ticks) != 0
    };
    assert!(read, "the performance counter is always readable");
    let (ticks, per_second) =
        (u128::try_from(ticks).expect("ticks"), u128::try_from(per_second).expect("frequency"));
    u64::try_from(ticks * 1_000_000_000 / per_second).expect("nanoseconds fit 64 bits")
}

#[cfg(test)]
#[path = "frame_clock_tests.rs"]
mod tests;
