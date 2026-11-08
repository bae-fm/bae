//! The list's view and folds: each write moves the view revision the read
//! reflecting it carries, and a view opens at its first selected row.

use crate::selection::SelectionChange;
use crate::tables::import_list::tests::*;
use crate::ImportListOrder;
use crate::TriageTab;

/// Folding every group is one write, and so is opening them all again.
#[tokio::test]
async fn every_group_folds_and_opens_at_once() {
    let lib = Library::new().await;
    let tables = crate::tables::ImportTables::new(lib.db.clone());
    lib.run(lib.scan("A/One") + lib.scan("B/Two") + lib.scan("Loose")).await;
    let list = lib.open(ordered(TriageTab::Pending, ImportListOrder::PathAscending)).await;
    let folded = tables.set_import_list_groups_folded(list, true).await.unwrap();
    let read = lib.read(list).await;
    assert_eq!(sequence(&read), ["group A", "group B", "candidate Loose"]);
    assert_eq!(read.view_revision, folded);
    let opened = tables.set_import_list_groups_folded(list, false).await.unwrap();
    assert_eq!(opened, folded + 1, "each fold moves the view's revision");
    assert_eq!(
        sequence(&lib.read(list).await),
        ["group A", "candidate A/One", "group B", "candidate B/Two", "candidate Loose"]
    );
}

/// A view opens at its first selected row: the write that sets it says where
/// that row sits in the list it shows, and the list keeps that place until
/// its view is set again, whatever is selected since. A view showing no
/// selected row opens at its top.
#[tokio::test]
async fn a_view_opens_at_its_first_selected_row() {
    let lib = Library::new().await;
    let tables = crate::tables::ImportTables::new(lib.db.clone());
    lib.run(lib.scan("Box/1") + lib.scan("Box/2") + lib.scan("Loose") + lib.scan("Zed")).await;
    let list = lib.open(ordered(TriageTab::Pending, ImportListOrder::PathDescending)).await;
    assert_eq!(lib.read(list).await.summary.opened_at, 0, "a list opens at its top");
    lib.select(list, SelectionChange::Replace { keys: vec![lib.key("Zed"), lib.key("Box/2")] })
        .await;
    assert_eq!(lib.read(list).await.summary.opened_at, 0, "a selection moves no view");
    tables
        .set_import_list_view(list, ordered(TriageTab::Pending, ImportListOrder::PathAscending))
        .await
        .unwrap();
    assert_eq!(lib.read(list).await.summary.opened_at, 2, "the header, Box/1, then Box/2");
    lib.select(list, SelectionChange::Replace { keys: Vec::new() }).await;
    assert_eq!(lib.read(list).await.summary.opened_at, 2, "the view keeps the place it opened at");
    tables
        .set_import_list_view(list, ordered(TriageTab::Pending, ImportListOrder::PathDescending))
        .await
        .unwrap();
    assert_eq!(
        lib.read(list).await.summary.opened_at,
        0,
        "a view showing no selected row opens at its top"
    );
}

/// The place a view opens at counts the groups' headers and members before
/// the first selected row, whichever way the order runs, and a selection
/// reaching the list's end opens there.
#[tokio::test]
async fn a_view_opens_at_a_selected_row_among_groups() {
    let lib = Library::new().await;
    let tables = crate::tables::ImportTables::new(lib.db.clone());
    lib.run(
        lib.scan("A/One")
            + lib.scan("A/Two")
            + lib.scan("B/Three")
            + lib.scan("Loose")
            + lib.scan("Zed"),
    )
    .await;
    let list = lib.open(ordered(TriageTab::Pending, ImportListOrder::PathDescending)).await;
    lib.select(list, SelectionChange::Replace { keys: vec![lib.key("Zed")] }).await;
    tables
        .set_import_list_view(list, ordered(TriageTab::Pending, ImportListOrder::PathAscending))
        .await
        .unwrap();
    assert_eq!(
        lib.read(list).await.summary.opened_at,
        6,
        "header A, A/One, A/Two, header B, B/Three, Loose, then Zed"
    );
    tables
        .set_import_list_view(list, ordered(TriageTab::Pending, ImportListOrder::PathDescending))
        .await
        .unwrap();
    assert_eq!(lib.read(list).await.summary.opened_at, 0, "Zed leads");
    lib.select(list, SelectionChange::Replace { keys: vec![lib.key("B/Three"), lib.key("A/Two")] })
        .await;
    tables
        .set_import_list_view(list, ordered(TriageTab::Pending, ImportListOrder::PathAscending))
        .await
        .unwrap();
    assert_eq!(lib.read(list).await.summary.opened_at, 2, "header A, A/One, then A/Two");
    tables
        .set_import_list_view(list, ordered(TriageTab::Pending, ImportListOrder::PathDescending))
        .await
        .unwrap();
    assert_eq!(
        lib.read(list).await.summary.opened_at,
        3,
        "Zed, Loose, then B/Three leads the group B, whose header follows it"
    );
}
