//! What each write of a candidate's preparation keeps, replaces and refuses.

use super::*;
use crate::cover_art::PreparedCover;
use crate::folder_scanner::{CandidateFileEdits, SheetDisc};
use crate::tables::import_state::tests::*;
use crate::tables::tests::*;
use crate::tables::watched_path::host_root;
use crate::tables::NewImportCandidateVerdict;
use crate::CandidateEditField;
use crate::CoverSelection;
use bae_catalogs::catalog::Catalog;
use bae_db::test_fixtures::empty_db;
use bae_identify::TerminalVerdict;

fn image(byte: u8) -> bae_catalogs::remote_image::RemoteImage {
    bae_catalogs::remote_image::RemoteImage {
        bytes: vec![byte],
        content_type: bae_util::content_type::ContentType::Jpeg,
    }
}

/// The cover `supplied` and `stored` settle on, as a metadata application
/// that brings `supplied` over a candidate holding `stored` leaves it.
fn settled(
    supplied: Option<PreparedCover>,
    stored: Option<PreparedCover>,
) -> Option<PreparedCover> {
    let metadata = |cover| crate::CandidateMetadataDraft {
        draft: candidate_draft("Album", "Artist"),
        provenance: None,
        cover,
    };
    let (mut supplied, mut stored) = (metadata(supplied), metadata(stored));
    settle_cover(&mut supplied, &mut stored);
    supplied.cover
}

fn remote_cover(url: &str, byte: u8) -> PreparedCover {
    PreparedCover::Remote {
        image: bae_catalogs::remote_image::RemoteImageSet::original(url.to_string()),
        source: Catalog::Discogs,
        fetched: image(byte),
    }
}

/// A source that brings no image of its own says nothing about the
/// cover, so the stored one stands — a remote one with the bytes it was
/// fetched with, which is what the candidate commits with.
#[test]
fn a_source_with_no_image_keeps_the_stored_cover_and_its_bytes() {
    let folders_own = PreparedCover::Local("cover.jpg".to_string());
    let remote = remote_cover("https://example.invalid/front", 1);

    assert_eq!(settled(None, Some(folders_own.clone())), Some(folders_own.clone()));
    assert_eq!(settled(None, Some(remote.clone())), Some(remote));
    assert_eq!(settled(None, None), None);

    let replacing = remote_cover("https://example.invalid/other", 2);
    assert_eq!(settled(Some(replacing.clone()), Some(folders_own)), Some(replacing));
}

#[tokio::test]
async fn a_verdict_cannot_create_state_for_an_absent_candidate() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let hash = pane_candidate().content_hash();

    assert!(!store_verdict_with_signals(&db, &hash, settled_signals()).await);
    assert!(tables.load_import_candidate_state(&hash).await.unwrap().is_none());
}

// Who wrote the candidate's draft, as the row reads it back: the author stored
// on the draft row, and `Nobody` for the blank draft discovery created.

/// A run that settled on one pressing writes the pick, so the row says
/// identification wrote it.
#[tokio::test]
async fn a_verdict_that_picks_names_identification_as_the_author() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    fetched(&db, "rel-1").await;
    let candidate =
        track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
    let hash = candidate.content_hash();
    let row = concluding(
        new_candidate_row(&hash, &host_root("/music/Some Album"), &sample_verdict()),
        "rel-1",
    );
    store_candidate_state(&db, &candidate, &row.candidate_key.test_folder()).await;

    crate::CandidatePreparations::new(db.clone()).store_verdict(&row).await.unwrap();

    let loaded = tables.load_import_candidate_states().await.unwrap();
    assert_eq!(
        loaded.get(&hash).expect("the candidate reads back").metadata_author,
        crate::MetadataAuthor::Identification
    );
}

/// A verdict that settled on nothing to pick leaves the draft to whoever wrote
/// it — here nobody.
#[tokio::test]
async fn a_verdict_that_picks_nothing_leaves_the_draft_unclaimed() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let candidate =
        track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
    let hash = candidate.content_hash();
    let row = new_candidate_row(&hash, &host_root("/music/Some Album"), &sample_verdict());
    assert!(row.pick.is_none(), "it settled on no release to write");
    store_candidate_state(&db, &candidate, &row.candidate_key.test_folder()).await;

    crate::CandidatePreparations::new(db.clone()).store_verdict(&row).await.unwrap();

    let loaded = tables.load_import_candidate_states().await.unwrap();
    assert_eq!(
        loaded.get(&hash).expect("the candidate reads back").metadata_author,
        crate::MetadataAuthor::Nobody
    );
}

