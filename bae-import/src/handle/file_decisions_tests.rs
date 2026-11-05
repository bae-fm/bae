//! File decisions: ignoring a CUE sheet, clearing one and binding it again, and
//! a sheet's disc read again redraw the draft over the settled audio, keep each
//! retained track's values, and settle every folder holding the same files. A
//! claimed candidate refuses them, and restating one changes nothing.

use super::*;
use crate::handle::tests::*;
use crate::identification::tests::*;
use std::path::PathBuf;

/// Applying a release writes every value into the draft; a file decision
/// after it keeps each retained track's composers, works and stated length,
/// and a track the folder gains has none — no source states anything about
/// it (B2, B5).
#[tokio::test(flavor = "multi_thread")]
async fn a_file_decision_keeps_the_retained_tracks_values_and_gives_a_new_track_none() {
    let (fixture, hash) = three_track_draft().await;
    let applied = stored_draft(&fixture.handle, &hash).await;
    assert_eq!(applied.tracks.len(), 3);
    assert_eq!(
        applied
            .tracks
            .iter()
            .map(|track| (
                track.stated_length_ms,
                track
                    .roles
                    .iter()
                    .map(|role| role.artist.musicbrainz_artist_id.clone())
                    .collect::<Vec<_>>(),
                track.works.clone(),
            ))
            .collect::<Vec<_>>(),
        (1..=3)
            .map(|number| (
                Some(111_000 * number),
                vec![Some(format!("release-with-works-composer-{number}"))],
                vec![format!("release-with-works-work-{number}")],
            ))
            .collect::<Vec<_>>()
    );

    fixture
        .handle
        .set_sheet_disc(
            fixture.key.clone(),
            "Disc.cue".to_string(),
            crate::folder_scanner::SheetDisc::Ignored,
        )
        .await
        .unwrap();
    let redrawn = stored_draft(&fixture.handle, &hash).await;
    shut_down(fixture.handle).await;

    assert_eq!(redrawn.tracks.len(), 2);
    assert_eq!(redrawn.tracks[0], applied.tracks[0], "the retained track keeps every value");
    let gained = &redrawn.tracks[1];
    assert!(gained.roles.is_empty());
    assert!(gained.works.is_empty());
    assert_eq!(gained.stated_length_ms, None);
    assert_eq!(
        redrawn
            .works
            .works
            .iter()
            .map(|work| work.musicbrainz_work_id.as_str())
            .collect::<Vec<_>>(),
        ["release-with-works-work-1"],
        "the draft keeps only the works its tracks still perform"
    );
}

/// The pane shows each track's stated length beside its file's, from the
/// draft: after a file decision the retained row still shows its own, and
/// the gained row its file's (B3).
#[tokio::test(flavor = "multi_thread")]
async fn the_pane_shows_each_tracks_stated_length_after_a_file_decision() {
    let (fixture, _hash) = three_track_draft().await;
    fixture
        .handle
        .set_sheet_disc(
            fixture.key.clone(),
            "Disc.cue".to_string(),
            crate::folder_scanner::SheetDisc::Ignored,
        )
        .await
        .unwrap();
    let pane = pane(&fixture.handle, &fixture.key).await;
    shut_down(fixture.handle).await;

    let rows: Vec<_> = pane
        .draft()
        .unwrap()
        .mapping
        .track_sides
        .iter()
        .flat_map(crate::MappingTrackSide::mappings)
        .collect();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].duration_ms, Some(111_000));
    let crate::MappingSource::File(file) = &rows[1].source else {
        panic!("the gained row is a whole file");
    };
    assert_eq!(rows[1].duration_ms, file.duration_ms);
    assert!(!rows[1].lengths_disagree);
}

