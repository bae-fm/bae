use super::*;
use crate::cue_flac::CueTrackMode;

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
        file: ScannedFile::new(PathBuf::from(path), relative_path.to_string(), size),
        role: FileRole::Audio,
        proposed_audio: true,
    }
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
    let watched_folder =
        crate::import::WatchedFolder::from_path(root.to_string_lossy().into_owned());
    // Stored the way the scan tables store them: each item written in turn,
    // deleting what it supersedes, keyed by path.
    let mut stored: std::collections::BTreeMap<String, ScanItem> = Default::default();
    scan_for_candidates_with_decisions(root, &StoredCandidateEdits::none(), &decisions, |item| {
        if matches!(item, ScanItem::Discovered(_) | ScanItem::Decided { .. }) {
            return;
        }
        let existing: Vec<_> = stored
            .iter()
            .map(|(key, item)| crate::import::candidates::StoredEntryKey {
                key: key.clone(),
                covers_whole_folder: match item {
                    ScanItem::Discovered(candidate) | ScanItem::Valid(candidate) => {
                        candidate.scope == ReleaseFileScope::Recursive
                    }
                    ScanItem::Invalid(_) | ScanItem::Decided { .. } => false,
                },
            })
            .collect();
        for key in crate::import::candidates::superseded_entry_keys(&existing, &item) {
            stored.remove(&key);
        }
        stored.insert(item.persisted_key().expect("a scan entry has a key"), item);
    })
    .unwrap();
    let _ = &watched_folder;
    // The order the import tab shows them in: valid releases first in natural
    // path order, then the folders that failed validation.
    let mut valid = Vec::new();
    let mut invalid = Vec::new();
    for item in stored.into_values() {
        match item {
            ScanItem::Discovered(candidate) | ScanItem::Valid(candidate) => valid.push(candidate),
            ScanItem::Invalid(candidate) => invalid.push(candidate),
            ScanItem::Decided { .. } => {}
        }
    }
    valid.sort_by(|left, right| {
        natord::compare_ignore_case(&left.display_path, &right.display_path)
    });
    invalid.sort_by(|left, right| {
        natord::compare_ignore_case(&left.display_path, &right.display_path)
    });
    valid
        .into_iter()
        .map(ScanItem::Valid)
        .chain(invalid.into_iter().map(ScanItem::Invalid))
        .collect()
}

/// Only the valid `FolderCandidate`s for `root` — the shape most scanner
/// tests assert against (counts, paths, categorized files).
fn scan_valid(root: impl Into<PathBuf>) -> Vec<FolderCandidate> {
    scan_items(root)
        .into_iter()
        .filter_map(|item| match item {
            ScanItem::Valid(c) => Some(c),
            ScanItem::Invalid(_) => None,
            ScanItem::Discovered(_) | ScanItem::Decided { .. } => None,
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
include!("tests/cue_and_file_validation.rs");
include!("tests/scenario_fixtures.rs");
include!("tests/scan_scenarios.rs");
include!("tests/bindings.rs");