/// A verdict revises the draft it lands on whoever wrote it, so a person's
/// pick and the fields they typed over it are replaced by the release the run
/// settled on.
#[tokio::test]
async fn a_verdict_replaces_a_person_s_pick_and_their_edits() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    fetched(&db, "rel-chosen").await;
    fetched(&db, "rel-1").await;
    let (_, hash) = stored_pane_candidate(&db).await;
    crate::CandidatePreparations::new(db.clone())
        .replace_metadata(
            &hash,
            &pane_candidate_key(),
            &metadata_draft("Album", "Artist"),
            Some(&release_pick("rel-chosen")),
            &[],
        )
        .await
        .unwrap();
    PaneControls::of_pane_candidate(&db)
        .type_field(&hash, CandidateEditField::PressingYear, "1991")
        .await
        .unwrap();

    assert!(crate::CandidatePreparations::new(db.clone())
        .store_verdict(&NewImportCandidateVerdict {
            content_hash: hash.to_string(),
            file_edit_revision: 0,
            candidate_key: pane_candidate_key(),
            verdict: sample_verdict(),
            signals: settled_signals(),
            pick: Some(crate::tables::VerdictPick {
                link: release_link("rel-1"),
                metadata: crate::CandidateMetadataDraft {
                    draft: candidate_draft("Different album", "Different Artist"),
                    provenance: Some(release_pick("rel-1")),
                    cover: None,
                },
            }),
            releases: Vec::new(),
        })
        .await
        .unwrap());

    let state = tables.load_import_candidate_state(&hash).await.unwrap().unwrap();
    assert_eq!(state.metadata_provenance, Some(release_pick("rel-1")));
    assert_eq!(
        tables.load_import_candidate_pane_rows(&hash).await.unwrap().draft.album_title,
        "Different album"
    );
}

/// A verdict written after a field edit replaces the edit: the later write
/// wins.
#[tokio::test]
async fn a_verdict_after_a_metadata_edit_replaces_it() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    fetched(&db, "rel-first").await;
    fetched(&db, "rel-second").await;
    let (_, hash) = stored_pane_candidate(&db).await;
    let first_pick = release_pick("rel-first");
    assert!(crate::CandidatePreparations::new(db.clone())
        .store_verdict(&NewImportCandidateVerdict {
            content_hash: hash.to_string(),
            file_edit_revision: 0,
            candidate_key: pane_candidate_key(),
            verdict: sample_verdict(),
            signals: settled_signals(),
            pick: Some(crate::tables::VerdictPick {
                link: release_link("rel-first"),
                metadata: crate::CandidateMetadataDraft {
                    draft: candidate_draft("First album", "Artist"),
                    provenance: Some(first_pick.clone()),
                    cover: None,
                },
            }),
            releases: Vec::new(),
        })
        .await
        .unwrap());
    PaneControls::of_pane_candidate(&db)
        .type_field(&hash, CandidateEditField::AlbumTitle, "Person's title")
        .await
        .unwrap();

    assert!(crate::CandidatePreparations::new(db.clone())
        .store_verdict(&NewImportCandidateVerdict {
            content_hash: hash.to_string(),
            file_edit_revision: 0,
            candidate_key: pane_candidate_key(),
            verdict: sample_verdict(),
            signals: settled_signals(),
            pick: Some(crate::tables::VerdictPick {
                link: release_link("rel-second"),
                metadata: crate::CandidateMetadataDraft {
                    draft: candidate_draft("Second album", "Different Artist"),
                    provenance: Some(release_pick("rel-second")),
                    cover: None,
                },
            }),
            releases: Vec::new(),
        })
        .await
        .unwrap());

    let state = tables.load_import_candidate_state(&hash).await.unwrap().unwrap();
    assert_eq!(state.metadata_revision, 3);
    assert_eq!(state.metadata_provenance, Some(release_pick("rel-second")));
    assert_eq!(
        tables.load_import_candidate_pane_rows(&hash).await.unwrap().draft.album_title,
        "Second album"
    );
}

fn found_nothing() -> TerminalVerdict {
    TerminalVerdict::NotFoundAnywhere { ledger: None }
}

