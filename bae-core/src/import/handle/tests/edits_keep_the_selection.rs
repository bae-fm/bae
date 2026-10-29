//! Typing into the pane of the selected row writes its draft, which the list
//! reads again: the row stays where it is and stays selected, under every
//! view of Found that shows it.

use super::*;
use crate::import::selection::{SelectionChange, SingleSelection};
use crate::import::{
    ArtistAssignment, CandidateEditField, Credited, DraftFieldEdit, ImportListItem,
    ImportListSnapshot, ImportListSubscription, ImportListView, PendingFilter, PressingFactEdit,
    RawLabelEdit,
};

/// The next snapshot the list delivers.
async fn delivered(list: &ImportListSubscription) -> ImportListSnapshot {
    tokio::time::timeout(HANG_GUARD, list.next())
        .await
        .expect("the list delivers")
        .expect("the list answers")
}

/// The candidate rows `snapshot` holds: each one's stable key, and its row.
fn candidate_rows(snapshot: &ImportListSnapshot) -> Vec<(String, &crate::import::TriageRow)> {
    snapshot
        .windows
        .iter()
        .flat_map(|window| &window.items)
        .filter_map(|item| match item {
            ImportListItem::Candidate { row, .. } => Some((item.stable_key(), row)),
            ImportListItem::GroupHeader { .. } | ImportListItem::Invalid { .. } => None,
        })
        .collect()
}

/// Every edit the pane's album fields write but the title, one after
/// another: the years, the labels, the barcodes and a choice of what the
/// pressing is.
fn field_edits() -> Vec<DraftFieldEdit> {
    vec![
        DraftFieldEdit::Text {
            field: CandidateEditField::PressingYear,
            value: "1997".to_string(),
        },
        DraftFieldEdit::Text {
            field: CandidateEditField::PressingYear,
            value: "1998".to_string(),
        },
        DraftFieldEdit::Text {
            field: CandidateEditField::AlbumYear,
            value: "1995".to_string(),
        },
        DraftFieldEdit::Labels(vec![RawLabelEdit {
            name: "Label Name".to_string(),
            catalog_number: "CAT-2".to_string(),
        }]),
        DraftFieldEdit::Barcodes(vec!["0123456789012".to_string()]),
        DraftFieldEdit::PressingFact(PressingFactEdit::Status(Some(
            bae_types::pressing::ReleaseStatus::Promotion,
        ))),
    ]
}

/// Select the identified row under `view`, write every pane edit to its
/// draft, the album title last, and read the list until it shows the title:
/// the row shows the title, so that read reflects every write before it. The
/// row keeps its key and stays selected, and neither the selection nor the
/// view moved.
async fn edits_under(view: ImportListView) {
    let (handle, _tmp, key) = super::release_link::picked_fixture("edited-while-selected").await;
    let list = handle.subscribe_whole_list(view).await;
    let opened = delivered(&list).await;
    let stable_key = crate::import::list::candidate_stable_key(&key);
    assert_eq!(
        candidate_rows(&opened)
            .into_iter()
            .map(|(stable, _)| stable)
            .collect::<Vec<_>>(),
        vec![stable_key.clone()],
        "the view shows the identified row"
    );
    let selected_at = list
        .change_selection(SelectionChange::Replace {
            keys: vec![key.clone()],
        })
        .await
        .unwrap();
    let selected = loop {
        let snapshot = delivered(&list).await;
        if snapshot.selection_revision >= selected_at {
            break snapshot;
        }
    };
    assert_eq!(
        selected.selection.single,
        Some(SingleSelection::Candidate { key: key.clone() })
    );

    for edit in field_edits() {
        handle.set_candidate_edit_field(&key, edit).await.unwrap();
    }
    // The album artist field writes on its own path; the credit it leaves
    // still matches a search for the artist.
    handle
        .set_candidate_album_artists(
            &key,
            vec![
                Credited::plain(ArtistAssignment::named("Artist Name")),
                Credited::plain(ArtistAssignment::named("Other Artist")),
            ],
        )
        .await
        .unwrap();
    handle
        .set_candidate_edit_field(
            &key,
            DraftFieldEdit::Text {
                field: CandidateEditField::AlbumTitle,
                value: "Typed Title".to_string(),
            },
        )
        .await
        .unwrap();

    let edited = loop {
        let snapshot = delivered(&list).await;
        let rows = candidate_rows(&snapshot);
        let shows_title = rows.iter().any(|(_, row)| {
            row.metadata_summary
                .as_ref()
                .is_some_and(|summary| summary.album_title == "Typed Title")
        });
        if shows_title {
            break snapshot;
        }
    };
    let rows = candidate_rows(&edited);
    assert_eq!(
        rows.iter().map(|(stable, _)| stable.clone()).collect::<Vec<_>>(),
        vec![stable_key],
        "the row keeps its key"
    );
    assert!(rows[0].1.selected, "the row stays selected");
    assert_eq!(
        edited.selection.single,
        Some(SingleSelection::Candidate { key: key.clone() })
    );
    assert_eq!(
        edited.selection_revision, selected.selection_revision,
        "no edit moved the selection"
    );
    assert_eq!(
        edited.view_revision, selected.view_revision,
        "no edit moved the view"
    );
    let draft = super::pane(&handle, &key).await.metadata_draft;
    assert_eq!(draft.pressing.year, "1998");
    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn edits_keep_the_row_selected_on_all_of_found() {
    edits_under(ImportListView::default()).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn edits_keep_the_row_selected_under_identified() {
    edits_under(ImportListView {
        pending_filter: PendingFilter::Identified,
        ..ImportListView::default()
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn edits_keep_the_row_selected_under_a_search_for_its_artist() {
    edits_under(ImportListView {
        filter_text: "Artist Name".to_string(),
        ..ImportListView::default()
    })
    .await;
}

