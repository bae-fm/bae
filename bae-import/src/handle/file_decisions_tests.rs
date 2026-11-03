//! File decisions: a sheet's disc read again redraws the draft over the
//! settled audio, keeps each retained track's values, and settles every folder
//! holding the same files.

use super::*;
use crate::handle::tests::*;

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
    let (parts, database, tmp) = setup_test_import().await;
    let tables = crate::tables::ImportTables::new(database);
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

    let preparation = crate::preparation::import_preparation(&handle.tables, &hash)
        .await
        .unwrap()
        .expect("the reshaped candidate remains prepared");
    assert_eq!(
        preparation.draft.tracks.iter().map(|track| track.edit.file.clone()).collect::<Vec<_>>(),
        ["01 Track.flac", "02 Track.flac"]
            .map(|file_id| crate::AudioFile::Standalone { file_id: file_id.to_string() }),
    );
    assert_eq!(preparation.draft.tracks[0].edit.title, "Renamed");

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
        let preparation = crate::preparation::import_preparation(&handle.tables, &hash)
            .await
            .unwrap()
            .expect("the candidate remains prepared");
        assert_eq!(
            preparation
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
