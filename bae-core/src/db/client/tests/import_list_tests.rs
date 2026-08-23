//! The list read, end to end against a database.
//!
//! The rules that place a row are tested over row literals in
//! `import::list::tests`; what these check is the read that produces those
//! rows — which columns it gathers, which documents it follows, and which
//! tables it deliberately never touches.

use super::super::*;
use crate::identify::{GroupKey, ResultProvenance, TerminalVerdict};

use crate::import::folder_scanner::{
    CandidateFile, CategorizedFiles, FileRole, FolderCandidate, ReleaseFileScope, ScanItem,
    ScannedFile,
};
use crate::import::list::{ImportListItem, ImportListRequest, ImportListView};
use crate::import::search::{MetadataResult, SourceTracks};
use crate::import::{ClaimLevel, IdentityPick, PayloadSource, TriageTab};
use coven::FixedClock;
use std::path::PathBuf;

fn now() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-01-15T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc)
}

async fn empty_db() -> (Database, tempfile::TempDir) {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("test.db");
    let db = Database::new_test(
        path.to_str().unwrap(),
        Arc::new(FixedClock(now())),
        Arc::new(coven::UuidProvider),
    )
    .await
    .unwrap();
    (db, tmp)
}

fn candidate(root: &str, name: &str) -> FolderCandidate {
    FolderCandidate {
        path: PathBuf::from(format!("{root}/{name}")),
        file_root: PathBuf::from(format!("{root}/{name}")),
        name: name.to_string(),
        files: CategorizedFiles {
            files: vec![CandidateFile {
                proposed_audio: true,
                file: ScannedFile::new(
                    PathBuf::from(format!("{root}/{name}/01.flac")),
                    "01.flac".to_string(),
                    1_000,
                ),
                role: FileRole::Audio,
            }],
            format_label: "FLAC".to_string(),
        },
        watched_folder_path: root.to_string(),
        scope: ReleaseFileScope::Recursive,
        file_edit_revision: 0,
        display_path: name.to_string(),
        resolved_boundaries: Vec::new(),
        combine_ancestor_key: None,
    }
}

/// One scanned candidate under a fresh watched root.
async fn scanned(db: &Database, root: &str, name: &str) -> FolderCandidate {
    db.add_watched_import_folder(root).await.unwrap();
    let generation = db.begin_folder_scan(root).await.unwrap();
    let candidate = candidate(root, name);
    db.save_folder_scan_item(root, generation, &ScanItem::Valid(candidate.clone()))
        .await
        .unwrap();
    db.finish_folder_scan(root, generation, None).await.unwrap();
    candidate
}

fn verdict(release_id: &str) -> TerminalVerdict {
    TerminalVerdict::Found {
        matches: vec![MetadataResult {
            source: MetadataSource::MusicBrainz,
            release_id: release_id.to_string(),
            title: "Verdict Album".to_string(),
            artist: Some("Verdict Artist".to_string()),
            year: Some(1999),
            format: Some("CD".to_string()),
            label: None,
            catalog_number: None,
            country: None,
            cover_art: None,
            source_group_id: Some("group-1".to_string()),
            source_tracks: Some(SourceTracks::Listed {
                count: 1,
                total_duration_ms: Some(1_000),
            }),
        }],
        track_count: 1,
        group: GroupKey {
            source: MetadataSource::MusicBrainz,
            source_group_id: "group-1".to_string(),
        },
        provenance: vec![ResultProvenance {
            by_disc_id: true,
            by_barcode: false,
            matches_catalog: false,
        }],
    }
}

async fn save_verdict(db: &Database, candidate: &FolderCandidate, release_id: &str) {
    assert!(db
        .save_import_candidate_verdict(&NewImportCandidateVerdict {
            content_hash: candidate.files.content_hash(),
            folder_path: candidate.path.to_string_lossy().into_owned(),
            verdict: verdict(release_id),
            probed_total_duration_ms: 1_000,
            expected_edit_revision: 0,
            identity_pick: None,
        })
        .await
        .unwrap());
}

