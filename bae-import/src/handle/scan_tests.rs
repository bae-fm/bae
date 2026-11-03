//! The draft's metadata source: reading the files' tags, picking a release,
//! clearing the draft, keeping it over a lookup, and unlinking — what each
//! replaces, what it keeps, and the link it leaves. A draft write leaves
//! identification's result standing and starts no run.

use super::*;
use crate::handle::tests::*;
use crate::identification::tests::*;
use crate::{CandidateEditField, MetadataAuthor, NeedsYouReason, PendingStanding};
use bae_catalogs::fake_catalogs::FakeCatalogs;
use bae_catalogs::test_releases::mb_release_json;
use bae_catalogs::test_releases::seed_discogs_pressing;
use bae_catalogs::test_releases::seed_discogs_pressing_printed_with;
use bae_catalogs::test_releases::seed_mb_release_with_catalog;

/// Store a verdict that found nothing for the fixture's candidate.
async fn store_not_found(handle: &ImportServiceHandle, hash: &str) {
    let prep = crate::preparation::stored_preparation(&handle.tables, hash)
        .await
        .unwrap()
        .expect("the fixture candidate is prepared");
    let stored = handle
        .preparations
        .store_verdict(&crate::tables::NewImportCandidateVerdict {
            content_hash: hash.to_string(),
            file_edit_revision: prep.file_edits.revision,
            candidate_key: crate::CandidateKey::from_bytes(Vec::new()),
            verdict: bae_identify::TerminalVerdict::NotFoundAnywhere { ledger: None },
            signals: bae_identify::signals::Signals {
                origin: bae_catalogs::AudioOrigin::default(),
                disc_id: bae_identify::signals::DiscIdSignal::Absent,
                barcode: bae_identify::signals::BarcodeSignal::Absent,
                text: bae_identify::signals::TextSignal::Settled {
                    catalogs: Vec::new(),
                    free_text: Vec::new(),
                },
                text_pool: Vec::new(),
                isrcs: Vec::new(),
                track_titles: Vec::new(),
            },
            pick: None,
            releases: Vec::new(),
        })
        .await
        .unwrap();
    assert!(stored, "the verdict lands on the fixture candidate");
}

/// Keep my info leaves the draft as it was and makes the folder Unmatched.
#[tokio::test(flavor = "multi_thread")]
async fn keeping_the_draft_makes_the_folder_unmatched() {
    let (handle, _tmp, key, hash) = pane_fixture().await;
    store_not_found(&handle, &hash).await;
    assert_eq!(
        standing(&handle, &key).await,
        PendingStanding::NeedsYou { reason: NeedsYouReason::NotFound }
    );
    let before = pane(&handle, &key).await.draft().unwrap().metadata_draft.clone();

    handle.keep_candidate_draft(key.clone()).await.unwrap();

    assert_eq!(standing(&handle, &key).await, PendingStanding::Unmatched);
    assert_eq!(pane(&handle, &key).await.draft().unwrap().metadata_draft, before);
    shut_down(handle).await;
}

/// A new verdict replaces the one the decision answered, so the decision
/// goes with it.
#[tokio::test(flavor = "multi_thread")]
async fn a_new_verdict_starts_without_the_kept_draft() {
    let (handle, _tmp, key, hash) = pane_fixture().await;
    store_not_found(&handle, &hash).await;
    handle.keep_candidate_draft(key.clone()).await.unwrap();
    assert_eq!(standing(&handle, &key).await, PendingStanding::Unmatched);

    store_not_found(&handle, &hash).await;

    assert_eq!(
        standing(&handle, &key).await,
        PendingStanding::NeedsYou { reason: NeedsYouReason::NotFound }
    );
    shut_down(handle).await;
}

/// With no lookup stored there is nothing to keep the draft over.
#[tokio::test(flavor = "multi_thread")]
async fn keeping_the_draft_with_no_lookup_is_refused() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    assert!(handle.keep_candidate_draft(key.clone()).await.is_err());
    assert_eq!(standing(&handle, &key).await, PendingStanding::NotLookedUp);
    shut_down(handle).await;
}

