//! The draft's edits: typed album and track fields, artist assignments, the
//! picked library album, edits refused while an import owns the candidate, and
//! the list keeping the edited row selected.

use super::*;
use crate::handle::tests::*;
use crate::selection::{SelectionChange, SingleSelection};
use crate::{
    ArtistAssignment, CandidateEditField, Credited, DraftFieldEdit, ImportListItem,
    ImportListSnapshot, ImportListView, LiveImportList, PendingFilter, PressingFactEdit,
    RawLabelEdit,
};
use bae_util::test_channels::HANG_GUARD;

// ── The album picker: a picked library album over the one the records name ──

/// A candidate with no records files under a new album until the person
/// picks one; the pick shows on the pane, and clearing it leaves the
/// candidate to its records again.
#[tokio::test(flavor = "multi_thread")]
async fn a_picked_album_shows_over_what_the_records_name() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    assert_eq!(pane(&handle, &key).await.draft().unwrap().album, crate::DraftAlbum::New);

    let artist = make_artist("Artist Name", None, None);
    handle.records.insert_artist(&artist).await.unwrap();
    let (album, _release) =
        insert_library_release(&handle.store, &artist.id, "Library Album").await;

    handle.set_candidate_album(&key, Some(album.id.clone())).await.unwrap();
    assert_eq!(
        pane(&handle, &key).await.draft().unwrap().album,
        crate::DraftAlbum::Picked {
            album_id: album.id.clone(),
            title: "Library Album".to_string(),
        }
    );

    handle.set_candidate_album(&key, None).await.unwrap();
    let after = pane(&handle, &key).await.draft().unwrap().album.clone();
    shut_down(handle).await;
    assert_eq!(after, crate::DraftAlbum::New);
}

/// An album no longer in the library cannot be picked.
#[tokio::test(flavor = "multi_thread")]
async fn an_album_not_in_the_library_cannot_be_picked() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    let picked = handle
        .set_candidate_album(&key, Some("0b7e1f1e-0000-4000-8000-000000000000".to_string()))
        .await;
    let after = pane(&handle, &key).await.draft().unwrap().album.clone();
    shut_down(handle).await;
    assert!(picked.is_err());
    assert_eq!(after, crate::DraftAlbum::New);
}

/// The draft keeps its barcode rows as the form holds them, in order, a blank
/// row the person has not filled yet included, and keeps only what the next
/// edit leaves.
#[tokio::test(flavor = "multi_thread")]
async fn the_draft_keeps_its_barcode_rows_as_typed() {
    let (fixture, hash) = three_track_draft().await;
    let set = |barcodes: &[&str]| {
        let barcodes = barcodes.iter().map(|code| code.to_string()).collect();
        let handle = fixture.handle.clone();
        let key = fixture.key.clone();
        async move {
            handle
                .set_candidate_edit_field(&key, crate::DraftFieldEdit::Barcodes(barcodes))
                .await
                .unwrap();
        }
    };

    set(&["0123456789012", "", "5012345678900"]).await;
    assert_eq!(
        stored_draft(&fixture.handle, &hash).await.pressing.barcodes,
        ["0123456789012", "", "5012345678900"]
    );

    set(&["5012345678900"]).await;
    assert_eq!(stored_draft(&fixture.handle, &hash).await.pressing.barcodes, ["5012345678900"]);
}

/// The draft keeps its label rows as the form holds them, in order, each field
/// as typed, a blank row the person has not filled yet included, and keeps
/// only what the next edit leaves.
#[tokio::test(flavor = "multi_thread")]
async fn the_draft_keeps_its_label_rows_as_typed() {
    let (fixture, hash) = three_track_draft().await;
    let rows = |labels: &[(&str, &str)]| -> Vec<crate::RawLabelEdit> {
        labels
            .iter()
            .map(|(name, catalog_number)| crate::RawLabelEdit {
                name: name.to_string(),
                catalog_number: catalog_number.to_string(),
                ..Default::default()
            })
            .collect()
    };
    let set = |labels: Vec<crate::RawLabelEdit>| {
        let handle = fixture.handle.clone();
        let key = fixture.key.clone();
        async move {
            handle
                .set_candidate_edit_field(&key, crate::DraftFieldEdit::Labels(labels))
                .await
                .unwrap();
        }
    };

    let typed = rows(&[(" Label Name ", "CAT-001"), ("", ""), ("", "CAT 002")]);
    set(typed.clone()).await;
    assert_eq!(stored_draft(&fixture.handle, &hash).await.pressing.labels, typed);

    let kept = rows(&[("Other Label", "")]);
    set(kept.clone()).await;
    assert_eq!(stored_draft(&fixture.handle, &hash).await.pressing.labels, kept);
}

