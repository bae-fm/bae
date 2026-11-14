//! The list's view and folds: a view change answers where the new view
//! opens, its first selected row, and a fold answers nothing.

use crate::selection::SelectionChange;
use crate::tables::import_list::tests::*;
use crate::ImportListOpening;
use crate::ImportListOrder;
use crate::TriageTab;

/// Where the view `view` opens in `list`, as the write that sets it answers:
/// the position, and the key a chain of windows starts at. The key reads the
/// row at the position back: the window from it starts with that row.
async fn opened_at(lib: &Library, list: crate::ImportListId, view: crate::ImportListView) -> u64 {
    let tables = lib.tables();
    let opening = tables
        .set_import_list_view(list, view)
        .await
        .unwrap()
        .expect("the view changed, so the write answers where it opens");
    match opening {
        ImportListOpening::Top => 0,
        ImportListOpening::At { key, position } => {
            assert_eq!(
                tables.import_key_at(list, position).await.unwrap().as_ref(),
                Some(&key),
                "the key is the key of the row at the position"
            );
            let from = bae_store::ListWindow::Forward { from: Some(key), until: None, limit: 1 };
            let first = tables.read_import_window(list, from).await.unwrap();
            let shown = items_sequence(&first.read.rows);
            let selected = lib.read(list).await;
            let whole = sequence(&selected);
            assert_eq!(shown[0], whole[position as usize], "the window from the key starts there");
            position
        }
    }
}

/// Folding every group is one write, and so is opening them all again.
#[tokio::test]
async fn every_group_folds_and_opens_at_once() {
    let lib = Library::new().await;
    let tables = crate::tables::ImportTables::new(lib.db.clone());
    lib.run(lib.scan("A/One") + lib.scan("B/Two") + lib.scan("Loose")).await;
    let list = lib.open(ordered(TriageTab::Pending, ImportListOrder::PathAscending)).await;
    tables.set_import_list_groups_folded(list, true).await.unwrap();
    let read = lib.read(list).await;
    assert_eq!(sequence(&read), ["group A", "group B", "candidate Loose"]);
    tables.set_import_list_groups_folded(list, false).await.unwrap();
    assert_eq!(
        sequence(&lib.read(list).await),
        ["group A", "candidate A/One", "group B", "candidate B/Two", "candidate Loose"]
    );
}

/// A view opens at its first selected row: the write that sets it says where
/// that row sits in the list it shows, by key and position. A view showing no
/// selected row opens at its top, and the view it already shows answers
/// nothing.
#[tokio::test]
async fn a_view_opens_at_its_first_selected_row() {
    let lib = Library::new().await;
    let tables = lib.tables();
    lib.run(lib.scan("Box/1") + lib.scan("Box/2") + lib.scan("Loose") + lib.scan("Zed")).await;
    let list = lib.open(ordered(TriageTab::Pending, ImportListOrder::PathDescending)).await;
    lib.select(list, SelectionChange::Replace { keys: vec![lib.key("Zed"), lib.key("Box/2")] })
        .await;
    assert_eq!(
        tables
            .set_import_list_view(
                list,
                ordered(TriageTab::Pending, ImportListOrder::PathDescending)
            )
            .await
            .unwrap(),
        None,
        "the view it already shows opens nowhere"
    );
    assert_eq!(
        opened_at(&lib, list, ordered(TriageTab::Pending, ImportListOrder::PathAscending)).await,
        2,
        "the header, Box/1, then Box/2"
    );
    lib.select(list, SelectionChange::Replace { keys: Vec::new() }).await;
    assert_eq!(
        opened_at(&lib, list, ordered(TriageTab::Pending, ImportListOrder::PathDescending)).await,
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
    lib.run(
        lib.scan("A/One")
            + lib.scan("A/Two")
            + lib.scan("B/Three")
            + lib.scan("Loose")
            + lib.scan("Zed"),
    )
    .await;
    let ascending = ordered(TriageTab::Pending, ImportListOrder::PathAscending);
    let descending = ordered(TriageTab::Pending, ImportListOrder::PathDescending);
    let list = lib.open(descending.clone()).await;
    lib.select(list, SelectionChange::Replace { keys: vec![lib.key("Zed")] }).await;
    assert_eq!(
        opened_at(&lib, list, ascending.clone()).await,
        6,
        "header A, A/One, A/Two, header B, B/Three, Loose, then Zed"
    );
    assert_eq!(opened_at(&lib, list, descending.clone()).await, 0, "Zed leads");
    lib.select(list, SelectionChange::Replace { keys: vec![lib.key("B/Three"), lib.key("A/Two")] })
        .await;
    assert_eq!(opened_at(&lib, list, ascending).await, 2, "header A, A/One, then A/Two");
    assert_eq!(
        opened_at(&lib, list, descending).await,
        3,
        "Zed, Loose, then B/Three leads the group B, whose header follows it"
    );
}
