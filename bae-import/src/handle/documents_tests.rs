//! Reading a candidate's documents: the text of a document or a track sheet,
//! decoded, and a refusal for any other file.

use crate::handle::tests::*;

#[tokio::test(flavor = "multi_thread")]
async fn a_track_sheet_reads_as_its_text() {
    let StoredCandidate { handle, key, tmp: _tmp, .. } =
        stored_candidate_with_sheet(SECOND_FILE_IN_TWO).await;

    let text = handle.read_candidate_document(&key, "Disc.cue").await;
    shut_down(handle).await;

    assert_eq!(text.unwrap(), SECOND_FILE_IN_TWO);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_document_written_in_latin1_reads_decoded() {
    let StoredCandidate { handle, mut candidate, key, tmp: _tmp, .. } = stored_candidate().await;
    std::fs::write(candidate.path.join("notes.txt"), b"Caf\xE9").unwrap();
    candidate.files = crate::folder_scanner::collect_release_candidate_files_with_scope(
        &candidate.path,
        candidate.scope,
        &crate::folder_scanner::StoredCandidateEdits::none(),
    )
    .unwrap();
    rescan_into(&handle.services.tables, candidate).await;

    let text = handle.read_candidate_document(&key, "notes.txt").await;
    shut_down(handle).await;

    assert_eq!(text.unwrap(), "Café");
}

#[tokio::test(flavor = "multi_thread")]
async fn audio_is_not_read_as_a_document() {
    let StoredCandidate { handle, key, tmp: _tmp, .. } = stored_candidate().await;

    let audio = handle.read_candidate_document(&key, "01 Track.flac").await;
    let missing = handle.read_candidate_document(&key, "absent.log").await;
    shut_down(handle).await;

    assert!(
        matches!(audio, Err(crate::ImportError::Document(crate::DocumentRefusal::NotADocument(_)))),
        "{audio:?}"
    );
    assert!(
        matches!(
            missing,
            Err(crate::ImportError::Document(crate::DocumentRefusal::FileGone { .. }))
        ),
        "{missing:?}"
    );
}
