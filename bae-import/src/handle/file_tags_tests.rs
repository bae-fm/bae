//! A candidate's file tags: the stored reading while its audio is unchanged,
//! a new complete reading when the files, the scan or a file decision moved,
//! and the cover applying the tags selects.

use super::*;
use crate::handle::tests::*;
use bae_util::test_channels::HANG_GUARD;

/// Apply the folder's own file tags as its metadata, reading them with
/// `reader`.
async fn pick_file_tags(
    handle: &mut ImportServiceHandle,
    key: &crate::CandidateKey,
    reader: std::sync::Arc<CountingFileTagReader>,
) -> Result<u64, crate::ImportError> {
    handle.services.file_tags = reader;
    handle.select_candidate_file_tags(key.clone()).await
}

/// The file-tag reading stored for the candidate at `key`.
async fn stored_tags(
    handle: &ImportServiceHandle,
    candidate: &crate::folder_scanner::FolderCandidate,
    key: &crate::CandidateKey,
) -> Option<crate::file_tag_snapshot::FileTagSnapshot> {
    let tables = handle.services.tables.clone();
    tables
        .load_candidate_file_tag_snapshot(&candidate.watched_folder_path, key)
        .await
        .unwrap()
        .expect("the candidate remains stored")
        .snapshot
}

#[tokio::test(flavor = "multi_thread")]
async fn a_file_that_cannot_be_read_stores_no_partial_tag_snapshot() {
    let StoredCandidate { mut handle, candidate, key, tmp: _tmp, .. } =
        unread_stored_candidate().await;
    let reader = std::sync::Arc::new(CountingFileTagReader::failing(1));

    let error = pick_file_tags(&mut handle, &key, reader.clone())
        .await
        .expect_err("the second audio file, which could not be read, rejects the reading");
    let stored = stored_tags(&handle, &candidate, &key).await;
    shut_down(handle).await;

    assert!(error.to_string().contains("fixture tag read 1 failed"));
    assert_eq!(reader.read_count(), 2);
    assert!(stored.is_none(), "facts from the first file cannot land without the complete reading");
}

