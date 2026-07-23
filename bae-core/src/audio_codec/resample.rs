//! A standalone FFmpeg audio resampler for interleaved `f32`.
//!
//! The encoder resamples inside its own pipeline; this exposes the same
//! `libswresample` conversion as a reusable unit for the playback side — the
//! AirPlay output resamples decoded audio to the fixed 44.1 kHz stereo a receiver
//! expects, since the decode pipeline fills the ring buffer at the track's native
//! rate. Input and output are interleaved single-precision float; channel count
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
// to the AirPlay send thread is sound.
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

    /// Convert one interleaved-`f32` input chunk, returning the interleaved output
    /// available so far (may be empty while the converter fills).
    pub fn convert(&mut self, input: &[f32]) -> Result<Vec<f32>, String> {
        // SAFETY: `swr` was built by `new` and is valid for the lifetime of self.
        unsafe { self.run(Some(input)) }
    }

    /// Drain the converter's buffered tail (no new input).
    pub fn flush(&mut self) -> Result<Vec<f32>, String> {
        // SAFETY: as `convert`.
        unsafe { self.run(None) }
    }

    unsafe fn run(&mut self, input: Option<&[f32]>) -> Result<Vec<f32>, String> {
        use ffmpeg_sys_next::*;

        let input_frames = input.map_or(0, |i| i.len() / self.src_channels) as c_int;
        let capacity = swr_get_out_samples(self.swr, input_frames);
        if capacity < 0 {
            return Err(format!(
                "Failed to size resampler output: {}",
                av_err_str(capacity)
            ));
        }

        let mut out = vec![0f32; capacity as usize * self.dst_channels];
        let out_ptr = out.as_mut_ptr() as *mut u8;
        let converted = match input {
            Some(samples) => {
                let in_ptr = samples.as_ptr() as *const u8;
                swr_convert(self.swr, &out_ptr, capacity, &in_ptr, input_frames)
            }
            None => swr_convert(self.swr, &out_ptr, capacity, ptr::null(), 0),
        };
        if converted < 0 {
            return Err(format!("Failed to resample: {}", av_err_str(converted)));
        }
        out.truncate(converted as usize * self.dst_channels);
        Ok(out)
    }
}

impl Drop for Resampler {
    fn drop(&mut self) {
        // SAFETY: `swr` was allocated by `new` and is freed exactly once here.
        unsafe { ffmpeg_sys_next::swr_free(&mut self.swr) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Resampling 48 kHz → 44.1 kHz preserves duration: a second of 48 kHz stereo
    /// comes out as ~44 100 frames (within the converter's small latency).
    #[test]
    fn downsamples_48k_to_44k_preserving_duration() {
        let mut r = Resampler::new(48_000, 2, 44_100, 2).unwrap();
        // One second of stereo silence: 48 000 frames = 96 000 interleaved samples.
        let input = vec![0.0f32; 48_000 * 2];
        let mut out = r.convert(&input).unwrap();
        out.extend(r.flush().unwrap());
        let out_frames = out.len() / 2;
        // Allow a few frames of resampler latency either way.
        assert!(
            (out_frames as i64 - 44_100).abs() < 100,
            "expected ~44100 frames, got {out_frames}"
        );
    }

    /// Mono upmixes to stereo: same rate preserves the frame count, and each
    /// frame's two channels carry the (matrixed) mono sample equally.
    #[test]
    fn upmixes_mono_to_stereo() {
        let mut r = Resampler::new(44_100, 1, 44_100, 2).unwrap();
        let input = vec![0.5f32; 100];
        let mut out = r.convert(&input).unwrap();
        out.extend(r.flush().unwrap());
        assert_eq!(out.len() / 2, 100, "frame count preserved at the same rate");
        // The upmix may apply a downmix gain, but the two channels are equal and
        // non-zero.
        for frame in out.chunks_exact(2) {
            assert!((frame[0] - frame[1]).abs() < 1e-4, "channels are equal");
            assert!(frame[0] > 0.0, "the mono signal reaches both channels");
        }
    }
}