fn musicbrainz_release(release_id: &str, title: &str) -> serde_json::Value {
    serde_json::json!({
        "id": release_id,
        "title": title,
        "artist-credit": [{ "name": "Picked Artist", "artist": { "id": "artist-1", "name": "Picked Artist" } }],
        "cover-art-archive": { "count": 0, "artwork": false, "front": false, "back": false, "darkened": false },
        "media": [{ "position": 1, "tracks": [
            { "id": "track-1", "position": 1, "number": "1", "title": "Track One",
              "recording": { "id": "rec-1", "title": "Track One" } }
        ] }],
    })
}

async fn request(tab: TriageTab) -> ImportListRequest {
    ImportListRequest {
        view: ImportListView {
            tab,
            ..ImportListView::default()
        },
        windows: std::iter::once(crate::library::LibraryPageWindow {
            offset: 0,
            limit: 50,
        })
        .collect(),
        runtime_facts: Default::default(),
    }
}

fn rows(projection: &crate::import::ImportListProjection) -> Vec<crate::import::TriageRow> {
    projection
        .windows
        .iter()
        .flat_map(|window| &window.items)
        .filter_map(|item| match item {
            ImportListItem::Candidate(row) => Some(row.clone()),
            _ => None,
        })
        .collect()
}

/// A candidate the user picked a release for leads with that release as its
/// own archived documents describe it — not with whatever the verdict named.
#[tokio::test]
async fn a_picked_row_leads_with_the_archived_document() {
    let (db, tmp) = empty_db().await;
    let root = tmp.path().join("watched");
    std::fs::create_dir_all(&root).unwrap();
    let root = root.to_str().unwrap();
    let candidate = scanned(&db, root, "Album").await;
    save_verdict(&db, &candidate, "mb-verdict").await;

    db.save_source_release_payloads(&[DbSourceReleasePayload {
        source: PayloadSource::MusicBrainz,
        source_release_id: "mb-picked".to_string(),
        json: musicbrainz_release("mb-picked", "Picked Album").to_string(),
        fetched_at: now(),
    }])
    .await
    .unwrap();
    db.save_candidate_identity_pick(
        &candidate.files.content_hash(),
        &candidate.path.to_string_lossy(),
        &IdentityPick::Release {
            source: MetadataSource::MusicBrainz,
            release_id: "mb-picked".to_string(),
            claim: ClaimLevel::Exact,
        },
    )
    .await
    .unwrap();

    let projection = db
        .load_import_list(request(TriageTab::Pending).await)
        .await
        .unwrap();
    let matched = rows(&projection)[0]
        .matched
        .clone()
        .expect("the row leads with the picked release");
    assert_eq!(matched.release_id, "mb-picked");
    assert_eq!(matched.title, "Picked Album");
}

/// With nothing archived behind the pick, the row leads with its folder name
/// rather than the release the verdict happened to name.
#[tokio::test]
async fn a_pick_with_no_documents_leads_with_nothing() {
    let (db, tmp) = empty_db().await;
    let root = tmp.path().join("watched");
    std::fs::create_dir_all(&root).unwrap();
    let root = root.to_str().unwrap();
    let candidate = scanned(&db, root, "Album").await;
    save_verdict(&db, &candidate, "mb-verdict").await;
    db.save_candidate_identity_pick(
        &candidate.files.content_hash(),
        &candidate.path.to_string_lossy(),
        &IdentityPick::Release {
            source: MetadataSource::MusicBrainz,
            release_id: "mb-never-fetched".to_string(),
            claim: ClaimLevel::Exact,
        },
    )
    .await
    .unwrap();

    let projection = db
        .load_import_list(request(TriageTab::Pending).await)
        .await
        .unwrap();
    let row = rows(&projection).remove(0);
    assert!(row.matched.is_none());
    assert!(
        row.picked.is_some(),
        "the decision itself is still on the row"
    );
}

