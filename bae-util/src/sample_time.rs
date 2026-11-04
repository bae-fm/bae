//! Audio frames counted at a sample rate, as time and back: one rule for
//! every decode, buffer and stream, so a time taken from a frame count lands
//! on that frame again.

use std::time::Duration;

/// When frame `frame` starts at `sample_rate` frames a second, rounded up to
/// the next nanosecond: the time lies within that frame, so [`frame_at`]
/// gives `frame` back.
///
/// # Panics
/// On a zero sample rate, which no stream has.
pub fn frame_start(frame: u64, sample_rate: u32) -> Duration {
    let rate = u64::from(rate_of(sample_rate));
    let within = u128::from(frame % rate) * 1_000_000_000;
    let nanos = within.div_ceil(u128::from(rate));
    Duration::new(
        frame / rate,
        u32::try_from(nanos).expect("a fraction of a second is under 10^9 ns"),
    )
}

/// The frame `time` falls on at `sample_rate` frames a second: the count of
/// frames that start before it.
///
/// # Panics
/// On a zero sample rate, which no stream has, and on a time whose frame
/// count passes `u64`.
pub fn frame_at(time: Duration, sample_rate: u32) -> u64 {
    let frames = time.as_nanos() * u128::from(rate_of(sample_rate)) / 1_000_000_000;
    u64::try_from(frames).expect("a time's frame count fits u64")
}

fn rate_of(sample_rate: u32) -> u32 {
    assert_ne!(sample_rate, 0, "a stream's sample rate is never zero");
    sample_rate
}

#[cfg(test)]
#[path = "sample_time_tests.rs"]
mod tests;
