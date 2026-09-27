//! The import list's selection as rows: Select All over every listed key, a
//! view change keeping only the shown ones, and keys leaving with the
//! candidates they name.

use super::super::*;
use super::live_query_tests::{live_db, RELEASE_ID};
use super::{candidate_with, exec, watched_root};
use crate::import::folder_scanner::{
    CandidateFile, CategorizedFiles, FileRole, FolderCandidate, ReleaseFileScope, ScanItem,
    ScannedFile,
};
use crate::import::list::{ImportListRequest, ImportListView};
use crate::import::selection::SelectionChange;

/// `names` scanned under `root` in one scan.
async fn scanned_many(db: &Database, root: &str, names: &[String]) -> Vec<FolderCandidate> {
    db.add_watched_import_folder(root).await.unwrap();
    let generation = db
        .begin_folder_scan(root, crate::import::VolumeKind::Local)
        .await
        .unwrap();
    let mut candidates = Vec::new();
    for (n, name) in names.iter().enumerate() {
        let candidate = distinct(root, name, n as u64);
        db.save_folder_scan_item(root, generation, &ScanItem::Valid(candidate.clone()))
            .await
            .unwrap();
        candidates.push(candidate);
    }
    db.finish_folder_scan(root, generation, None).await.unwrap();
    candidates
}

/// A one-FLAC folder whose file layout no other in the test shares, so each
/// has its own content hash.
fn distinct(root: &str, name: &str, n: u64) -> FolderCandidate {
    let folder = std::path::Path::new(root).join(name);
    candidate_with(
        root,
        name,
        CategorizedFiles {
            files: vec![CandidateFile {
                proposed_audio: true,
                file: ScannedFile::new(folder.join("01.flac"), "01.flac".to_string(), 1_000 + n, 1)
                    .with_test_flac_audio(),
                role: FileRole::Audio,
            }],
            parts: Vec::new(),
        },
        ReleaseFileScope::Recursive,
    )
}

fn names(count: usize, prefix: &str) -> Vec<String> {
    (0..count).map(|n| format!("{prefix} {n:03}")).collect()
}

async fn selected(db: &Database) -> Vec<String> {
    db.load_selected_candidates()
        .await
        .unwrap()
        .into_iter()
        .map(|candidate| candidate.candidate_key)
        .collect()
}

fn texted(text: &str) -> ImportListRequest {
    ImportListRequest {
        view: ImportListView {
            filter_text: text.to_string(),
            ..ImportListView::default()
        },
        // A surface's loaded window is not what Select All reads.
        windows: std::iter::once(crate::library::LibraryPageWindow {
            offset: 0,
            limit: 50,
        })
        .collect(),
        ..ImportListRequest::default()
    }
}

fn keys(candidates: &[FolderCandidate]) -> Vec<String> {
    let mut keys: Vec<String> = candidates.iter().map(FolderCandidate::key).collect();
    keys.sort();
    keys
}

/// Select All selects every key the view lists, far past one page of rows.
#[tokio::test]
async fn select_all_selects_every_listed_key_past_one_page() {
    let (db, _tmp, root) = watched_root().await;
    let wanted = scanned_many(&db, &root, &names(120, "Album")).await;

    db.select_shown_candidates(texted("album")).await.unwrap();

    assert_eq!(selected(&db).await, keys(&wanted));
}

/// Changing the view keeps only the selected keys it still lists, and a view
/// listing more rows adds none of them.
#[tokio::test]
async fn a_view_change_deletes_the_selected_keys_it_hides() {
    let (db, _tmp, root) = watched_root().await;
    let mut all_names = names(3, "Album");
    all_names.extend(names(2, "Other"));
    let candidates = scanned_many(&db, &root, &all_names).await;
    db.select_shown_candidates(texted("")).await.unwrap();

    db.keep_shown_candidate_selection(texted("album"))
        .await
        .unwrap();
    assert_eq!(selected(&db).await, keys(&candidates[..3]));

    db.keep_shown_candidate_selection(texted("")).await.unwrap();
    assert_eq!(selected(&db).await, keys(&candidates[..3]));
}

