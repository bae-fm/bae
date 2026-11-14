//! The import tables' tests, over a library store of their own.

use crate::folder_scanner::{
    CandidateFile, CategorizedFiles, FileRole, FolderCandidate, ReleaseFileScope, ScannedFile,
};
use bae_store::test_fixtures::empty_db;
use bae_store::Store;
use std::path::PathBuf;

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
        relative_folder: crate::RelativeFolder::test(name),
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

/// The release a scan reads at `key`'s folder below the watched folder
/// `root`: one audio file, named for the folder so that each folder's files
/// hash apart.
pub(crate) fn key_candidate(root: &std::path::Path, key: &crate::CandidateKey) -> FolderCandidate {
    let folder = key.test_folder();
    let relative = folder.strip_prefix(root).expect("the key's folder is below the root");
    let name = relative.to_str().expect("a test's folder is UTF-8").replace('\\', "/");
    let file_name = format!("{}.flac", hex::encode(coven::path_bytes(relative)));
    candidate_with(
        root,
        &name,
        CategorizedFiles {
            files: vec![CandidateFile {
                file: ScannedFile::new(folder.join(&file_name), file_name, 1_000, 1)
                    .with_test_flac_audio(),
                role: FileRole::Audio,
            }],
            parts: Vec::new(),
        },
        ReleaseFileScope::Recursive,
    )
}

/// `key`'s release scanned under the watched folder `root`, in a pass that
/// reads again every release the last one stored: the files it is
/// identified as.
pub(crate) async fn scan_key(
    tables: &crate::tables::ImportTables,
    root: &std::path::Path,
    key: &crate::CandidateKey,
) -> crate::CandidateFiles {
    let candidate = key_candidate(root, key);
    tables.add_watched_import_folder(root).await.unwrap();
    let kept: Vec<_> = tables
        .load_folder_scan_items(root)
        .await
        .unwrap()
        .into_iter()
        .filter(|item| item.key() != *key)
        .collect();
    let generation = tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();
    for item in kept.into_iter().chain([crate::folder_scanner::ScanItem::Valid(candidate.clone())])
    {
        tables.save_folder_scan_item(root, generation, &item).await.unwrap();
    }
    tables.finish_folder_scan(root, generation, None).await.unwrap();
    crate::CandidateFiles { content_hash: candidate.files.content_hash(), file_edit_revision: 0 }
}

/// The message of the one bug `heard` was reported, which `failed` carries
/// back from the panic that failed its write.
pub(crate) fn the_reported_bug(
    heard: &coven::task::HeardBugs,
    failed: &impl coven::task::CarriesBug,
) -> String {
    let reported = heard.take();
    assert_eq!(reported.len(), 1, "{reported:?}");
    assert_eq!(failed.bug(), Some(&reported[0]));
    reported[0].message.clone()
}

/// `empty_db` plus a watched-folder root that exists on disk.
pub(crate) async fn watched_root() -> (Store, tempfile::TempDir, std::path::PathBuf) {
    let (db, tmp) = empty_db(crate::TEST_SCHEMA).await;
    let root = tmp.path().join("watched");
    std::fs::create_dir_all(&root).unwrap();
    (db, tmp, root)
}

/// A folder of plain track files, each `(relative_path, size)`.
pub(crate) fn track_files_candidate(files: &[(&str, u64)]) -> CategorizedFiles {
    CategorizedFiles {
        files: files
            .iter()
            .map(|(name, size)| CandidateFile {
                file: ScannedFile::new(PathBuf::from(*name), name.to_string(), *size, 1)
                    .with_test_flac_audio(),
                role: FileRole::Audio,
            })
            .collect(),
        parts: Vec::new(),
    }
}

pub(crate) fn scanned_candidate(
    root: &std::path::Path,
    name: &str,
) -> crate::folder_scanner::ScanItem {
    scanned_candidate_with_scope(root, name, crate::folder_scanner::ReleaseFileScope::Direct)
}

pub(crate) fn scanned_candidate_with_scope(
    root: &std::path::Path,
    name: &str,
    scope: crate::folder_scanner::ReleaseFileScope,
) -> crate::folder_scanner::ScanItem {
    crate::folder_scanner::ScanItem::Valid(candidate_with(
        root,
        name,
        track_files_candidate(&[("01.flac", 123)]),
        scope,
    ))
}

pub(crate) fn scan_candidate(
    root: &std::path::Path,
    name: &str,
) -> crate::folder_scanner::ScanItem {
    crate::folder_scanner::ScanItem::Valid(candidate(root, name))
}