/// Tags that do not parse state nothing: the folder's other files' tags make
/// the draft, the file's name titles its track, and the reading keeps why its
/// tags do not parse.
#[tokio::test(flavor = "multi_thread")]
async fn a_file_whose_tags_do_not_parse_leaves_the_others_to_make_the_draft() {
    let StoredCandidate { mut handle, candidate, key, tmp: _tmp, .. } =
        unread_stored_candidate().await;
    let reader = std::sync::Arc::new(CountingFileTagReader::unparseable(1));

    pick_file_tags(&mut handle, &key, reader.clone())
        .await
        .expect("the folder's tags are applied though one file's do not parse");
    let draft = stored_draft(&handle, &candidate.files.content_hash()).await;
    let stored = stored_tags(&handle, &candidate, &key).await;
    shut_down(handle).await;

    assert_eq!(draft.album_title, "Album Title");
    assert_eq!(
        draft.tracks.iter().map(|track| track.edit.title.as_str()).collect::<Vec<_>>(),
        ["Track Title 1", "02 Track"]
    );
    let stored = stored.expect("the reading is stored");
    assert_eq!(titles(&stored), [Some("Track Title 1"), None]);
    assert_eq!(
        stored.files[1].tags,
        crate::file_tag_snapshot::TagReading::Unparseable(
            "fixture tags 1 do not parse".to_string()
        )
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn matching_file_observations_reuse_the_stored_tag_snapshot() {
    let StoredCandidate { mut handle, candidate, key, tmp: _tmp, .. } =
        unread_stored_candidate().await;
    let reader = std::sync::Arc::new(CountingFileTagReader::immediate());

    pick_file_tags(&mut handle, &key, reader.clone()).await.unwrap();
    assert_eq!(reader.read_count(), 2);
    let read = handle.read_file_tag_snapshot(&key, reader.clone()).await.unwrap();

    assert_eq!(Some(read.snapshot), stored_tags(&handle, &candidate, &key).await);
    assert_eq!(
        reader.read_count(),
        2,
        "an exact stored observation avoids opening the audio tags again"
    );
    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn changed_file_observations_read_the_tags_again() {
    let StoredCandidate { mut handle, mut candidate, key, tmp, .. } =
        unread_stored_candidate().await;
    let reader = std::sync::Arc::new(CountingFileTagReader::immediate());

    pick_file_tags(&mut handle, &key, reader.clone()).await.unwrap();

    let first_audio = tmp.path().join("watched/Album/01 Track.flac");
    let audio = std::fs::OpenOptions::new().write(true).open(&first_audio).unwrap();
    audio
        .set_times(
            std::fs::FileTimes::new()
                .set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(5)),
        )
        .unwrap();

    let after_modified_time =
        handle.read_file_tag_snapshot(&key, reader.clone()).await.unwrap().snapshot;
    assert_eq!(reader.read_count(), 4);
    assert_eq!(titles(&after_modified_time), vec![Some("Track Title 3"), Some("Track Title 4")]);

    let second_audio = tmp.path().join("watched/Album/02 Track.flac");
    let mut audio = std::fs::OpenOptions::new().append(true).open(&second_audio).unwrap();
    std::io::Write::write_all(&mut audio, &[0]).unwrap();
    candidate
        .files
        .files
        .iter_mut()
        .find(|file| file.file.relative_path == "02 Track.flac")
        .unwrap()
        .file
        .version
        .size = std::fs::metadata(&second_audio).unwrap().len();
    rescan_unread_into(&handle.services.tables, candidate).await;

    let after_size = handle.read_file_tag_snapshot(&key, reader.clone()).await.unwrap().snapshot;
    assert_eq!(reader.read_count(), 6);
    assert_eq!(titles(&after_size), vec![Some("Track Title 5"), Some("Track Title 6")]);
    shut_down(handle).await;
}

/// A scan that finds the same files leaves the stored reading standing — it
/// re-read the very files that reading was taken from — while a file decision
/// makes the candidate a different shape and its tags are read again.
#[tokio::test(flavor = "multi_thread")]
async fn a_file_decision_makes_the_tags_read_again() {
    let StoredCandidate { mut handle, candidate, key, tmp: _tmp, .. } =
        unread_stored_candidate().await;
    let reader = std::sync::Arc::new(CountingFileTagReader::immediate());
    pick_file_tags(&mut handle, &key, reader.clone()).await.unwrap();

    rescan_into(&handle.services.tables, candidate).await;

    let after_generation =
        handle.read_file_tag_snapshot(&key, reader.clone()).await.unwrap().snapshot;
    assert_eq!(reader.read_count(), 2);
    assert_eq!(titles(&after_generation), vec![Some("Track Title 1"), Some("Track Title 2")]);
    shut_down(handle).await;

    let StoredCandidate { mut handle, key, tmp: _tmp, .. } =
        stored_candidate_with_sheet(SECOND_FILE_IN_TWO).await;
    let reader = std::sync::Arc::new(CountingFileTagReader::immediate());
    pick_file_tags(&mut handle, &key, reader.clone()).await.unwrap();

    handle
        .set_sheet_disc(
            key.clone(),
            "Disc.cue".to_string(),
            crate::folder_scanner::SheetDisc::Ignored,
        )
        .await
        .unwrap();

    let after_file_edit =
        handle.read_file_tag_snapshot(&key, reader.clone()).await.unwrap().snapshot;
    assert_eq!(reader.read_count(), 4);
    assert_eq!(titles(&after_file_edit), vec![Some("Track Title 3"), Some("Track Title 4")]);
    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn file_tags_cannot_restore_mappings_read_before_a_file_decision() {
    let StoredCandidate { handle, candidate, key, tmp, .. } =
        stored_candidate_with_sheet(SECOND_FILE_IN_TWO).await;
    let hash = candidate.files.content_hash();
    handle.select_candidate_file_tags(key.clone()).await.unwrap();
    let stale = crate::preparations::stored_preparation(&handle.services.tables, &hash)
        .await
        .unwrap()
        .expect("the candidate starts prepared");
    handle
        .set_sheet_disc(
            key.clone(),
            "Disc.cue".to_string(),
            crate::folder_scanner::SheetDisc::Ignored,
        )
        .await
        .unwrap();
    let snapshot = handle
        .read_file_tag_snapshot(&key, std::sync::Arc::new(CountingFileTagReader::immediate()))
        .await
        .unwrap()
        .snapshot;
    let root = tmp.path().join("watched");

    let error = handle
        .services
        .preparations
        .apply_file_metadata(
            &root,
            &key,
            &crate::CandidateAsRead {
                content_hash: hash.clone(),
                file_edit_revision: stale.file_edits.revision,
                metadata_revision: stale.metadata_revision,
            },
            &snapshot,
            &stale.metadata.draft,
            stale.metadata.cover.as_ref(),
        )
        .await
        .expect_err("the new file snapshot cannot carry mappings from the old file revision");

    assert!(error.to_string().contains("candidate changed"), "{error}");
    shut_down(handle).await;
}

/// Tags read while a scan stored the candidate again describe an older scan,
/// so picking them reads them again and stores the reading of the newer one.
#[tokio::test(flavor = "multi_thread")]
async fn a_scan_that_moves_during_tag_reading_makes_the_pick_read_again() {
    let StoredCandidate { mut handle, candidate, key, tmp: _tmp, .. } =
        unread_stored_candidate().await;
    let (reader, reached, gate) = CountingFileTagReader::held();
    let reader = std::sync::Arc::new(reader);
    handle.services.file_tags = reader.clone();
    let operation = tokio::spawn({
        let handle = handle.clone();
        let key = key.clone();
        async move { handle.select_candidate_file_tags(key).await }
    });
    reached.recv_timeout(HANG_GUARD).expect("the tag reader reached the first audio file");

    rescan_into(&handle.services.tables, candidate.clone()).await;
    gate.open();

    operation.await.unwrap().expect("the pick reads the tags of the newer scan");
    let stored = handle
        .services
        .tables
        .load_candidate_file_tag_snapshot(&candidate.watched_folder_path, &key)
        .await
        .unwrap()
        .expect("the candidate remains stored");
    assert_eq!(reader.read_count(), 4, "the tags were read twice");
    assert_eq!(
        stored.snapshot.map(|snapshot| snapshot.scan_generation),
        Some(stored.scan_generation)
    );
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
    assert_eq!(cover.selection(), crate::CoverSelection::Local("cover.jpg".to_string()));
    let scanned = candidate
        .files
        .files
        .iter()
        .find(|entry| entry.file.relative_path == "cover.jpg")
        .expect("the fixture folder holds cover.jpg");
    assert_eq!(
        cover.into_image(),
        crate::cover_art::CoverImageSource::Local { file: scanned.file.version.clone() },
        "a folder image is drawn from disk at the version the scan read"
    );

    shut_down(handle).await;
}

/// A folder image named as the front cover ranks ahead of the tags' embedded
/// artwork when file tags are applied, and the selection persists.
#[tokio::test(flavor = "multi_thread")]
async fn file_tags_persist_the_front_cover_image_ahead_of_embedded_artwork() {
    let StoredCandidate { mut handle, parts, key, tmp: _tmp, .. } = stored_candidate().await;
    handle.services.file_tags =
        std::sync::Arc::new(CountingFileTagReader::with_embedded_cover(vec![1, 2, 3, 4]));
    handle.select_candidate_file_tags(key.clone()).await.unwrap();

    let cover = pane(&handle, &key)
        .await
        .draft()
        .unwrap()
        .cover
        .clone()
        .expect("applying tags that embed artwork selects a cover");
    assert_eq!(cover.selection(), crate::CoverSelection::Local("cover.jpg".to_string()));
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
            .selection(),
        crate::CoverSelection::Local("cover.jpg".to_string())
    );
    shut_down(reopened).await;
}