/// Picking a release after keeping the draft answers the folder with it:
/// Identified, and the kept draft stands no more.
#[tokio::test(flavor = "multi_thread")]
async fn a_pick_after_keeping_the_draft_is_identified() {
    let fakes = FakeCatalogs::offline();
    let (handle, _tmp, key, hash) = pane_fixture_with(fakes.providers()).await;
    let tables = handle.tables.clone();
    store_not_found(&handle, &hash).await;
    handle.keep_candidate_draft(key.clone()).await.unwrap();
    seed_mb_release_with_catalog(&fakes, "kept-then-picked", "CAT-1");

    pick(&handle, &key, "kept-then-picked").await;

    assert_eq!(standing(&handle, &key).await, PendingStanding::Identified);
    let kept = tables
        .load_import_candidate_state(&hash)
        .await
        .unwrap()
        .and_then(|state| state.identify)
        .expect("the verdict stands")
        .kept_own_draft;
    assert!(!kept, "the pick answers the folder over the kept draft");
    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn resetting_to_the_tags_drops_what_was_typed() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    let from_tags = pane(&handle, &key).await.draft().unwrap().metadata_draft.clone();

    handle
        .set_candidate_edit_field(
            &key,
            crate::DraftFieldEdit::Text {
                field: CandidateEditField::AlbumTitle,
                value: "Typed Title".to_string(),
            },
        )
        .await
        .unwrap();
    handle
        .set_candidate_edit_field(
            &key,
            crate::DraftFieldEdit::Labels(vec![crate::RawLabelEdit {
                name: "Typed Label".to_string(),
                catalog_number: String::new(),
                ..Default::default()
            }]),
        )
        .await
        .unwrap();
    handle.select_candidate_file_tags(key.clone()).await.unwrap();

    let reset = pane(&handle, &key).await;
    shut_down(handle).await;

    assert_eq!(
        reset.draft().unwrap().metadata_draft.album_title,
        from_tags.album_title,
        "the title the tags state replaces the one that was typed over it"
    );
    assert_eq!(
        reset.draft().unwrap().metadata_draft.pressing.labels,
        from_tags.pressing.labels,
        "a field the tags leave blank goes back to blank"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pick_replaces_typed_fields_with_the_catalog_metadata() {
    let fakes = FakeCatalogs::offline();
    let (handle, _tmp, key, _hash) = pane_fixture_with(fakes.providers()).await;
    handle
        .catalogs
        .store_discogs_key("test-discogs-token", bae_preferences::DiscogsValidation::Valid)
        .await
        .unwrap();
    handle
        .set_candidate_edit_field(
            &key,
            crate::DraftFieldEdit::Barcodes(vec!["5099749".to_string()]),
        )
        .await
        .unwrap();

    let release_id = "70000101";
    seed_discogs_pressing(&fakes, release_id, "Label Name", "CAT-1");
    handle
        .select_candidate_release(
            key.clone(),
            bae_identify::PressingLink {
                record: bae_catalogs::records::LookupRef::new(
                    bae_catalogs::catalog::LookupCatalog::Discogs,
                    release_id.to_string(),
                ),
                partners: vec![],
            },
            crate::CatalogApplySteps::unread(),
        )
        .await
        .unwrap();
    let pane = pane(&handle, &key).await;
    shut_down(handle).await;

    assert_eq!(
        pane.draft().unwrap().metadata_draft.pressing.labels,
        vec![crate::RawLabelEdit {
            name: "Label Name".to_string(),
            catalog_number: "CAT-1".to_string(),
            ..Default::default()
        }]
    );
    assert_eq!(
        pane.draft().unwrap().metadata_draft.pressing.barcodes,
        Vec::<String>::new(),
        "explicit application replaces typed values, including with absent metadata"
    );
}

/// A Discogs release printed with an EAN and a UPC keeps both: applying it
/// writes both barcode rows into the draft, and the edit the import writes
/// states both.
#[tokio::test(flavor = "multi_thread")]
async fn applying_a_release_with_two_barcodes_keeps_both() {
    let fakes = FakeCatalogs::offline();
    let (handle, _tmp, key, _hash) = pane_fixture_with(fakes.providers()).await;
    handle
        .catalogs
        .store_discogs_key("test-discogs-token", bae_preferences::DiscogsValidation::Valid)
        .await
        .unwrap();
    let release_id = "70000102";
    seed_discogs_pressing_printed_with(
        &fakes,
        release_id,
        "Label Name",
        "CAT-1",
        &["5 012345 678900", "0 12345 67890 5"],
    );
    handle
        .select_candidate_release(
            key.clone(),
            bae_identify::PressingLink {
                record: bae_catalogs::records::LookupRef::new(
                    bae_catalogs::catalog::LookupCatalog::Discogs,
                    release_id.to_string(),
                ),
                partners: vec![],
            },
            crate::CatalogApplySteps::unread(),
        )
        .await
        .unwrap();
    let pane = pane(&handle, &key).await;
    shut_down(handle).await;

    let both = ["5 012345 678900", "0 12345 67890 5"];
    assert_eq!(pane.draft().unwrap().metadata_draft.pressing.barcodes, both);
    let shaped = pane.draft().unwrap().metadata_draft.shape().expect("the applied draft shapes");
    assert_eq!(shaped.pressing.barcodes, both);
    assert_eq!(
        crate::RawPressingEdit::from_pressing(&shaped.pressing).barcodes,
        both,
        "the editor's rows read both back"
    );
}

/// Who wrote the draft travels on the pane's own value, so a surface reading
/// the detail knows whether the pick in front of it is the person's or
/// identification's without inferring it from anything else.
#[tokio::test(flavor = "multi_thread")]
async fn a_pick_the_person_made_names_them_as_the_author() {
    let StoredCandidate { handle, key, tmp: _tmp, .. } = stored_candidate().await;
    assert_eq!(
        pane(&handle, &key).await.draft().unwrap().metadata_author,
        MetadataAuthor::Prefill,
        "a candidate nobody has picked for carries the scan's draft"
    );

    handle.select_candidate_file_tags(key.clone()).await.unwrap();

    assert_eq!(pane(&handle, &key).await.draft().unwrap().metadata_author, MetadataAuthor::Person);
    shut_down(handle).await;
}

/// Clearing the draft is the person's act on it like a pick is: the blank
/// draft it leaves is theirs, not a draft nobody has touched.
#[tokio::test(flavor = "multi_thread")]
async fn clearing_the_draft_leaves_the_person_its_author() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    assert_eq!(pane(&handle, &key).await.draft().unwrap().metadata_author, MetadataAuthor::Person);

    handle.clear_candidate_metadata(key.clone()).await.unwrap();

    let cleared = pane(&handle, &key).await;
    assert_eq!(cleared.draft().unwrap().metadata_author, MetadataAuthor::Person);
    assert_eq!(cleared.draft().unwrap().metadata_provenance, None);
    shut_down(handle).await;
}

/// Applying and clearing metadata replaces what the rows say, never which
/// audio they play or which row they are.
#[tokio::test(flavor = "multi_thread")]
async fn metadata_source_changes_keep_each_row_on_its_audio() {
    let (handle, _tmp, key, hash) = pane_fixture().await;
    let rows = |draft: &crate::CandidateDraft| {
        draft
            .tracks
            .iter()
            .map(|track| (track.edit.id.clone(), track.edit.file.clone()))
            .collect::<Vec<_>>()
    };
    let before = rows(
        &crate::preparation::stored_preparation(&handle.tables, &hash)
            .await
            .unwrap()
            .unwrap()
            .metadata
            .draft,
    );

    handle.select_candidate_file_tags(key.clone()).await.unwrap();
    let reapplied = crate::preparation::stored_preparation(&handle.tables, &hash)
        .await
        .unwrap()
        .expect("the reapplied source remains prepared");
    assert_eq!(rows(&reapplied.metadata.draft), before);

    handle.clear_candidate_metadata(key).await.unwrap();
    let cleared = crate::preparation::stored_preparation(&handle.tables, &hash)
        .await
        .unwrap()
        .expect("the cleared source remains prepared");
    assert_eq!(rows(&cleared.metadata.draft), before);
    shut_down(handle).await;
}

/// Folder artwork remains the effective cover when file metadata has no embedded
/// image to select.
#[tokio::test(flavor = "multi_thread")]
async fn file_tags_uses_the_conventional_folder_cover() {
    let StoredCandidate { handle, candidate, key, tmp: _tmp, .. } = stored_candidate().await;
    handle.select_candidate_file_tags(key.clone()).await.unwrap();
    let cover = pane(&handle, &key)
        .await
        .draft()
        .unwrap()
        .cover
        .clone()
        .expect("file metadata applies its deterministic default cover");
    assert_eq!(cover.selection, crate::CoverSelection::Local("cover.jpg".to_string()));
    let scanned = candidate
        .files
        .files
        .iter()
        .find(|entry| entry.file.relative_path == "cover.jpg")
        .expect("the fixture folder holds cover.jpg");
    assert_eq!(
        cover.image,
        crate::cover_art::CoverImageSource::Local { file: scanned.file.version() },
        "a folder image is drawn from disk at the version the scan read"
    );

    shut_down(handle).await;
}

/// A folder image named as the front cover ranks ahead of the tags' embedded
/// artwork when file tags are applied, and the selection persists.
#[tokio::test(flavor = "multi_thread")]
async fn file_tags_persist_the_front_cover_image_ahead_of_embedded_artwork() {
    let StoredCandidate { mut handle, parts, key, tmp: _tmp, .. } = stored_candidate().await;
    handle.file_tags =
        std::sync::Arc::new(CountingFileTagReader::with_embedded_cover(vec![1, 2, 3, 4]));
    handle.select_candidate_file_tags(key.clone()).await.unwrap();

    let cover = pane(&handle, &key)
        .await
        .draft()
        .unwrap()
        .cover
        .clone()
        .expect("applying tags that embed artwork selects a cover");
    assert_eq!(cover.selection, crate::CoverSelection::Local("cover.jpg".to_string()));
    shut_down(handle).await;

    let reopened = parts.start().await.unwrap();
    assert_eq!(
        pane(&reopened, &key)
            .await
            .draft()
            .unwrap()
            .cover
            .clone()
            .expect("the persisted selection survives a relaunch")
            .selection,
        crate::CoverSelection::Local("cover.jpg".to_string())
    );
    shut_down(reopened).await;
}

fn linked_to(release_id: &str) -> crate::ReleaseLink {
    crate::ReleaseLink::Pressing(bae_identify::PressingLink {
        record: bae_catalogs::records::LookupRef::new(
            bae_catalogs::catalog::LookupCatalog::MusicBrainz,
            release_id.to_string(),
        ),
        partners: vec![],
    })
}

/// Reading the draft from the files' tags after a pick replaces the draft
/// and leaves the candidate linked to the release it picked.
#[tokio::test(flavor = "multi_thread")]
async fn reading_the_tags_after_a_pick_keeps_the_link() {
    let (handle, _tmp, key) = picked_fixture("linked-then-tags").await;

    handle.select_candidate_file_tags(key.clone()).await.unwrap();

    let after = pane(&handle, &key).await;
    assert_eq!(
        after.draft().unwrap().metadata_provenance,
        Some(crate::MetadataProvenance::FileMetadata),
        "the draft is the files' tags now"
    );
    assert_eq!(after.draft().unwrap().release_link, Some(linked_to("linked-then-tags")));
    assert_eq!(standing(&handle, &key).await, PendingStanding::Identified);
    shut_down(handle).await;
}

/// Unlinking clears the link alone: the draft read from the release stays
/// as it is, and the folder is unmatched.
#[tokio::test(flavor = "multi_thread")]
async fn unlinking_keeps_the_draft_and_unmatches_the_folder() {
    let (handle, _tmp, key) = picked_fixture("linked-then-unlinked").await;
    let before = pane(&handle, &key).await;

    handle.unlink_candidate_release(key.clone()).await.unwrap();

    let after = pane(&handle, &key).await;
    assert_eq!(after.draft().unwrap().release_link, None);
    assert_eq!(after.draft().unwrap().metadata_draft, before.draft().unwrap().metadata_draft);
    assert_eq!(
        after.draft().unwrap().metadata_provenance,
        before.draft().unwrap().metadata_provenance
    );
    assert_eq!(standing(&handle, &key).await, PendingStanding::Unmatched);
    shut_down(handle).await;
}

/// Typing into the draft and clearing it are draft writes, which never touch
/// the link.
#[tokio::test(flavor = "multi_thread")]
async fn editing_or_clearing_the_draft_keeps_the_link() {
    let (handle, _tmp, key) = picked_fixture("linked-then-edited").await;

    handle
        .set_candidate_edit_field(
            &key,
            crate::DraftFieldEdit::Text {
                field: CandidateEditField::AlbumTitle,
                value: "Typed Title".to_string(),
            },
        )
        .await
        .unwrap();
    let edited = pane(&handle, &key).await;
    assert_eq!(edited.draft().unwrap().metadata_draft.album_title, "Typed Title");
    assert_eq!(edited.draft().unwrap().release_link, Some(linked_to("linked-then-edited")));

    handle.clear_candidate_metadata(key.clone()).await.unwrap();
    let cleared = pane(&handle, &key).await;
    assert!(cleared.draft().unwrap().metadata_draft.is_blank());
    assert_eq!(cleared.draft().unwrap().release_link, Some(linked_to("linked-then-edited")));
    assert_eq!(standing(&handle, &key).await, PendingStanding::Identified);
    shut_down(handle).await;
}

/// Clearing the draft, and reading the folder's tags back into it, say
/// nothing about what identification concluded: the result stands, the file
/// revision it is keyed on does not move, and no new run starts.
#[tokio::test(flavor = "multi_thread")]
async fn a_draft_write_leaves_the_result_and_starts_no_run() {
    for name in ["clear-metadata", "reset-to-tags"] {
        let fixture = Fixture::new(name).await;
        let dir = fixture.disc_id_candidate("Candidate");
        let key = crate::CandidateKey::folder(&dir);
        let probed = fixture.probed_total_ms(&dir);
        fixture.provider.route(
            "/discid/",
            200,
            discid_json("mb-draft-write", "rg-draft-write", &[probed, 0]),
        );
        fixture.provider.route(
            "/release/mb-draft-write?",
            200,
            mb_release_json("mb-draft-write", "rg-draft-write", &[probed, 0]),
        );
        fixture.scan(1).await;
        fixture.drain_automatic().await;
        let settled = fixture.stored_for(&dir).await.expect("identification stores a result");
        let asked = fixture.provider.count_containing("/discid/");

        if name == "clear-metadata" {
            fixture.import.clear_candidate_metadata(key).await.unwrap();
        } else {
            fixture.import.select_candidate_file_tags(key).await.unwrap();
        }

        let after = fixture.stored_for(&dir).await.expect("the candidate is still stored");
        assert_eq!(after.identify, settled.identify, "{name} left the result alone");
        assert_eq!(
            after.file_edits.revision, settled.file_edits.revision,
            "{name} did not move the file revision the result is keyed on"
        );

        fixture.drain_automatic().await;

        assert_eq!(fixture.provider.count_containing("/discid/"), asked, "{name} started no run");
    }
}