/// A re-run that finds nothing leaves the draft an earlier run wrote.
#[tokio::test]
async fn a_re_run_that_finds_nothing_leaves_the_draft_an_earlier_run_wrote() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    fetched(&db, "mb-rel-1").await;
    let candidate = track_files_candidate(&[("01 Track.flac", 123_456)]);
    let hash = store_candidate_state(&db, &candidate, &host_root("/music/Album")).await;

    let settled = concluding(
        new_candidate_row(&hash, &host_root("/music/Album"), &sample_verdict()),
        "mb-rel-1",
    );
    crate::CandidatePreparations::new(db.clone()).store_verdict(&settled).await.unwrap();

    let re_run = new_candidate_row(&hash, &host_root("/music/Album"), &found_nothing());
    assert!(re_run.pick.is_none(), "nothing was found to pick");
    crate::CandidatePreparations::new(db.clone()).store_verdict(&re_run).await.unwrap();

    let loaded = tables.load_import_candidate_states().await.unwrap();
    assert_eq!(
        loaded.get(&hash).expect("the row is still there").metadata_provenance,
        Some(release_pick("mb-rel-1")),
        "a run that concluded no release wrote no draft"
    );
}

/// A re-run that finds nothing leaves a person's pick in place.
#[tokio::test]
async fn a_re_run_that_finds_nothing_leaves_a_person_s_pick_alone() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    fetched(&db, "mb-rel-chosen").await;
    let candidate = track_files_candidate(&[("01 Track.flac", 123_456)]);
    let hash = store_candidate_state(&db, &candidate, &host_root("/music/Album")).await;

    let initial = new_candidate_row(&hash, &host_root("/music/Album"), &found_nothing());
    crate::CandidatePreparations::new(db.clone()).store_verdict(&initial).await.unwrap();
    crate::CandidatePreparations::new(db.clone())
        .replace_metadata(
            &hash,
            &crate::CandidateKey::folder(&host_root("/music/Album")),
            &metadata_draft("Album", "Artist"),
            Some(&release_pick("mb-rel-chosen")),
            &[],
        )
        .await
        .unwrap();

    let re_run = new_candidate_row(&hash, &host_root("/music/Album"), &found_nothing());
    crate::CandidatePreparations::new(db.clone()).store_verdict(&re_run).await.unwrap();

    let loaded = tables.load_import_candidate_states().await.unwrap();
    assert_eq!(
        loaded.get(&hash).expect("the row is still there").metadata_provenance,
        Some(release_pick("mb-rel-chosen")),
        "a verdict that concluded no release must not unmake a choice a person made"
    );
}

/// A re-run that settles on a different release replaces the earlier run's
/// pick.
#[tokio::test]
async fn a_re_run_that_settles_elsewhere_replaces_the_pick_it_made() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    fetched(&db, "mb-rel-first").await;
    fetched(&db, "mb-rel-second").await;
    let candidate = track_files_candidate(&[("01 Track.flac", 123_456)]);
    let hash = store_candidate_state(&db, &candidate, &host_root("/music/Album")).await;

    let first = concluding(
        new_candidate_row(&hash, &host_root("/music/Album"), &sample_verdict()),
        "mb-rel-first",
    );
    crate::CandidatePreparations::new(db.clone()).store_verdict(&first).await.unwrap();

    let second = concluding(
        new_candidate_row(&hash, &host_root("/music/Album"), &sample_verdict()),
        "mb-rel-second",
    );
    crate::CandidatePreparations::new(db.clone()).store_verdict(&second).await.unwrap();

    let loaded = tables.load_import_candidate_states().await.unwrap();
    assert_eq!(
        loaded.get(&hash).expect("the row is still there").metadata_provenance,
        Some(release_pick("mb-rel-second")),
        "the live verdict's own conclusion is the one that stands"
    );
}

