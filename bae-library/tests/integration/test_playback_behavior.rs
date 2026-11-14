//! Playback over imported releases: the shared helpers (`support`), the
//! fixtures the tests play (`fixtures`), and the tests, one module per area.

mod fixtures;
mod support;

mod cd_boundaries;
mod high_rate_and_restore;
mod local_sparse_buffer;
// Only Linux takes a name that isn't UTF-8.
#[cfg(target_os = "linux")]
mod non_utf8_names;
mod now_playing_edits;
mod open_files;
mod queue_and_pregap;
mod remote_sparse_buffer;
mod row_failure;
mod service_restart;
mod side_and_navigation;
mod side_pause_countdown;
