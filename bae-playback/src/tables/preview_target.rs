//! A preview target as playback's rows store it: the file's path in
//! `preview_path` and the stretch of it in `preview_start_sample` and
//! `preview_end_sample`.

use bae_track_audio::PreviewTarget;

/// `target`'s values for `preview_path`, `preview_start_sample` and
/// `preview_end_sample`, in that order.
pub(super) fn preview_target_values(target: &PreviewTarget) -> (Vec<u8>, u64, Option<u64>) {
    let (path, start_sample, end_sample) = target.clone().into_sample_range();
    (coven::path_bytes(&path), start_sample, end_sample)
}
