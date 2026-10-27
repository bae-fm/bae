use super::*;
use crate::cue_flac::CueTrackMode;
use std::sync::Arc;

/// Valid FLAC fixture bytes.
fn fake_flac() -> Vec<u8> {
    std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/flac/01 Test Track 1.flac"
    ))
    .expect("read FLAC fixture")
}

/// One audio-role entry, for the hand-built `CategorizedFiles` in the
/// content-hash tests.
fn audio_entry(path: &str, relative_path: &str, size: u64) -> CandidateFile {
    CandidateFile {
        file: ScannedFile::new(PathBuf::from(path), relative_path.to_string(), size, 1)
            .with_test_flac_audio(),
        role: FileRole::Audio,
    }
}

fn assert_uniform_source_audio(
    files: &CategorizedFiles,
    layout: crate::album_detail::SourceAudioLayout,
    codec: &str,
) {
    let summary = files
        .source_audio_summary()
        .expect("candidate has source audio");
    match summary {
        crate::album_detail::SourceAudioSummary::Uniform { descriptor } => {
            assert_eq!(descriptor.layout, layout);
            assert_eq!(descriptor.format.codec, codec);
        }
        crate::album_detail::SourceAudioSummary::Mixed { differences } => {
            panic!("expected uniform source audio, got {differences:?}")
        }
    }
}

#[test]
fn source_audio_keeps_every_physical_file_in_release_order() {
    let files = CategorizedFiles {
        files: vec![
            audio_entry("/music/02.flac", "02.flac", 20),
            audio_entry("/music/03.flac", "03.flac", 30),
        ],
        parts: Vec::new(),
    };

    let source_audio = files.source_audio().expect("candidate has source audio");

    assert_eq!(
        source_audio
            .files
            .iter()
            .map(|file| file.relative_path.as_str())
            .collect::<Vec<_>>(),
        ["02.flac", "03.flac"]
    );
    assert!(matches!(
        source_audio.summary,
        crate::album_detail::SourceAudioSummary::Uniform { .. }
    ));
}

/// The final projected scan items for `root`. The callback is an update stream:
/// a later item with the same key can add a proven combine action or replace
/// provisional candidates with an unresolved boundary.
fn scan_items(root: impl Into<PathBuf>) -> Vec<ScanItem> {
    scan_projected_items_with_decisions(root.into(), FolderReleaseDecisions::default())
}

fn scan_projected_items_with_decisions(
    root: PathBuf,
    decisions: FolderReleaseDecisions,
) -> Vec<ScanItem> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(stored_scan(root, decisions))
}