#[tokio::test]
async fn stale_file_revision_cannot_replace_prepared_metadata() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let (files, hash) = stored_pane_candidate(&db).await;
    let stale = tables
        .load_candidate_preparation(&hash)
        .await
        .unwrap()
        .and_then(crate::tables::CandidatePreparationRead::stored)
        .expect("the scanned candidate is prepared");
    crate::CandidatePreparations::new(db.clone())
        .store_file_decisions(
            &crate::CandidateAsRead {
                content_hash: hash.clone(),
                file_edit_revision: stale.file_edits.revision,
                metadata_revision: stale.metadata_revision,
            },
            &pane_candidate_key(),
            &CandidateFileEdits::default(),
            &[(pane_candidate_key(), files)],
            stale.metadata.draft.clone(),
        )
        .await
        .unwrap();

    let error = crate::CandidatePreparations::new(db.clone())
        .apply_source(
            &host_root("/music"),
            &crate::CandidateAsRead {
                content_hash: hash.clone(),
                file_edit_revision: stale.file_edits.revision,
                metadata_revision: stale.metadata_revision,
            },
            &pane_candidate_key(),
            &crate::CandidateMetadataDraft {
                draft: stale.metadata.draft,
                provenance: stale.metadata.provenance,
                cover: stale.metadata.cover,
            },
        )
        .await
        .expect_err("metadata prepared for the prior files is stale");
    assert!(error.to_string().contains("candidate changed"), "{error}");
}

#[tokio::test]
async fn metadata_replacement_refuses_a_candidate_key_that_now_names_other_files() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let (_, hash) = stored_pane_candidate(&db).await;
    let stale = tables
        .load_candidate_preparation(&hash)
        .await
        .unwrap()
        .and_then(crate::tables::CandidatePreparationRead::stored)
        .expect("the scanned candidate is prepared");
    let root = host_root("/music");
    let mut replacement = scanned_candidate(&root, "Album");
    let crate::folder_scanner::ScanItem::Valid(candidate) = &mut replacement else {
        unreachable!("the fixture creates a valid candidate");
    };
    candidate.files.files[0].file.version.size += 1;
    let generation = tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    tables
        .save_folder_scan_item(&root, generation, &replacement)
        .await
        .unwrap()
        .expect("the current scan accepts the replacement");
    tables.finish_folder_scan(&root, generation, None).await.unwrap();

    let error = crate::CandidatePreparations::new(db.clone())
        .apply_source(
            &root,
            &crate::CandidateAsRead {
                content_hash: hash.clone(),
                file_edit_revision: stale.file_edits.revision,
                metadata_revision: stale.metadata_revision,
            },
            &pane_candidate_key(),
            &crate::CandidateMetadataDraft {
                draft: stale.metadata.draft,
                provenance: stale.metadata.provenance,
                cover: stale.metadata.cover,
            },
        )
        .await
        .expect_err("the candidate key no longer names the prepared files");
    assert!(error.to_string().contains("candidate changed"), "{error}");
}