/// Pointing at rows replaces, toggles, or extends to a range of the rows the
/// list shows, loaded or not.
#[tokio::test]
async fn pointing_at_rows_replaces_toggles_or_extends() {
    let (db, _tmp, root) = watched_root().await;
    let candidates = scanned_many(&db, &root, &names(100, "Album")).await;
    let all = keys(&candidates);

    db.change_candidate_selection(
        texted(""),
        SelectionChange::Replace {
            keys: vec![all[3].clone()],
        },
    )
    .await
    .unwrap();
    assert_eq!(selected(&db).await, vec![all[3].clone()]);

    db.change_candidate_selection(
        texted(""),
        SelectionChange::Toggle {
            add: vec![all[5].clone()],
            remove: vec![all[3].clone()],
        },
    )
    .await
    .unwrap();
    assert_eq!(selected(&db).await, vec![all[5].clone()]);

    let listed = {
        let request = ImportListRequest {
            view: ImportListView {
                order: crate::import::ImportListOrder::PathAscending,
                ..ImportListView::default()
            },
            ..ImportListRequest::default()
        };
        db.change_candidate_selection(
            request,
            SelectionChange::Extend {
                from: all[5].clone(),
                to: all[90].clone(),
            },
        )
        .await
        .unwrap();
        all[5..=90].to_vec()
    };
    assert_eq!(selected(&db).await, listed);
}

/// Each change says the selection revision of the list read that reflects it,
/// and a change that changes nothing says the one the list already carries, so
/// a surface can tell a list read taken before its change from one after.
#[tokio::test]
async fn a_change_says_the_list_revision_that_reflects_it() {
    let (db, _tmp, root) = watched_root().await;
    let all = keys(&scanned_many(&db, &root, &names(2, "Album")).await);
    let revision_listed = || async {
        db.load_import_list(texted(""))
            .await
            .unwrap()
            .selection_revision
    };
    let replace = |key: &String| SelectionChange::Replace {
        keys: vec![key.clone()],
    };

    let before = revision_listed().await;
    let first = db
        .change_candidate_selection(texted(""), replace(&all[0]))
        .await
        .unwrap();
    assert!(first > before);
    assert_eq!(revision_listed().await, first);

    let second = db
        .change_candidate_selection(texted(""), replace(&all[1]))
        .await
        .unwrap();
    assert!(second > first);
    assert_eq!(revision_listed().await, second);

    let unchanged = db
        .change_candidate_selection(texted(""), replace(&all[1]))
        .await
        .unwrap();
    assert_eq!(unchanged, second);
}

/// A key leaves the selection in the same write that removes its release from
/// the queue, imports its files, or sets it aside; a rescan that rewrites a
/// release keeps it selected.
#[tokio::test]
async fn a_key_leaves_with_its_candidate_and_stays_through_a_rewrite() {
    let (db, _temp) = live_db().await;
    let root = crate::import::watched_folder::host_root("/music");
    let candidates = scanned_many(&db, &root, &names(4, "Album")).await;
    db.select_shown_candidates(texted("")).await.unwrap();

    // A rescan that finds the same folders rewrites nothing away.
    scanned_many(&db, &root, &names(4, "Album")).await;
    assert_eq!(selected(&db).await, keys(&candidates));

    // Imported: a release now holds the first folder's files.
    exec(
        &db,
        "UPDATE releases SET content_hash = ?1 WHERE id = ?2",
        &[candidates[0].files.content_hash().as_str(), RELEASE_ID],
    )
    .await;
    // Set aside.
    db.set_import_candidate_skipped(&candidates[1].key(), true)
        .await
        .unwrap();
    // Gone from the scan: the last folder is not found again, and the third
    // is found changed, which rewrites its row.
    let generation = db
        .begin_folder_scan(&root, crate::import::VolumeKind::Local)
        .await
        .unwrap();
    let changed = distinct(&root, "Album 002", 50);
    assert_ne!(
        changed.files.content_hash(),
        candidates[2].files.content_hash()
    );
    for candidate in [&candidates[0], &candidates[1], &changed] {
        db.save_folder_scan_item(&root, generation, &ScanItem::Valid(candidate.clone()))
            .await
            .unwrap();
    }
    db.finish_folder_scan(&root, generation, None)
        .await
        .unwrap();

    assert_eq!(selected(&db).await, vec![candidates[2].key()]);
}

/// What two thousand selected rows can be told to do comes from the one read
/// of the selected rows, joined with what is running for them.
#[tokio::test]
async fn the_offers_of_two_thousand_selected_rows_come_from_one_read() {
    let (db, _tmp, root) = watched_root().await;
    scanned_many(&db, &root, &names(2_000, "Album")).await;
    db.select_shown_candidates(texted("")).await.unwrap();

    let mut summaries = crate::import::selection::watch_selection(
        db.subscribe_selected_candidates(),
        crate::import::candidate_runtime::RuntimeFactsWatch::of(&Default::default()),
        &tokio::runtime::Handle::current(),
    );
    let summary = summaries.recv().await.expect("the selection answers");

    assert_eq!(summary.count, 2_000);
    assert_eq!(summary.single, None);
    let identify = summary
        .offers
        .iter()
        .find(|offer| offer.action == crate::import::CandidateAction::Identify)
        .expect("every selected row can be identified");
    assert_eq!(identify.count, 2_000);
}
