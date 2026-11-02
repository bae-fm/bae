//! The import tables' tests, over a library store of their own.

use crate::db::client::tests::{
    empty_db, exec, exec_batch, fixed_now, temp_db, write_cover, ALBUM_A, ARTIST_A, ARTIST_B,
    RELEASE_A,
};
use crate::db::Database;
use crate::import::folder_scanner::{
    CandidateFile, CategorizedFiles, FileRole, FolderCandidate, ReleaseFileScope, ScannedFile,
};

/// The folder at `name`, a `/`-separated path below `root`, spelled the way
/// the scanner spells it on this host.
pub(crate) fn folder_below(root: &std::path::Path, name: &str) -> std::path::PathBuf {
    name.split('/').fold(std::path::PathBuf::from(root), |folder, part| folder.join(part))
}

/// A scanned folder at `name` below watched root `root` holding `files`, as the
/// scanner produces it for a folder whose file decisions nobody has edited.
pub(crate) fn candidate_with(
    root: &std::path::Path,
    name: &str,
    files: CategorizedFiles,
    scope: ReleaseFileScope,
) -> FolderCandidate {
    let path = folder_below(root, name);
    FolderCandidate {
        path: path.clone(),
        file_root: path,
        name: name.to_string(),
        files,
        watched_folder_path: std::path::PathBuf::from(root),
        scope,
        file_edit_revision: 0,
        relative_folder: crate::import::RelativeFolder::test(name),
        grouping: None,
    }
}

/// `candidate_with` for the folder most import fixtures want: one bound FLAC,
/// scanned recursively.
pub(crate) fn candidate(root: &std::path::Path, name: &str) -> FolderCandidate {
    candidate_with(
        root,
        name,
        CategorizedFiles {
            files: vec![CandidateFile {
                file: ScannedFile::new(
                    folder_below(root, name).join("01.flac"),
                    "01.flac".to_string(),
                    1_000,
                    1,
                )
                .with_test_flac_audio(),
                role: FileRole::Audio,
            }],
            parts: Vec::new(),
        },
        ReleaseFileScope::Recursive,
    )
}

/// `empty_db` plus a watched-folder root that exists on disk.
pub(crate) async fn watched_root() -> (Database, tempfile::TempDir, std::path::PathBuf) {
    let (db, tmp) = empty_db().await;
    let root = tmp.path().join("watched");
    std::fs::create_dir_all(&root).unwrap();
    (db, tmp, root)
}

mod candidate_standing_tests;
mod draftless_candidate_tests;
mod file_tag_snapshot_tests;
mod folder_scan_live_query_tests;
mod import_candidate_state_tests;
mod import_list_live_query_tests;
mod import_list_placement_tests;
mod import_list_tests;
mod release_grouping_tests;
mod watched_folder_tests;