/// The next snapshot the list delivers.
async fn delivered(list: &LiveImportList) -> ImportListSnapshot {
    tokio::time::timeout(HANG_GUARD, list.next())
        .await
        .expect("the list delivers")
        .expect("the list answers")
}

/// The candidate rows `snapshot` holds: each one's stable key, and its row.
fn candidate_rows(snapshot: &ImportListSnapshot) -> Vec<(String, &crate::TriageRow)> {
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
        DraftFieldEdit::Text { field: CandidateEditField::PressingYear, value: "1997".to_string() },
        DraftFieldEdit::Text { field: CandidateEditField::PressingYear, value: "1998".to_string() },
        DraftFieldEdit::Text { field: CandidateEditField::AlbumYear, value: "1995".to_string() },
        DraftFieldEdit::Labels(vec![RawLabelEdit {
            name: "Label Name".to_string(),
            catalog_number: "CAT-2".to_string(),
            ..Default::default()
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
    let (handle, _tmp, key) = picked_fixture("edited-while-selected").await;
    let list = handle.subscribe_whole_list(view).await;
    let opened = delivered(&list).await;
    let stable_key = crate::list::candidate_stable_key(&key);
    assert_eq!(
        candidate_rows(&opened).into_iter().map(|(stable, _)| stable).collect::<Vec<_>>(),
        vec![stable_key.clone()],
        "the view shows the identified row"
    );
    let selected_at =
        list.change_selection(SelectionChange::Replace { keys: vec![key.clone()] }).await.unwrap();
    let selected = loop {
        let snapshot = delivered(&list).await;
        if snapshot.selection_revision >= selected_at {
            break snapshot;
        }
    };
    assert_eq!(selected.selection.single, Some(SingleSelection::Candidate { key: key.clone() }));

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
    assert_eq!(edited.selection.single, Some(SingleSelection::Candidate { key: key.clone() }));
    assert_eq!(
        edited.selection_revision, selected.selection_revision,
        "no edit moved the selection"
    );
    assert_eq!(edited.view_revision, selected.view_revision, "no edit moved the view");
    let draft = pane(&handle, &key).await.draft().unwrap().metadata_draft.clone();
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

/// Every preparation mutation the pane offers, each refused with the error
/// `is_expected` names, and the whole set left unwritten. The values passed in
/// are inert: a refused mutation stores nothing.
async fn assert_every_mutation_refused(
    handle: &ImportServiceHandle,
    key: &crate::CandidateKey,
    album_title: &str,
    track: crate::RawTrackEdit,
    cover: crate::CoverSelection,
    is_expected: fn(&crate::ImportError) -> bool,
) {
    let refused = |result: Result<(), crate::ImportError>| {
        let error = result.expect_err("the mutation must be refused");
        assert!(is_expected(&error), "unexpected refusal: {error:?}");
    };

    refused(
        handle
            .set_candidate_edit_field(
                key,
                crate::DraftFieldEdit::Text {
                    field: crate::CandidateEditField::AlbumTitle,
                    value: album_title.to_string(),
                },
            )
            .await
            .map(drop),
    );
    refused(
        handle
            .set_candidate_album_artists(
                key,
                vec![crate::Credited::plain(crate::ArtistAssignment::named("Blocked artist"))],
            )
            .await
            .map(drop),
    );
    refused(handle.set_candidate_track_edit(key, track.clone()).await.map(drop));
    refused(
        handle
            .set_candidate_track_artists(
                key,
                vec![track.id.clone()],
                crate::TrackArtistAssignments::AlbumArtists,
            )
            .await
            .map(drop),
    );
    refused(handle.set_candidate_cover(key, cover).await.map(drop));
    refused(handle.select_candidate_file_tags(key.clone()).await.map(drop));
    refused(handle.clear_candidate_metadata(key.clone()).await.map(drop));
}

/// A typed field replaces that one field of the form and leaves the rest to
/// the pick. Committing it empty is the person clearing the field, not undoing
/// their edit: the blank is stored and the form comes back blank.
#[tokio::test(flavor = "multi_thread")]
async fn a_typed_field_lands_in_the_next_form_empty_included() {
    let (handle, _tmp, key, hash) = pane_fixture().await;
    let tables = handle.tables.clone();
    let seeded = pane(&handle, &key).await;
    let seeded_artists = seeded.draft().unwrap().metadata_draft.clone().album_artist_assignments;

    handle
        .set_candidate_edit_field(
            &key,
            crate::DraftFieldEdit::Text {
                field: crate::CandidateEditField::AlbumTitle,
                value: "Typed Title".to_string(),
            },
        )
        .await
        .unwrap();

    let edited = pane(&handle, &key).await.draft().unwrap().metadata_draft.clone();
    assert_eq!(edited.album_title, "Typed Title");
    assert_eq!(edited.album_artist_assignments, seeded_artists, "nothing else moved with it");

    handle
        .set_candidate_edit_field(
            &key,
            crate::DraftFieldEdit::Text {
                field: crate::CandidateEditField::AlbumTitle,
                value: String::new(),
            },
        )
        .await
        .unwrap();

    assert_eq!(pane(&handle, &key).await.draft().unwrap().metadata_draft.album_title, "");
    let stored = tables.load_import_candidate_pane_rows(&hash).await.unwrap();
    assert_eq!(
        stored.draft.album_title,
        String::new(),
        "a cleared field is a value the person set, not an absent edit"
    );

    shut_down(handle).await;
}

/// A source-less candidate owns a draft immediately, so direct entry needs no
/// source selection before it can persist edits.
#[tokio::test(flavor = "multi_thread")]
async fn an_edit_with_no_metadata_source_updates_the_draft() {
    let StoredCandidate { handle, key, tmp: _tmp, .. } = stored_candidate().await;

    handle
        .set_candidate_edit_field(
            &key,
            crate::DraftFieldEdit::Text {
                field: crate::CandidateEditField::PressingYear,
                value: "1991".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(pane(&handle, &key).await.draft().unwrap().metadata_draft.pressing.year, "1991");

    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn album_artist_assignments_preserve_existing_and_new_artist_choices() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    let existing = make_artist("Existing Artist", Some("discogs-existing"), None);
    handle.records.insert_artist(&existing).await.unwrap();
    let assignments = vec![
        crate::ArtistAssignment::picked(existing.clone().into()),
        crate::ArtistAssignment::named("New Artist"),
    ];

    handle
        .set_candidate_album_artists(&key, crate::Credited::plain_list(assignments.clone()))
        .await
        .unwrap();

    assert_eq!(
        pane(&handle, &key).await.draft().unwrap().metadata_draft.album_artist_assignments,
        crate::Credited::plain_list(assignments)
    );
    shut_down(handle).await;
}

/// A row's audio is the folder's to decide: an edit naming other audio is
/// refused and the draft is left as it was.
#[tokio::test(flavor = "multi_thread")]
async fn a_track_edit_cannot_change_the_audio_it_plays() {
    let (handle, _tmp, key, hash) = pane_fixture().await;
    let before = track_rows(&pane(&handle, &key).await.draft().unwrap().mapping);
    let stored =
        crate::preparations::stored_preparation(&handle.tables, &hash).await.unwrap().unwrap();

    let error = handle
        .set_candidate_track_edit(
            &key,
            crate::RawTrackEdit {
                title: "Renamed".to_string(),
                file: before[1].file.clone(),
                ..before[0].clone()
            },
        )
        .await
        .unwrap_err();

    assert!(error.to_string().contains("audio"), "{error}");
    assert_eq!(
        crate::preparations::stored_preparation(&handle.tables, &hash)
            .await
            .unwrap()
            .unwrap()
            .metadata
            .draft,
        stored.metadata.draft
    );
    shut_down(handle).await;
}

/// An edited row comes back edited and its neighbours come back untouched.
#[tokio::test(flavor = "multi_thread")]
async fn an_edited_track_row_redraws_alone() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    let before = track_rows(&pane(&handle, &key).await.draft().unwrap().mapping);
    let first = before.first().expect("the folder names tracks").clone();
    let untouched = before[1].title.clone();

    handle
        .set_candidate_track_edit(
            &key,
            crate::RawTrackEdit {
                title: "Renamed".to_string(),
                artist_assignments: crate::TrackArtistAssignments::Explicit(vec![
                    crate::Credited::plain(crate::ArtistAssignment::named("Someone")),
                ]),
                ..first.clone()
            },
        )
        .await
        .unwrap();

    let after = track_rows(&pane(&handle, &key).await.draft().unwrap().mapping);
    assert_eq!(after.len(), before.len());
    assert_eq!(after[0].title, "Renamed");
    assert_eq!(
        after[0].artist_assignments,
        crate::TrackArtistAssignments::Explicit(vec![crate::Credited::plain(
            crate::ArtistAssignment::named("Someone")
        )])
    );
    assert_eq!(after[0].file, first.file, "the audio the row was bound to rides through the edit");
    assert_eq!(after[1].title, untouched);

    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_claimed_candidate_refuses_every_preparation_mutation() {
    let (handle, _tmp, key, hash) = pane_fixture().await;
    let before = crate::preparations::stored_preparation(&handle.tables, &hash)
        .await
        .unwrap()
        .expect("the candidate is prepared");
    let pane_before = pane(&handle, &key).await;
    let first_track = track_rows(&pane_before.draft().unwrap().mapping)[0].clone();
    let cover = pane_before
        .draft()
        .unwrap()
        .cover
        .clone()
        .expect("the fixture has a selected cover")
        .selection();
    handle.claim_candidate_for_import_for_test(&key, "import-1").await;

    assert_every_mutation_refused(&handle, &key, "Blocked title", first_track, cover, |error| {
        matches!(error, crate::ImportError::CandidateImportInProgress)
    })
    .await;

    let after = crate::preparations::stored_preparation(&handle.tables, &hash)
        .await
        .unwrap()
        .expect("the candidate remains prepared");
    assert_eq!(after, before);
    shut_down(handle).await;
}

/// One spreadsheet fill writes the same artist choice onto every named row
/// while preserving each row's title and audio mapping.
#[tokio::test(flavor = "multi_thread")]
async fn track_artist_assignments_fill_across_named_rows() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    let before = track_rows(&pane(&handle, &key).await.draft().unwrap().mapping);
    let target_ids = before.iter().map(|track| track.id.clone()).collect();
    let assignments = crate::TrackArtistAssignments::Explicit(vec![crate::Credited::plain(
        crate::ArtistAssignment::named("Filled Artist"),
    )]);

    handle.set_candidate_track_artists(&key, target_ids, assignments.clone()).await.unwrap();

    let after = track_rows(&pane(&handle, &key).await.draft().unwrap().mapping);
    assert_eq!(after.len(), before.len());
    for (before, after) in before.iter().zip(after.iter()) {
        assert_eq!(after.artist_assignments, assignments);
        assert_eq!(after.title, before.title);
        assert_eq!(after.file, before.file);
    }

    shut_down(handle).await;
}

/// A cover is one of the folder's images or the artwork its tags hold, as its
/// scan stored them: a file the folder does not hold, a file that is not one
/// of its images, or tag artwork its tags do not hold is refused to whoever
/// chose it, and the candidate keeps the cover it had.
#[tokio::test(flavor = "multi_thread")]
async fn a_cover_the_scan_did_not_store_is_refused() {
    let (handle, _tmp, key, hash) = pane_fixture().await;
    let before = crate::preparations::stored_preparation(&handle.tables, &hash)
        .await
        .unwrap()
        .expect("the candidate is prepared");

    for (cover, refusal) in [
        (
            crate::CoverSelection::Local("gone.jpg".to_string()),
            crate::CoverRefusal::NotInFolder { file_id: "gone.jpg".to_string() },
        ),
        (
            crate::CoverSelection::Local("01 Track.flac".to_string()),
            crate::CoverRefusal::NotAnImage { file_id: "01 Track.flac".to_string() },
        ),
        (
            crate::CoverSelection::Embedded("01 Track.flac".to_string()),
            crate::CoverRefusal::NoTagArtwork { source_file_id: "01 Track.flac".to_string() },
        ),
    ] {
        let error = handle.set_candidate_cover(&key, cover.clone()).await.expect_err("refused");
        assert!(
            matches!(&error, crate::ImportError::Cover(refused) if *refused == refusal),
            "{cover:?}: {error:?}"
        );
    }

    let after = crate::preparations::stored_preparation(&handle.tables, &hash)
        .await
        .unwrap()
        .expect("the candidate remains prepared");
    shut_down(handle).await;
    assert_eq!(after, before);
}
