//! Releases read from several folders picked together: what their grouping
//! places, and the folders it takes in.

use crate::tables::import_list::placement::tests::*;
use crate::tables::import_list::tests::*;

/// The folders a grouping takes in leave the list while the grouping stands,
/// and the grouping's release, shown as its first folder by the draft the
/// combine writes, is dated by the earliest of them.
#[tokio::test]
async fn a_grouping_takes_its_folders_out_and_is_dated_by_the_earliest() {
    let lib = Library::new().await;
    lib.run(lib.scan_dated("Disc 1", 100) + lib.scan_dated("Disc 2", 200)).await;
    assert_eq!(
        lib.moved(lib.combine(&["Disc 1", "Disc 2"], "Discs")).await,
        moves([
            (lib.release("Disc 1"), None),
            (lib.release("Disc 2"), None),
            (
                lib.grouped_release("Discs"),
                Some(found("Disc 1").dated(100).shown("Disc 1", Some("")))
            ),
        ])
    );
    assert_eq!(
        lib.moved(lib.scan_dated("Disc 1", 500)).await,
        moves([(
            lib.grouped_release("Discs"),
            Some(found("Disc 1").dated(200).shown("Disc 1", Some("")))
        )])
    );
}

/// A grouping's release is skipped by its grouping, and one that cannot be
/// worked on as it stands, a folder it takes in gone, is Found's import
/// error.
#[tokio::test]
async fn a_grouping_skips_and_blocks_its_release() {
    let lib = Library::new().await;
    lib.run(lib.scan("Disc 1") + lib.scan("Disc 2") + lib.combine(&["Disc 1", "Disc 2"], "Discs"))
        .await;
    assert_eq!(
        lib.moved(lib.grouping_skipped("Discs", true)).await,
        moves([(
            lib.grouped_release("Discs"),
            Some(found("Disc 1").on("skipped").shown("Disc 1", Some("")))
        )])
    );
    assert_eq!(
        lib.moved(lib.grouping_skipped("Discs", false) + lib.unscan("Disc 1")).await,
        moves([(
            lib.grouped_release("Discs"),
            Some(found("Disc 1").state("import_error").shown("Disc 1", Some("")))
        )])
    );
}

/// Undoing a grouping takes its release out and gives its folders back.
#[tokio::test]
async fn undoing_a_grouping_gives_its_folders_back() {
    let lib = Library::new().await;
    lib.run(lib.scan("Disc 1") + lib.scan("Disc 2") + lib.combine(&["Disc 1", "Disc 2"], "Discs"))
        .await;
    assert_eq!(
        lib.moved(lib.separate("Discs")).await,
        moves([
            (lib.release("Disc 1"), Some(found("Disc 1"))),
            (lib.release("Disc 2"), Some(found("Disc 2"))),
            (lib.grouped_release("Discs"), None),
        ])
    );
}