/// Applying or clearing metadata replaces the whole draft and removes artist
/// assignments owned by the prior source, while every physical decision stays.
#[tokio::test]
async fn metadata_apply_and_clear_preserve_every_physical_decision() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    fetched(&db, "rel-1").await;
    fetched(&db, "rel-2").await;
    let (files, hash) = stored_pane_candidate(&db).await;
    let old_draft = metadata_draft("Old album", "Replacement Artist");
    crate::CandidatePreparations::new(db.clone())
        .replace_metadata(
            &hash,
            &pane_candidate_key(),
            &old_draft,
            Some(&release_pick("rel-1")),
            &[],
        )
        .await
        .unwrap();
    let mut file_edits = CandidateFileEdits::default();
    file_edits.sheet_discs.set("CDImage.cue".to_string(), SheetDisc::Ignored);
    let mut settled = files;
    settled.apply_candidate_file_edits(&file_edits).unwrap();
    let (metadata_revision, draft) = current_draft(&db, &hash).await;
    crate::CandidatePreparations::new(db.clone())
        .store_file_decisions(
            &as_read(&hash, metadata_revision),
            &pane_candidate_key(),
            &file_edits,
            &[(pane_candidate_key(), settled)],
            draft,
        )
        .await
        .unwrap();
    PaneControls::of_pane_candidate(&db)
        .choose_cover(&hash, &crate::cover_art::PreparedCover::Local("cover.jpg".to_string()))
        .await
        .unwrap();
    let mapping = edited_row(
        "candidate-track-0",
        "Track Title",
        Some(
            tables.load_import_candidate_pane_rows(&hash).await.unwrap().draft.tracks[0]
                .edit
                .file
                .clone(),
        ),
    );
    PaneControls::of_pane_candidate(&db).edit_track(&hash, &mapping).await.unwrap();

    let new_draft = metadata_draft("New album", "New Artist");
    let applied_revision = crate::CandidatePreparations::new(db.clone())
        .replace_metadata(
            &hash,
            &pane_candidate_key(),
            &new_draft,
            Some(&release_pick("rel-2")),
            &[],
        )
        .await
        .unwrap();
    let applied = tables.load_import_candidate_pane_rows(&hash).await.unwrap();
    let mut expected = new_draft;
    expected.tracks[0].file = mapping.file.clone();
    assert_eq!(
        applied.draft.release_edit(),
        expected,
        "the new draft lands and the person's file choice survives it"
    );
    assert_eq!(applied_revision, 4);
    assert_eq!(
        applied.cover,
        Some(CoverSelection::Local("cover.jpg".to_string())),
        "the source brought no image of its own, so the folder's cover stands"
    );
    assert_eq!(
        tables.load_import_candidate_state(&hash).await.unwrap().unwrap().file_edits.sheet_discs,
        file_edits.sheet_discs
    );

    let chosen = PreparedCover::Remote {
        image: bae_catalogs::remote_image::RemoteImageSet::original(
            "https://example.invalid/cover".to_string(),
        ),
        source: Catalog::MusicBrainz,
        fetched: image(1),
    };
    PaneControls::of_pane_candidate(&db).choose_cover(&hash, &chosen).await.unwrap();
    let mut blank = crate::pane::blank_candidate_draft(&pane_candidate()).release_edit();
    blank.tracks.truncate(1);
    let cleared_revision = crate::CandidatePreparations::new(db.clone())
        .replace_metadata(&hash, &pane_candidate_key(), &blank, None, &[])
        .await
        .unwrap();
    let cleared = tables.load_import_candidate_pane_rows(&hash).await.unwrap();
    assert!(cleared.draft.release_edit().is_blank());
    assert_eq!(Some(cleared.draft.tracks[0].edit.file.clone()), mapping.file);
    assert_eq!(cleared_revision, 6);
    assert_eq!(
        cleared.cover,
        Some(chosen.selection()),
        "a draft that brings no image of its own says nothing about the cover, \
         so the one the person chose stands"
    );
    assert_eq!(
        crate::preparations::stored_preparation(&tables, &hash)
            .await
            .unwrap()
            .expect("the candidate is prepared")
            .metadata
            .cover,
        Some(chosen),
        "the chosen cover keeps the bytes fetched with it"
    );
}

#[tokio::test]
async fn stale_metadata_revision_cannot_replace_prepared_file_mappings() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let (files, hash) = stored_pane_candidate(&db).await;
    assert!(store_verdict_with_signals(&db, &hash, settled_signals(),).await);
    let stale = tables
        .load_candidate_preparation(&hash)
        .await
        .unwrap()
        .and_then(crate::tables::CandidatePreparationRead::stored)
        .expect("the candidate is prepared");
    PaneControls::of_pane_candidate(&db)
        .type_field(&hash, CandidateEditField::PressingYear, "1991")
        .await
        .unwrap();

    let mut edits = CandidateFileEdits::default();
    edits.sheet_discs.set("CDImage.cue".to_string(), SheetDisc::Ignored);
    let mut settled = files;
    settled.apply_candidate_file_edits(&edits).unwrap();
    let error = crate::CandidatePreparations::new(db.clone())
        .store_file_decisions(
            &crate::CandidateAsRead {
                content_hash: hash.clone(),
                file_edit_revision: stale.file_edits.revision,
                metadata_revision: stale.metadata_revision,
            },
            &pane_candidate_key(),
            &edits,
            &[(pane_candidate_key(), settled)],
            stale.metadata.draft.clone(),
        )
        .await
        .expect_err("file mappings prepared for the prior metadata are stale");
    assert!(error.to_string().contains("metadata changed"), "{error}");

    let state = tables
        .load_import_candidate_state(&hash)
        .await
        .unwrap()
        .expect("the candidate state remains");
    assert_eq!(state.file_edits, CandidateFileEdits::default());
    assert!(state.identify.is_some(), "the rejected write keeps its verdict");
    let pane = tables.load_import_candidate_pane_rows(&hash).await.unwrap();
    assert_eq!(pane.draft.pressing.year, "1991");
    assert_eq!(pane.draft.tracks, stale.metadata.draft.tracks);
}

