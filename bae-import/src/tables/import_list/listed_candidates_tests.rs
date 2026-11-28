//! The releases the import list lists, as automation reads them: every kind,
//! in key order, with one statement for the releases and one for the drafts.

use super::*;
use crate::tables::import_list::tests::*;

const PATHS: [&str; 4] = ["Imported", "Invalid", "Pending", "Skipped"];

/// A pending candidate holding a draft, a skipped one, one this device
/// imported and a folder that failed validation, in a library of their own.
async fn listed_library() -> Library {
    let lib = Library::new().await;
    lib.run(lib.scan("Pending") + lib.scan("Skipped") + lib.skip("Skipped") + lib.scan("Imported"))
        .await;
    lib.run(lib.import_of(
        lib.candidate("Imported"),
        "Imported",
        "2026-01-01T00:00:00Z",
        "Album Title",
        Vec::new(),
    ))
    .await;
    lib.run(lib.scan_invalid("Invalid", None)).await;
    lib
}

/// The key a listed release is under, and whether it is a candidate.
fn listed_key(listed: &ListedCandidate) -> (crate::CandidateKey, bool) {
    match listed {
        ListedCandidate::Folder(folder) => (folder.detail.candidate.key(), true),
        ListedCandidate::Invalid(row) => (row.candidate_key.clone(), false),
    }
}

#[tokio::test]
async fn the_list_reads_every_kind_of_release_in_key_order_with_two_statements() {
    let lib = listed_library().await;
    let counter = lib.db.track_work_for_test().await.unwrap();
    let listed = lib.tables().load_listed_candidates().await.unwrap();
    assert_eq!(counter.take().app_read_statements, 2);

    let mut expected: Vec<_> =
        PATHS.iter().map(|path| (lib.key(path), *path != "Invalid")).collect();
    expected.sort();
    assert_eq!(listed.iter().map(listed_key).collect::<Vec<_>>(), expected);
}

#[tokio::test]
async fn one_release_reads_with_two_statements() {
    let lib = listed_library().await;
    for path in PATHS {
        let counter = lib.db.track_work_for_test().await.unwrap();
        let listed = lib.tables().load_listed_candidate(&lib.key(path)).await.unwrap();
        assert_eq!(counter.take().app_read_statements, 2, "{path}");
        assert_eq!(listed.as_ref().map(listed_key), Some((lib.key(path), path != "Invalid")));
    }
}
