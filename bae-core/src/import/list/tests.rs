//! The flatten's rules, over row literals.
//!
//! Everything the tab shows is decided here — which rows exist, in what order,
//! under which header, in which tab, and what the chrome around them says — so
//! the fixtures are the columns the read gathers rather than a database.

use super::*;
use crate::db::{CandidateStateListRow, ImportQueueRows, ScanCandidateKind, ScanCandidateListRow};
use crate::identify::{LeadMatch, VerdictKind, VerdictSummary};
use crate::import::folder_registry::host_root;
use crate::import::folder_scanner::InvalidReason;
use crate::import::search::SourceTracks;
use crate::import::types::MetadataSource;
use crate::import::{FolderScanStatus, ImportedRelease};
use crate::import::{IdentifyPhase, TriageImportStatus, TriagePlacement};

mod flatten;
mod flatten_groups;
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
/// agreeing, and identification's seed for that match.
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
        metadata_provenance: Some(MetadataProvenance::ExternalRelease {
            source: MetadataSource::MusicBrainz,
            release_id: release_id.to_string(),
            partners: vec![],
        }),
        metadata_draft_valid: true,
        metadata_summary: None,
        selected_cover: None,
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
        metadata_provenance: None,
        metadata_draft_valid: false,
        metadata_summary: None,
        selected_cover: None,
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
        metadata_provenance: None,
        metadata_draft_valid: false,
        metadata_summary: None,
        selected_cover: None,
    }
}

/// The external release seed chosen from a release row.
fn external_release_seed(release_id: &str) -> MetadataProvenance {
    MetadataProvenance::ExternalRelease {
        source: MetadataSource::MusicBrainz,
        release_id: release_id.to_string(),
        partners: vec![],
    }
}

/// Mark `display_path`'s candidate imported as `release_id` at `imported_at`
/// epoch milliseconds — what puts its row on Done.
fn imported(rows: &mut ImportQueueRows, display_path: &str, release_id: &str, imported_at: i64) {
    let hash = format!("hash-{display_path}");
    rows.imported.insert(
        hash.clone(),
        ImportedRelease {
            release_id: release_id.to_string(),
            album_id: format!("album-{release_id}"),
        },
    );
    rows.imported_at.insert(hash, imported_at);
}

fn view(tab: TriageTab) -> ImportListView {
    ImportListView {
        tab,
        ..ImportListView::default()
    }
}

fn flattened(rows: &ImportQueueRows, view: &ImportListView) -> Flattened {
    flatten(rows, &request(view.clone())).expect("the queue flattens")
}

/// A request showing `view` with nothing running and nothing in the outbox.
fn request(view: ImportListView) -> ImportListRequest {
    ImportListRequest {
        view,
        ..ImportListRequest::default()
    }
}

fn queued_request(view: ImportListView, display_paths: &[&str]) -> ImportListRequest {
    ImportListRequest {
        view,
        runtime_facts: display_paths
            .iter()
            .map(|display_path| {
                (
                    key(display_path),
                    TriageRuntimeFacts {
                        identify_phase: Some(IdentifyPhase::Queued),
                        importing: false,
                    },
                )
            })
            .collect(),
        ..ImportListRequest::default()
    }
}

fn flattened_queued(
    rows: &ImportQueueRows,
    view: ImportListView,
    display_paths: &[&str],
) -> Flattened {
    flatten(rows, &queued_request(view, display_paths)).expect("the queue flattens")
}

/// The item sequence, as one readable line per item.
fn sequence(rows: &ImportQueueRows, flat: &Flattened) -> Vec<String> {
    flat.items
        .iter()
        .map(|item| match item {
            ItemRef::Header(index) => format!("group {}", flat.headers[*index].group.name),
            ItemRef::Candidate { index, .. } => {
                format!("candidate {}", flat.rows[*index].row.display_path)
            }
            ItemRef::Invalid { index, .. } => {
                format!("invalid {}", rows.candidates[*index].display_path)
            }
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
