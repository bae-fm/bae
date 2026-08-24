//! The flatten's rules, over row literals.
//!
//! Everything the tab shows is decided here — which rows exist, in what order,
//! under which header, in which tab, and what the chrome around them says — so
//! the fixtures are the columns the read gathers rather than a database.

use super::*;
use crate::db::{
    CandidateStateListRow, ImportQueueRows, ScanBoundaryListRow, ScanCandidateKind,
    ScanCandidateListRow,
};
use crate::identify::{LeadMatch, VerdictKind, VerdictSummary};
use crate::import::folder_registry::host_root;
use crate::import::folder_scanner::InvalidReason;
use crate::import::search::SourceTracks;
use crate::import::types::MetadataSource;
use crate::import::{FolderScanStatus, ImportedRelease};
use crate::import::{IdentifyPhase, TriageImportStatus, TriagePlacement};

mod flatten;
mod subscription;
mod window;

fn root() -> String {
    host_root("/music")
}

fn queue() -> ImportQueueRows {
    ImportQueueRows {
        watched_folders: vec![WatchedFolder {
            path: root(),
            name: "music".to_string(),
        }],
        folder_scan_statuses: vec![WatchedFolderScanStatus {
            watched_folder_path: root(),
            watched_folder_name: "music".to_string(),
            status: FolderScanStatus::Complete,
            on_network_volume: false,
        }],
        ..ImportQueueRows::default()
    }
}

/// One settled candidate at `display_path`, with its own content hash so the
/// stored rows below can be keyed per candidate.
fn candidate(display_path: &str) -> ScanCandidateListRow {
    ScanCandidateListRow {
        watched_folder_path: root(),
        path: format!("{}/{display_path}", root()),
        kind: ScanCandidateKind::Valid,
        name: display_path
            .rsplit('/')
            .next()
            .expect("a display path has a last component")
            .to_string(),
        display_path: display_path.to_string(),
        content_hash: Some(format!("hash-{display_path}")),
        file_edit_revision: 0,
        combine_ancestor_relative_path: None,
        invalid_reason: None,
    }
}

fn tentative(display_path: &str) -> ScanCandidateListRow {
    ScanCandidateListRow {
        kind: ScanCandidateKind::Tentative,
        ..candidate(display_path)
    }
}

fn invalid(display_path: &str) -> ScanCandidateListRow {
    ScanCandidateListRow {
        kind: ScanCandidateKind::Invalid,
        content_hash: None,
        invalid_reason: Some(InvalidReason::NoValidAudio),
        ..candidate(display_path)
    }
}

fn boundary(display_path: &str, tree_row_display_paths: Vec<String>) -> ScanBoundaryListRow {
    ScanBoundaryListRow {
        watched_folder_path: root(),
        relative_folder_path: display_path.to_string(),
        name: display_path
            .rsplit('/')
            .next()
            .expect("a display path has a last component")
            .to_string(),
        display_path: display_path.to_string(),
        tree_row_display_paths,
    }
}

fn key(display_path: &str) -> String {
    format!("{}/{display_path}", root())
}

fn lead(release_id: &str) -> LeadMatch {
    LeadMatch {
        release_id: release_id.to_string(),
        source: MetadataSource::MusicBrainz,
        source_group_id: Some("group-1".to_string()),
        title: "Album Title".to_string(),
        artist: Some("Artist Name".to_string()),
        year: Some(1999),
        format: Some("CD".to_string()),
        cover_thumbnail_url: Some("https://example.test/thumb.jpg".to_string()),
        source_tracks: Some(SourceTracks::Listed {
            count: 11,
            total_duration_ms: Some(2_400_000),
        }),
        by_disc_id: true,
        by_barcode: false,
    }
}

/// A stored verdict that classifies Ready: one match, counts and lengths
/// agreeing, and identification's own pick of that match.
fn ready_state(release_id: &str) -> CandidateStateListRow {
    CandidateStateListRow {
        edit_revision: 0,
        verdict: Some(VerdictSummary {
            kind: VerdictKind::Found,
            track_count: Some(11),
            match_count: 1,
            lead: Some(lead(release_id)),
        }),
        probed_total_duration_ms: 2_400_000,
        pick: Some(IdentityPick::Release {
            source: MetadataSource::MusicBrainz,
            release_id: release_id.to_string(),
        }),
    }
}

/// A stored verdict that classifies Needs you: several pressings matched.
fn several_matches_state() -> CandidateStateListRow {
    CandidateStateListRow {
        edit_revision: 0,
        verdict: Some(VerdictSummary {
            kind: VerdictKind::Found,
            track_count: Some(11),
            match_count: 3,
            lead: Some(lead("mb-1")),
        }),
        probed_total_duration_ms: 2_400_000,
        pick: None,
    }
}

/// A stored verdict that classifies Needs you: nothing matched anywhere.
fn not_found_state() -> CandidateStateListRow {
    CandidateStateListRow {
        edit_revision: 0,
        verdict: Some(VerdictSummary {
            kind: VerdictKind::NotFound,
            track_count: None,
            match_count: 0,
            lead: None,
        }),
        probed_total_duration_ms: 2_400_000,
        pick: None,
    }
}

/// The pick a user makes on a release row.
fn release_pick(release_id: &str) -> IdentityPick {
    IdentityPick::Release {
        source: MetadataSource::MusicBrainz,
        release_id: release_id.to_string(),
    }
}

fn view(tab: TriageTab) -> ImportListView {
    ImportListView {
        tab,
        ..ImportListView::default()
    }
}

fn flattened(rows: &ImportQueueRows, view: &ImportListView) -> Flattened {
    flatten(rows, view, &BTreeMap::new()).expect("the queue flattens")
}

/// The item sequence, as one readable line per item.
fn sequence(rows: &ImportQueueRows, flat: &Flattened) -> Vec<String> {
    flat.items
        .iter()
        .map(|item| match item {
            ItemRef::Header(index) => format!("group {}", flat.headers[*index].group.name),
            ItemRef::Candidate(index) => {
                format!("candidate {}", flat.rows[*index].row.display_path)
            }
            ItemRef::Boundary(index) => {
                format!("boundary {}", rows.boundaries[*index].display_path)
            }
            ItemRef::Invalid(index) => format!("invalid {}", rows.candidates[*index].display_path),
        })
        .collect()
}

fn row_for<'a>(flat: &'a Flattened, display_path: &str) -> &'a TriageRow {
    flat.rows
        .iter()
        .map(|placed| &placed.row)
        .find(|row| row.display_path == display_path)
        .unwrap_or_else(|| panic!("{display_path} is a placed row"))
}