/// A row with a verdict and no pick leads with the verdict's own lead match,
/// read off its stored columns.
#[tokio::test]
async fn a_row_without_a_pick_leads_with_the_verdicts_lead_match() {
    let (db, tmp) = empty_db().await;
    let root = tmp.path().join("watched");
    std::fs::create_dir_all(&root).unwrap();
    let root = root.to_str().unwrap();
    let candidate = scanned(&db, root, "Album").await;
    save_verdict(&db, &candidate, "mb-verdict").await;

    let projection = db
        .load_import_list(request(TriageTab::Pending).await)
        .await
        .unwrap();
    let matched = rows(&projection)[0]
        .matched
        .clone()
        .expect("the row leads with the verdict's lead");
    assert_eq!(matched.release_id, "mb-verdict");
    assert_eq!(matched.title, "Verdict Album");
    assert_eq!(matched.artist.as_deref(), Some("Verdict Artist"));
}

/// The list places a row from `scan_candidate`'s own columns and the stored
/// verdict — never from the folder's files. Deleting every file row leaves the
/// list unchanged, which is what "reads columns, decodes nothing" means.
#[tokio::test]
async fn the_list_places_a_row_without_reading_its_files() {
    let (db, tmp) = empty_db().await;
    let root = tmp.path().join("watched");
    std::fs::create_dir_all(&root).unwrap();
    let root = root.to_str().unwrap();
    let candidate = scanned(&db, root, "Album").await;
    save_verdict(&db, &candidate, "mb-verdict").await;

    let before = db
        .load_import_list(request(TriageTab::Pending).await)
        .await
        .unwrap();
    db.call(|sql| {
        sql.execute("DELETE FROM scan_candidate_file", [])
            .map(|_| ())
            .map_err(DbError::from)
    })
    .await
    .unwrap();
    let after = db
        .load_import_list(request(TriageTab::Pending).await)
        .await
        .unwrap();

    assert_eq!(rows(&before), rows(&after));
    assert_eq!(before.summary, after.summary);
}

/// The pane's read stands the stored verdict back up with the live library
/// status of every release it names, aligned with its matches — the answer a
/// candidate shows when no run is in flight.
#[tokio::test]
async fn the_detail_resumes_the_stored_verdict_with_live_statuses() {
    let (db, tmp) = empty_db().await;
    let root = tmp.path().join("watched");
    std::fs::create_dir_all(&root).unwrap();
    let root = root.to_str().unwrap();
    let candidate = scanned(&db, root, "Album").await;
    save_verdict(&db, &candidate, "mb-verdict").await;

    let detail = db
        .load_import_candidate(&candidate.path.to_string_lossy())
        .await
        .unwrap()
        .expect("the scanned candidate reads back");

    let crate::identify::IdentifyState::Found {
        matches,
        library_statuses,
        ..
    } = &detail.resumed_identify_state
    else {
        panic!(
            "expected the stored Found, got {:?}",
            detail.resumed_identify_state
        );
    };
    assert_eq!(
        matches
            .iter()
            .map(|result| result.release_id.as_str())
            .collect::<Vec<_>>(),
        vec!["mb-verdict"]
    );
    assert_eq!(
        library_statuses
            .iter()
            .map(|status| (status.release_id.as_str(), status.release_in_library))
            .collect::<Vec<_>>(),
        vec![("mb-verdict", false)],
        "statuses ride the resumed state, aligned with its matches"
    );
}

/// A verdict stored for an earlier file-edit revision describes files the
/// candidate no longer has; it does not resume, and the row goes back to
/// waiting on identification.
#[tokio::test]
async fn a_verdict_from_another_revision_does_not_resume() {
    let (db, tmp) = empty_db().await;
    let root = tmp.path().join("watched");
    std::fs::create_dir_all(&root).unwrap();
    let root = root.to_str().unwrap();
    let candidate = scanned(&db, root, "Album").await;
    save_verdict(&db, &candidate, "mb-verdict").await;
    let key = candidate.path.to_string_lossy().into_owned();
    let edited = key.clone();
    db.call(move |sql| {
        sql.execute(
            "UPDATE scan_candidate SET file_edit_revision = 1 WHERE path = ?1",
            params![edited],
        )
        .map(|_| ())
        .map_err(DbError::from)
    })
    .await
    .unwrap();

    let detail = db
        .load_import_candidate(&key)
        .await
        .unwrap()
        .expect("the scanned candidate still reads back");

    assert!(matches!(
        detail.resumed_identify_state,
        crate::identify::IdentifyState::Idle
    ));
    assert!(detail.answer.is_none());
    assert!(detail.matched.is_none());
}
