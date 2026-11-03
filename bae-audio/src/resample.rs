//! A standalone FFmpeg audio resampler for interleaved `f32`.
//!
//! The encoder resamples inside its own pipeline; this exposes the same
//! `libswresample` conversion as a reusable unit for the playback side: the
//! decode pipeline fills the ring buffer at the track's native rate, and an
//! output converts from there to the rate its device takes — the fixed 44.1 kHz
//! stereo an AirPlay receiver expects, or a rate a Mac's output device runs at
//! when it can't run at the track's. Input and output are interleaved single-precision float; channel count
//! and sample rate both convert.

use std::os::raw::c_int;
use std::ptr;

use super::av_err_str;

/// Resamples interleaved `f32` from one rate/channel-count to another, buffering
/// the converter's internal tail across calls.
pub struct Resampler {
    swr: *mut ffmpeg_sys_next::SwrContext,
    src_channels: usize,
    dst_channels: usize,
}

// The `SwrContext` is exclusively owned and never shared, so moving a `Resampler`
// to the thread that plays its output is sound.
unsafe impl Send for Resampler {}

impl Resampler {
    /// Build a resampler from `src` to `dst` format. Interleaved float both ways.
    pub fn new(
        src_rate: u32,
        src_channels: u32,
        dst_rate: u32,
        dst_channels: u32,
    ) -> Result<Self, String> {
        use ffmpeg_sys_next::*;
        unsafe {
            let mut src_layout: AVChannelLayout = std::mem::zeroed();
            av_channel_layout_default(&mut src_layout, src_channels as c_int);
            let mut dst_layout: AVChannelLayout = std::mem::zeroed();
            av_channel_layout_default(&mut dst_layout, dst_channels as c_int);

            let mut swr: *mut SwrContext = ptr::null_mut();
            let ret = swr_alloc_set_opts2(
                &mut swr,
                &dst_layout,
                AVSampleFormat::AV_SAMPLE_FMT_FLT,
                dst_rate as c_int,
                &src_layout,
                AVSampleFormat::AV_SAMPLE_FMT_FLT,
                src_rate as c_int,
                0,
                ptr::null_mut(),
            );
            if ret < 0 || swr.is_null() {
                return Err(format!("Failed to allocate resampler: {}", av_err_str(ret)));
            }
            let ret = swr_init(swr);
            if ret < 0 {
                swr_free(&mut swr);
                return Err(format!("Failed to init resampler: {}", av_err_str(ret)));
            }
            Ok(Resampler {
                swr,
                src_channels: src_channels.max(1) as usize,
                dst_channels: dst_channels.max(1) as usize,
            })
        }
    }

    /// Convert one interleaved-`f32` input chunk, appending the interleaved
    /// output available so far to `out` (possibly nothing while the converter
    /// fills). Appending lets a caller on an audio thread keep one buffer whose
    /// capacity settles, rather than allocating per chunk.
    pub fn convert(&mut self, input: &[f32], out: &mut Vec<f32>) -> Result<(), String> {
        // SAFETY: `swr` was built by `new` and is valid for the lifetime of self.
        unsafe { self.run(Some(input), out) }
    }

    /// Drain the converter's buffered tail (no new input), appending it to `out`.
    pub fn flush(&mut self, out: &mut Vec<f32>) -> Result<(), String> {
        // SAFETY: as `convert`.
        unsafe { self.run(None, out) }
    }

    unsafe fn run(&mut self, input: Option<&[f32]>, out: &mut Vec<f32>) -> Result<(), String> {
        use ffmpeg_sys_next::*;

        let input_frames = input.map_or(0, |i| i.len() / self.src_channels) as c_int;
        let capacity = swr_get_out_samples(self.swr, input_frames);
        if capacity < 0 {
            return Err(format!("Failed to size resampler output: {}", av_err_str(capacity)));
        }

        let start = out.len();
        out.resize(start + capacity as usize * self.dst_channels, 0.0);
        let out_ptr = out[start..].as_mut_ptr() as *mut u8;
        let converted = match input {
            Some(samples) => {
                let in_ptr = samples.as_ptr() as *const u8;
                swr_convert(self.swr, &out_ptr, capacity, &in_ptr, input_frames)
            }
            None => swr_convert(self.swr, &out_ptr, capacity, ptr::null(), 0),
        };
        if converted < 0 {
            out.truncate(start);
            return Err(format!("Failed to resample: {}", av_err_str(converted)));
        }
        out.truncate(start + converted as usize * self.dst_channels);
        Ok(())
    }
}

impl Drop for Resampler {
    fn drop(&mut self) {
        // SAFETY: `swr` was allocated by `new` and is freed exactly once here.
        unsafe { ffmpeg_sys_next::swr_free(&mut self.swr) };
    }
}

#[cfg(test)]
#[path = "resample_tests.rs"]
mod tests;