/// A file decision reshapes the folder, so the extracted signals go with it. The caller's replacement mappings, metadata,
/// and cover land with the file decision as one candidate state.
#[tokio::test]
async fn a_file_decision_clears_what_the_reshaped_folder_invalidates() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    fetched(&db, "rel-1").await;
    let (files, hash) = stored_pane_candidate(&db).await;
    assert!(store_verdict_with_signals(&db, &hash, settled_signals()).await);
    crate::CandidatePreparations::new(db.clone())
        .replace_metadata(
            &hash,
            &pane_candidate_key(),
            &metadata_draft("Album", "Artist"),
            Some(&release_pick("rel-1")),
            &[],
        )
        .await
        .unwrap();
    PaneControls::of_pane_candidate(&db)
        .type_field(&hash, CandidateEditField::PressingYear, "1991")
        .await
        .unwrap();
    PaneControls::of_pane_candidate(&db)
        .choose_cover(&hash, &crate::cover_art::PreparedCover::Local("cover.jpg".to_string()))
        .await
        .unwrap();
    let preparation = tables
        .load_candidate_preparation(&hash)
        .await
        .unwrap()
        .and_then(crate::tables::CandidatePreparationRead::stored)
        .expect("the candidate has a stored preparation");
    crate::CandidatePreparations::new(db.clone())
        .set_track_edit_prepared(
            &host_root("/music"),
            &pane_candidate_key(),
            &crate::CandidateAsRead {
                content_hash: hash.clone(),
                file_edit_revision: preparation.file_edits.revision,
                metadata_revision: preparation.metadata_revision,
            },
            &edited_row(
                "candidate-track-0",
                "Track Title",
                Some(preparation.metadata.draft.tracks[0].edit.file.clone()),
            ),
        )
        .await
        .unwrap();
    let mut edits = CandidateFileEdits::default();
    edits.sheet_discs.set("CDImage.cue".to_string(), SheetDisc::Ignored);
    let mut settled = files;
    settled.apply_candidate_file_edits(&edits).unwrap();
    let (metadata_revision, draft) = current_draft(&db, &hash).await;
    crate::CandidatePreparations::new(db.clone())
        .store_file_decisions(
            &as_read(&hash, metadata_revision),
            &pane_candidate_key(),
            &edits,
            &[(pane_candidate_key(), settled)],
            draft.clone(),
        )
        .await
        .unwrap();

    let state = tables.load_import_candidate_state(&hash).await.unwrap().unwrap();
    assert!(state.signals.is_none(), "the disc ID is recomputed");

    let pane = tables.load_import_candidate_pane_rows(&hash).await.unwrap();
    assert_eq!(pane.draft, draft, "the prepared replacement draft lands atomically");
    assert_eq!(pane.draft.pressing.year, "1991", "the draft survives");
    assert_eq!(
        pane.cover,
        Some(CoverSelection::Local("cover.jpg".to_string())),
        "and so does its cover"
    );
}

/// A file decision clears the verdict and keeps applied metadata's provenance,
/// whoever applied it.
#[tokio::test]
async fn a_file_decision_preserves_applied_metadata_from_either_author() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    fetched(&db, "mb-rel-derived").await;
    fetched(&db, "mb-rel-chosen").await;
    let candidate = track_files_candidate(&[("01 Track.flac", 123_456)]);
    let hash = candidate.content_hash();
    let edits = crate::folder_scanner::CandidateFileEdits::default();
    store_candidate_state(&db, &candidate, &host_root("/music/Album")).await;

    let settled = concluding(
        new_candidate_row(&hash, &host_root("/music/Album"), &sample_verdict()),
        "mb-rel-derived",
    );
    crate::CandidatePreparations::new(db.clone()).store_verdict(&settled).await.unwrap();
    let (metadata_revision, draft) = current_draft(&db, &hash).await;
    crate::CandidatePreparations::new(db.clone())
        .store_file_decisions(
            &as_read(&hash, metadata_revision),
            &crate::CandidateKey::folder(&host_root("/music/Album")),
            &edits,
            &[(crate::CandidateKey::folder(&host_root("/music/Album")), candidate.clone())],
            draft,
        )
        .await
        .unwrap();
    let loaded = tables.load_import_candidate_states().await.unwrap();
    let row = loaded.get(&hash).expect("the row is still there");
    assert!(row.identify.is_none(), "the decision clears the verdict");
    assert_eq!(
        row.metadata_provenance,
        Some(release_pick("mb-rel-derived")),
        "the applied metadata keeps its source"
    );

    crate::CandidatePreparations::new(db.clone())
        .replace_metadata(
            &hash,
            &crate::CandidateKey::folder(&host_root("/music/Album")),
            &metadata_draft("Album", "Artist"),
            Some(&release_pick("mb-rel-chosen")),
            &[],
        )
        .await
        .unwrap();
    let (metadata_revision, draft) = current_draft(&db, &hash).await;
    crate::CandidatePreparations::new(db.clone())
        .store_file_decisions(
            &crate::CandidateAsRead {
                content_hash: hash.clone(),
                file_edit_revision: 1,
                metadata_revision,
            },
            &crate::CandidateKey::folder(&host_root("/music/Album")),
            &edits,
            &[(crate::CandidateKey::folder(&host_root("/music/Album")), candidate.clone())],
            draft,
        )
        .await
        .unwrap();
    let loaded = tables.load_import_candidate_states().await.unwrap();
    assert_eq!(
        loaded.get(&hash).expect("the row is still there").metadata_provenance,
        Some(release_pick("mb-rel-chosen")),
        "a file decision must not unmake a choice a person made"
    );
}