/// What the scan tables hold after one scan of `root` under `decisions`: the
/// readings stored first, as a library holds them before its scan, then each
/// item the walk reports written in turn as a scan writes it, and the scan
/// finished.
async fn stored_scan(root: PathBuf, decisions: FolderReleaseDecisions) -> Vec<ScanItem> {
    let store = tempfile::TempDir::new().unwrap();
    let db = crate::db::Database::new_test(
        store.path().join("test.db").to_str().unwrap(),
        Arc::new(coven::SystemClock),
    )
    .await
    .unwrap();
    let root_key = root.to_string_lossy().into_owned();
    db.add_watched_import_folder(&root_key).await.unwrap();
    for (folder, reading) in decisions.iter() {
        db.store_folder_reading(
            &FolderReleaseDecisionKey {
                watched_folder_path: root_key.clone(),
                relative_folder_path: folder.clone(),
            },
            reading,
        )
        .await
        .unwrap();
    }
    let decisions = db.load_folder_release_decisions(&root_key).await.unwrap();
    let mut walked = Vec::new();
    scan_for_candidates_with_decisions(root, &StoredCandidateEdits::none(), &decisions, |item| {
        walked.push(item)
    })
    .unwrap();
    let generation = db
        .begin_folder_scan(&root_key, crate::import::VolumeKind::Local)
        .await
        .unwrap();
    for item in walked {
        match item {
            ScanItem::Decided {
                key,
                decision,
                grouping,
            } => db
                .record_scanned_folder_release_decision(&key, decision, &grouping)
                .await
                .unwrap(),
            item => {
                db.save_folder_scan_item(&root_key, generation, &item)
                    .await
                    .unwrap()
                    .expect("the scan generation is current");
            }
        }
    }
    db.finish_folder_scan(&root_key, generation, None)
        .await
        .unwrap()
        .expect("the scan generation is current");
    // The order the import tab shows them in: valid releases first in natural
    // path order, then the folders that failed validation.
    let mut valid = Vec::new();
    let mut invalid = Vec::new();
    for item in db.load_folder_scan_items(&root_key).await.unwrap() {
        match item {
            ScanItem::Invalid(_) => invalid.push(item),
            ScanItem::Discovered(_) | ScanItem::Valid(_) => valid.push(item),
            ScanItem::Decided { .. } | ScanItem::Sidecar(_) => {
                unreachable!("the scan tables load only releases: {item:?}")
            }
        }
    }
    let shown = |item: &ScanItem| {
        item.display_path()
            .expect("a release shows a path")
            .to_string()
    };
    valid.sort_by(|left, right| natord::compare_ignore_case(&shown(left), &shown(right)));
    invalid.sort_by(|left, right| natord::compare_ignore_case(&shown(left), &shown(right)));
    valid.into_iter().chain(invalid).collect()
}

/// How each folder named reads, under a grouping key of its own.
fn readings(
    readings: &[(&str, FolderReleaseDecision, FolderReleaseDecisionAuthor)],
) -> FolderReleaseDecisions {
    FolderReleaseDecisions::new(
        readings
            .iter()
            .map(|(folder, decision, author)| {
                (
                    folder.to_string(),
                    FolderReading {
                        decision: *decision,
                        author: *author,
                        grouping: format!("grouping:{folder}"),
                    },
                )
            })
            .collect(),
    )
}

/// Everything `root` holds, read as one release, with nothing stored about it.
fn scan_files(root: impl AsRef<Path>) -> CategorizedFiles {
    collect_release_candidate_files_with_scope(
        root.as_ref(),
        ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan")
}

/// A temp folder holding an empty `Album` directory. The `TempDir` comes back
/// with it because dropping it deletes the folder.
fn album_dir() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(&album).unwrap();
    (tmp, album)
}

/// Only the valid `FolderCandidate`s for `root` — the shape most scanner
/// tests assert against (counts, paths, categorized files).
fn scan_valid(root: impl Into<PathBuf>) -> Vec<FolderCandidate> {
    scan_items(root)
        .into_iter()
        .filter_map(|item| match item {
            ScanItem::Valid(c) => Some(c),
            ScanItem::Invalid(_) => None,
            ScanItem::Discovered(_) | ScanItem::Decided { .. } | ScanItem::Sidecar(_) => None,
        })
        .collect()
}

fn scan_for_candidates_with_decisions_collect(
    root: PathBuf,
    decisions: FolderReleaseDecisions,
) -> Vec<ScanItem> {
    let mut items = Vec::new();
    scan_for_candidates_with_decisions(root, &StoredCandidateEdits::none(), &decisions, |item| {
        if !matches!(item, ScanItem::Discovered(_)) {
            items.push(item);
        }
    })
    .unwrap();
    items
}

include!("tests/boundaries.rs");
include!("tests/folder_parts.rs");
include!("tests/cue_and_file_validation.rs");
include!("tests/scenario_fixtures.rs");
include!("tests/scan_scenarios.rs");
include!("tests/bindings.rs");
include!("tests/audio_formats.rs");
include!("tests/sidecars.rs");
include!("tests/track_order.rs");
include!("tests/file_reads.rs");
include!("tests/folders_at_once.rs");