/// The disc a candidate's `Disc.cue` is read as, and the file decision
/// revision it stands at.
async fn sheet_disc_of(
    handle: &ImportServiceHandle,
    key: &crate::CandidateKey,
) -> (crate::folder_scanner::SheetDisc, u64) {
    let candidate = handle.get_release_candidate(key).await.unwrap().unwrap();
    let disc = candidate
        .files
        .track_sheets()
        .find(|sheet| sheet.file.relative_path == "Disc.cue")
        .expect("the candidate holds its sheet")
        .disc;
    (disc, candidate.file_edit_revision)
}

/// A file decision about one folder is about its files, so it settles every
/// folder holding the same files, and no folder holding others.
#[tokio::test(flavor = "multi_thread")]
async fn a_file_decision_settles_every_folder_holding_the_same_files() {
    let (parts, db, tmp) = setup_test_import().await;
    let tables = crate::tables::ImportTables::new(db);
    let (mut original, original_key, _) = picked_candidate(&tables, &tmp, "Album").await;
    std::fs::write(original.path.join("Disc.cue"), SECOND_FILE_IN_TWO).unwrap();
    original.files = crate::folder_scanner::collect_release_candidate_files_with_scope(
        &original.path,
        original.scope,
        &crate::folder_scanner::StoredCandidateEdits::none(),
    )
    .unwrap();
    let copy = twin_of(&original, "Copy");
    let copy_key = crate::CandidateKey::folder(&copy.path);
    let mut other = twin_of(&original, "Other");
    let other_key = crate::CandidateKey::folder(&other.path);
    std::fs::write(other.path.join("Notes.txt"), "a file the others lack").unwrap();
    other.files = crate::folder_scanner::collect_release_candidate_files_with_scope(
        &other.path,
        other.scope,
        &crate::folder_scanner::StoredCandidateEdits::none(),
    )
    .unwrap();
    rescan_all_into(&tables, &[original, copy, other]).await;
    let handle = parts.start().await.unwrap();
    let (unsettled, _) = sheet_disc_of(&handle, &other_key).await;

    handle
        .set_sheet_disc(
            original_key.clone(),
            "Disc.cue".to_string(),
            crate::folder_scanner::SheetDisc::Ignored,
        )
        .await
        .unwrap();

    for key in [&original_key, &copy_key] {
        assert_eq!(
            sheet_disc_of(&handle, key).await,
            (crate::folder_scanner::SheetDisc::Ignored, 1),
            "{key}"
        );
    }
    assert_eq!(sheet_disc_of(&handle, &other_key).await, (unsettled, 0));
    shut_down(handle).await;
}