/// The pair that makes re-identification correct rather than incidental:
/// changing a binding leaves the row's key alone, **and** clears the
/// verdict stored under it.
///
/// The hash covers files and never decisions, so the edit addresses
/// the same row rather than orphaning it — and that row's verdict was
/// derived from the shape the folder no longer has, so the queue must
/// answer the candidate again instead of trusting it.
#[tokio::test]
async fn changing_a_binding_keeps_the_hash_and_clears_the_verdict() {
    use crate::folder_scanner::{
        collect_release_candidate_files_with_scope, CandidateFileEdits, SheetBindingEdits,
        StoredCandidateEdits, UserSheetBinding,
    };

    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let folder = walkthrough_folder();
    let proposed = collect_release_candidate_files_with_scope(
        folder.path(),
        crate::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .unwrap();
    assert_eq!(proposed.track_count(), 12);
    let hash = proposed.content_hash();
    store_candidate_state(&db, &proposed, folder.path()).await;

    crate::CandidatePreparations::new(db.clone())
        .store_verdict(&new_candidate_row(&hash, folder.path(), &sample_verdict()))
        .await
        .unwrap();
    assert!(
        tables
            .load_import_candidate_states()
            .await
            .unwrap()
            .get(&hash)
            .expect("the verdict is stored")
            .identify
            .is_some(),
        "the candidate starts out identified"
    );

    let mut edits = SheetBindingEdits::default();
    edits.set_reference("cd.cue".to_string(), "cd.wav".to_string(), UserSheetBinding::Cleared);
    let mut settled = proposed.clone();
    settled
        .apply_candidate_file_edits(&CandidateFileEdits {
            sheet_bindings: edits.clone(),
            ..Default::default()
        })
        .unwrap();
    let folder_path = crate::CandidateKey::folder(folder.path());
    let (metadata_revision, mut draft) = current_draft(&db, &hash).await;
    draft.tracks = crate::pane::blank_candidate_draft(&settled).tracks;
    crate::CandidatePreparations::new(db.clone())
        .store_file_decisions(
            &as_read(&hash, metadata_revision),
            &folder_path,
            &CandidateFileEdits { sheet_bindings: edits, ..Default::default() },
            &[(folder_path.clone(), settled)],
            draft,
        )
        .await
        .unwrap();

    let unbound = collect_release_candidate_files_with_scope(
        folder.path(),
        crate::ReleaseFileScope::Recursive,
        &tables.load_stored_candidate_edits().await.unwrap(),
    )
    .unwrap();
    assert_eq!(
        unbound.track_count(),
        1,
        "the folder really did change shape -- otherwise this proves nothing"
    );
    assert_eq!(
        unbound.content_hash(),
        hash,
        "the hash covers files, never decisions, so the row stays addressable"
    );

    let row = tables
        .load_import_candidate_states()
        .await
        .unwrap()
        .remove(&hash)
        .expect("the row is still found under the unchanged hash");
    assert!(
        row.identify.is_none(),
        "the stored verdict described the folder before the binding; it must be cleared \
         so the queue identifies the candidate again"
    );
    assert_eq!(
        row.file_edits.sheet_bindings.get("cd.cue").and_then(|references| references.get("cd.wav")),
        Some(&UserSheetBinding::Cleared),
        "the decision that cleared the verdict is what the row now holds"
    );
}
