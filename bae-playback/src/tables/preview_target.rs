//! A preview target as playback's rows store it: the file's path in
//! `preview_path` and the stretch of it in `preview_start_sample` and
//! `preview_end_sample`.

use bae_store::path_column;
use bae_track_audio::PreviewTarget;
use rusqlite::Row;

/// `target`'s values for `preview_path`, `preview_start_sample` and
/// `preview_end_sample`, in that order.
pub(super) fn preview_target_values(target: &PreviewTarget) -> (Vec<u8>, u64, Option<u64>) {
    let (path, start_sample, end_sample) = target.clone().into_sample_range();
    (coven::path_bytes(&path), start_sample, end_sample)
}

/// The preview target `row` stores, `None` when its `preview_path` is NULL.
pub(super) fn preview_target_from_row(row: &Row<'_>) -> rusqlite::Result<Option<PreviewTarget>> {
    let Some(path) = path_column::get_optional(row, "preview_path")? else {
        return Ok(None);
    };
    Ok(Some(PreviewTarget::sample_range(
        path,
        row.get("preview_start_sample")?,
        row.get("preview_end_sample")?,
    )))
}