/// Ignoring a sheet gives its container back as one track; the audio the
/// decision did not touch keeps the track a person named.
#[tokio::test(flavor = "multi_thread")]
async fn changing_audio_preserves_the_metadata_of_surviving_tracks() {
    let StoredCandidate { handle, candidate, key, tmp: _tmp, .. } =
        stored_candidate_with_sheet(SECOND_FILE_IN_TWO).await;
    let hash = candidate.files.content_hash();
    handle.select_candidate_file_tags(key.clone()).await.unwrap();
    let mut tracks = track_rows(&pane(&handle, &key).await.draft().unwrap().mapping);
    assert_eq!(tracks.len(), 3);
    tracks[0].title = "Renamed".to_string();
    handle.set_candidate_track_edit(&key, tracks[0].clone()).await.unwrap();

    handle
        .set_sheet_disc(key, "Disc.cue".to_string(), crate::folder_scanner::SheetDisc::Ignored)
        .await
        .unwrap();

    let preparation = crate::preparations::stored_preparation(&handle.tables, &hash)
        .await
        .unwrap()
        .expect("the reshaped candidate remains prepared");
    assert_eq!(
        preparation
            .metadata
            .draft
            .tracks
            .iter()
            .map(|track| track.edit.file.clone())
            .collect::<Vec<_>>(),
        ["01 Track.flac", "02 Track.flac"]
            .map(|file_id| crate::AudioFile::Standalone { file_id: file_id.to_string() }),
    );
    assert_eq!(preparation.metadata.draft.tracks[0].edit.title, "Renamed");

    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn sheet_decisions_leave_the_draft_drawn_over_the_settled_audio() {
    let StoredCandidate { handle, candidate, key, tmp: _tmp, .. } =
        stored_candidate_with_sheet(SECOND_FILE_IN_TWO).await;
    let hash = candidate.files.content_hash();

    for disc in [
        crate::folder_scanner::SheetDisc::Ignored,
        crate::folder_scanner::SheetDisc::Disc { number: 1 },
    ] {
        handle.set_sheet_disc(key.clone(), "Disc.cue".to_string(), disc).await.unwrap();
        let settled = handle.get_release_candidate(&key).await.unwrap().unwrap();
        let preparation = crate::preparations::stored_preparation(&handle.tables, &hash)
            .await
            .unwrap()
            .expect("the candidate remains prepared");
        assert_eq!(
            preparation
                .metadata
                .draft
                .tracks
                .iter()
                .map(|track| track.edit.file.clone())
                .collect::<Vec<_>>(),
            crate::audio_layout::audio_units(&settled.files),
            "{disc:?}"
        );
    }

    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn ignoring_a_cue_replaces_its_song_rows_with_whole_audio() {
    let fixture = Fixture::new("cue-audio-replacement").await;
    let dir = fixture.seed_cue_album("Album");
    fixture.scan(1).await;
    let hash = fixture.content_hash(&dir);
    let before = crate::preparations::stored_preparation(&fixture.tables(), &hash)
        .await
        .unwrap()
        .unwrap()
        .metadata
        .draft;
    assert_eq!(before.tracks.len(), 5);
    let unchanged = before
        .tracks
        .iter()
        .filter(|track| matches!(track.edit.file, crate::AudioFile::Standalone { .. }))
        .cloned()
        .collect::<Vec<_>>();

    fixture
        .import
        .set_sheet_disc(
            crate::CandidateKey::folder(&dir),
            "Test Album.cue".to_string(),
            crate::folder_scanner::SheetDisc::Ignored,
        )
        .await
        .unwrap();

    let after = crate::preparations::stored_preparation(&fixture.tables(), &hash)
        .await
        .unwrap()
        .unwrap()
        .metadata
        .draft;
    assert_eq!(after.tracks.len(), 3, "one image plus two loose tracks");
    assert!(after
        .tracks
        .iter()
        .all(|track| matches!(track.edit.file, crate::AudioFile::Standalone { .. })));
    for track in unchanged {
        assert!(after.tracks.contains(&track), "unaffected tracks retain identity and metadata");
    }
    assert_eq!(after.album_title, before.album_title);
    let image = after
        .tracks
        .iter()
        .find(|track| {
            matches!(&track.edit.file,
                crate::AudioFile::Standalone { file_id } if file_id == "Test Album.flac"
            )
        })
        .unwrap();
    assert!(!image.edit.title.is_empty());
    fixture
        .import
        .set_sheet_disc(
            crate::CandidateKey::folder(&dir),
            "Test Album.cue".into(),
            crate::folder_scanner::SheetDisc::Disc { number: 1 },
        )
        .await
        .unwrap();
    let restored = crate::preparations::stored_preparation(&fixture.tables(), &hash)
        .await
        .unwrap()
        .unwrap()
        .metadata
        .draft;
    assert_eq!(restored.tracks.len(), 5);
    let slices = restored
        .tracks
        .iter()
        .filter(|track| matches!(track.edit.file, crate::AudioFile::SheetSlice { .. }))
        .collect::<Vec<_>>();
    assert_eq!(slices.len(), 3);
    assert_eq!(slices[0].edit.title, "Track One (Silence)");
    assert!(restored.tracks.iter().all(|track| track.edit.id != image.edit.id));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_claimed_candidate_refuses_sheet_decisions() {
    let fixture = Fixture::new("claimed-file-decisions").await;
    let dir = fixture.seed_cue_album("Album");
    fixture.scan(1).await;
    fixture.archive("mb-claimed-1", "rg-claimed-1", &[500, 500]).await;
    fixture.store_settled_verdict(&dir, "mb-claimed-1", "rg-claimed-1").await;
    let key = crate::CandidateKey::folder(&dir);
    fixture.import.claim_candidate_for_import(&key, "import-1").await;

    for result in [
        fixture
            .import
            .set_sheet_disc(
                key.clone(),
                "Test Album.cue".to_string(),
                crate::folder_scanner::SheetDisc::Disc { number: 1 },
            )
            .await,
        fixture
            .import
            .set_sheet_binding(
                key.clone(),
                "Test Album.cue".to_string(),
                "Test Album.flac".to_string(),
                Some("Test Album.flac".to_string()),
            )
            .await,
    ] {
        assert!(matches!(result, Err(crate::ImportError::CandidateImportInProgress)));
    }
}

/// Clearing a sheet's binding and then naming its audio again is the ordinary
/// way back from "Describes nothing". The second decision lands like the
/// first: the sheet carves its container again and the pane redraws from it.
#[tokio::test(flavor = "multi_thread")]
async fn a_cleared_sheet_can_be_bound_again() {
    let fixture = Fixture::new("rebind-after-clear").await;
    let dir = fixture.seed_cue_album("Album");
    fixture.scan(1).await;
    fixture.archive("mb-rebind-1", "rg-rebind-1", &[500, 500]).await;
    fixture.store_settled_verdict(&dir, "mb-rebind-1", "rg-rebind-1").await;
    let key = crate::CandidateKey::folder(&dir);

    fixture
        .import
        .set_sheet_binding(
            key.clone(),
            "Test Album.cue".to_string(),
            "Test Album.flac".to_string(),
            None,
        )
        .await
        .unwrap();
    // The release was found while identification ran on its own, and the
    // person's decision ends its identification before a run starts.
    fixture.drain_automatic().await;
    assert!(
        fixture.identified_for(&dir).await.is_none(),
        "the cleared binding left the found release unanswered"
    );
    let options = fixture
        .import
        .sheet_binding_options(key.clone(), "Test Album.cue".to_string())
        .await
        .unwrap();
    assert!(
        options[0].options.iter().any(|option| option.file_id == "Test Album.flac"
            && option.offer == crate::folder_scanner::SheetBindingOffer::Offered),
        "the picker offers the container after the clear: {options:?}"
    );

    fixture
        .import
        .set_sheet_binding(
            key.clone(),
            "Test Album.cue".to_string(),
            "Test Album.flac".to_string(),
            Some("Test Album.flac".to_string()),
        )
        .await
        .expect("a cleared sheet binds again");

    let pane = fixture.pane(&dir).await.expect("the candidate reads back");
    let bound = pane
        .candidate
        .files
        .track_sheets()
        .find(|sheet| sheet.file.relative_path == "Test Album.cue")
        .expect("the sheet is still a sheet")
        .binding
        .audio_files()
        .map(|files| files.iter().map(|file| file.file_id.clone()).collect::<Vec<_>>());
    assert_eq!(bound, Some(vec!["Test Album.flac".to_string()]));

    // The draft grew with the slots: three blank tracks for three loose files
    // became the sheet's three entries plus the two loose files, each with a
    // mapping of its own.
    let preparation =
        crate::preparations::stored_preparation(&fixture.tables(), &fixture.content_hash(&dir))
            .await
            .unwrap()
            .expect("the rebound candidate is prepared");
    assert_eq!(preparation.metadata.draft.tracks.len(), 5);
}

#[tokio::test(flavor = "multi_thread")]
async fn changing_a_cues_disc_preserves_titles_but_updates_its_group() {
    let fixture = Fixture::new("cue-disc-change").await;
    let dir = fixture.seed_cue_album("Album");
    fixture.scan(1).await;
    let hash = fixture.content_hash(&dir);
    let key = crate::CandidateKey::folder(&dir);
    let before = crate::preparations::stored_preparation(&fixture.tables(), &hash)
        .await
        .unwrap()
        .unwrap()
        .metadata
        .draft;
    fixture
        .import
        .set_sheet_disc(
            key,
            "Test Album.cue".into(),
            crate::folder_scanner::SheetDisc::Disc { number: 2 },
        )
        .await
        .unwrap();
    let after = crate::preparations::stored_preparation(&fixture.tables(), &hash)
        .await
        .unwrap()
        .unwrap()
        .metadata
        .draft;
    assert_eq!(after.mediums.len(), 2, "the loose audio and the sheet's disc");
    for original in before.tracks {
        let actual = after.tracks.iter().find(|track| track.edit.id == original.edit.id).unwrap();
        assert_eq!(actual.edit.title, original.edit.title);
        assert_eq!(actual.edit.file, original.edit.file);
        if matches!(actual.edit.file, crate::AudioFile::SheetSlice { .. }) {
            // The loose audio, which names no disc, is the first medium;
            // the sheet's slices are the second.
            assert_eq!(actual.place.medium.get(), 2);
        }
    }
}

/// The disc menu and the role picker fire on every selection, including of
/// the item already in force. A decision that re-states what is already true
/// writes nothing — above all it does not clear the stored verdict, which
/// would re-identify a folder whose shape did not change and blank the pane
/// over it.
#[tokio::test(flavor = "multi_thread")]
async fn restating_a_file_decision_changes_nothing() {
    let fixture = Fixture::new("edit-noop").await;
    let dir = fixture.seed_cue_album("Album");
    fixture.scan(1).await;
    fixture.archive("mb-noop-1", "rg-noop-1", &[500, 500]).await;
    fixture.store_settled_verdict(&dir, "mb-noop-1", "rg-noop-1").await;
    let key = crate::CandidateKey::folder(&dir);
    let mut events = fixture.import.every_event();

    // The sheet already carves disc one and already binds its own container.
    fixture
        .import
        .set_sheet_disc(
            key.clone(),
            "Test Album.cue".to_string(),
            crate::folder_scanner::SheetDisc::Disc { number: 1 },
        )
        .await
        .unwrap();
    fixture
        .import
        .set_sheet_binding(
            key.clone(),
            "Test Album.cue".to_string(),
            "Test Album.flac".to_string(),
            Some("Test Album.flac".to_string()),
        )
        .await
        .unwrap();

    let row = fixture.stored_for(&dir).await.expect("the candidate's row remains");
    assert!(row.identify.is_some(), "a re-stated decision must not clear the stored verdict");
    assert!(
        !drain_events(&mut events).iter().any(|event| matches!(
            event,
            ImportEvent::Scan(ScanEvent::CandidateBindingChanged { .. })
        )),
        "and must not announce a changed candidate"
    );

    // A genuinely different decision still lands and still clears.
    fixture
        .import
        .set_sheet_disc(
            key,
            "Test Album.cue".to_string(),
            crate::folder_scanner::SheetDisc::Ignored,
        )
        .await
        .unwrap();
    assert!(
        fixture.stored_for(&dir).await.is_none_or(|row| row.identify.is_none()),
        "a real change clears the verdict as before"
    );
}

impl Fixture {
    /// Copy the whole cue_flac fixture into `<root>/<name>` and return it.
    fn seed_cue_album(&self, name: &str) -> PathBuf {
        let dir = self.root.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        for file in [
            "Test Album.cue",
            "Test Album.flac",
            "02 Test Artist - Track Two (White Noise).flac",
            "03 Test Artist - Track Three (Brown Noise).flac",
        ] {
            copy_fixture(&bae_audio::test_fixtures::cue_flac_dir().join(file), &dir.join(file));
        }
        dir
    }
}
