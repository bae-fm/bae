//! Where a stream's timestamps put its samples, shared by the decodes and the
//! landing search so both count samples the same way.

use std::os::raw::c_int;

/// Where a stream's timestamps put its samples. Sample 0 is the first sample
/// a decode from the stream's start puts out: the stream's `start_time`, which
/// for a codec with an encoder delay (an MP3's LAME delay) lies past the first
/// packet's timestamp, the delay being samples the decoder drops. A target
/// sample and every frame are placed on this one count, so a decode that
/// starts at sample N puts out what a decode from the start puts out from N
/// on.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SampleClock {
    time_base: ffmpeg_sys_next::AVRational,
    sample_rate: u32,
    /// The timestamp of sample 0.
    origin: i64,
}

impl SampleClock {
    pub(crate) fn new(
        time_base: ffmpeg_sys_next::AVRational,
        sample_rate: u32,
        origin: i64,
    ) -> Self {
        Self {
            time_base,
            sample_rate,
            origin,
        }
    }

    /// The clock of `stream`, decoded at `sample_rate`.
    ///
    /// # Safety
    /// `stream` must be a valid, probed stream.
    pub(crate) unsafe fn of_stream(
        stream: *const ffmpeg_sys_next::AVStream,
        sample_rate: u32,
    ) -> Self {
        let start_time = (*stream).start_time;
        let origin = if start_time == ffmpeg_sys_next::AV_NOPTS_VALUE {
            0
        } else {
            start_time
        };
        Self::new((*stream).time_base, sample_rate, origin)
    }

    fn samples(&self) -> ffmpeg_sys_next::AVRational {
        ffmpeg_sys_next::AVRational {
            num: 1,
            den: self.sample_rate as c_int,
        }
    }

    /// The sample a timestamp falls on.
    pub(crate) fn sample_at(&self, timestamp: i64) -> i64 {
        unsafe {
            ffmpeg_sys_next::av_rescale_q(timestamp - self.origin, self.time_base, self.samples())
        }
    }

    /// The timestamp of `sample`, which lies before the stream's first when
    /// negative.
    pub(crate) fn timestamp_of(&self, sample: i64) -> i64 {
        self.origin
            + unsafe { ffmpeg_sys_next::av_rescale_q(sample, self.samples(), self.time_base) }
    }
}
