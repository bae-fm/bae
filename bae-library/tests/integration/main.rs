//! bae-library's integration tests, one module per area, built as one test
//! binary: one link and one copy on disk instead of one per area.
//!
//! The modules share one process. A test that changes something process-wide
//! or can take the process down runs its body in a process of its own
//! (`own_process`). The playback CPU test is a binary of its own
//! (`tests/test_playback_cpu.rs`): it reads the process's CPU time and runs in
//! the release profile.
//!
//! Run one area with a filter on its module: `scripts/cargo.sh test -p
//! bae-library --features test-utils --test integration -- test_people::`.

mod own_process;

mod alac_fixtures;
mod dropbox_sign_in;
mod test_admin_changes;
mod test_album_sort;
mod test_candidate_text_fixtures;
mod test_collections;
mod test_coven_contract;
mod test_cue_ape;
mod test_cue_flac;
mod test_delete;
mod test_export;
mod test_gap_appended_rip;
mod test_history;
mod test_image_track_audio;
mod test_import_open_files;
mod test_import_service;
mod test_imported_release;
mod test_joined_library;
mod test_library_create_cost;
mod test_people;
mod test_playback_behavior;
mod test_profile_request;
mod test_release_edit_validation;
mod test_restore_cost;
mod test_restore_resume;
mod test_save_s3_config;
mod test_scrub_points;
mod test_sql_read_tripwire;
mod test_storage;
mod test_sync_identity;
