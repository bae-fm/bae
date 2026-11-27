//! Which row the pane marks as the one the preview player holds.

use super::*;
use bae_playback::tables::PlaybackNowRow;
use bae_playback::{PlaybackChange, PreviewState};
use bae_track_audio::PreviewTarget;

/// Each track row's audition target and whether it is marked as previewing.
fn auditions(pane: &crate::ImportCandidateDetail) -> Vec<(PreviewTarget, bool)> {
    let draft = pane.draft().expect("a folder nobody imported holds a draft");
    draft
        .mapping
        .track_sides
        .iter()
        .flatten()
        .flat_map(crate::TrackTableSide::rows)
        .map(|row| match &row.audition {
            crate::TrackAudition::Preview(audition) => {
                (audition.target.clone(), audition.previewing)
            }
            other => panic!("a folder's row previews its file, got {other:?}"),
        })
        .collect()
}

/// The next value of the pane's open read.
async fn next_pane(
    live: &mut coven::ReconfigurableLiveQuery<
        Option<crate::CandidateKey>,
        Option<crate::ImportCandidateDetail>,
        crate::ImportError,
    >,
) -> crate::ImportCandidateDetail {
    tokio::time::timeout(bae_util::test_channels::HANG_GUARD, live.next_inline())
        .await
        .expect("a change to the preview reaches the pane")
        .expect("the query stays open")
        .result
        .unwrap()
        .expect("listed")
}

/// The pane's open read marks the one track row whose stretch of audio the
/// preview player holds, playing or paused, and none once the preview ends.
#[tokio::test]
async fn the_pane_marks_the_row_the_preview_player_holds() {
    let (db, _temp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    stored_pane_candidate(&db).await;
    let playback = PlaybackNowRow::new(db.clone());
    let mut live = tables.subscribe_import_candidate(Some(pane_candidate_key()));
    let rows = auditions(&next_pane(&mut live).await);
    assert!(rows.iter().all(|(_, previewing)| !previewing), "nothing previews yet");
    let (target, _) = rows.first().expect("the folder has a track").clone();
    let marked: Vec<_> = rows.iter().map(|(row, _)| (row.clone(), *row == target)).collect();

    let playing = PreviewState::Playing { target: target.clone(), duration_ms: 1_000 };
    playback.apply(&PlaybackChange::Preview(playing)).await.unwrap();
    assert_eq!(auditions(&next_pane(&mut live).await), marked);

    // A pause leaves every row as playing marked it, so the open read has no
    // new value to send.
    let paused = PreviewState::Paused { target, duration_ms: 1_000 };
    playback.apply(&PlaybackChange::Preview(paused)).await.unwrap();
    let pane = tables.load_import_candidate(&pane_candidate_key()).await.unwrap().expect("listed");
    assert_eq!(auditions(&pane), marked);

    playback.apply(&PlaybackChange::Preview(PreviewState::Idle)).await.unwrap();
    assert_eq!(auditions(&next_pane(&mut live).await), rows);
}

/// The pane of a folder that failed validation marks the audio file the
/// preview player holds.
#[tokio::test]
async fn the_skipped_pane_marks_the_file_the_preview_player_holds() {
    use crate::folder_scanner::{InvalidCandidate, ProposedRole, SkippedFile};
    let (db, _tmp, root) = watched_root().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    tables.add_watched_import_folder(&root).await.unwrap();
    let folder = PathBuf::from(&root).join("Half Copied");
    let file = |name: &str| SkippedFile {
        relative_path: name.to_string(),
        file: FileVersion { path: folder.join(name), size: 1_000, modified_at_ns: 1 },
        role: ProposedRole::Audio,
    };
    let invalid = InvalidCandidate {
        path: folder.clone(),
        name: "Half Copied".to_string(),
        watched_folder_path: root.clone(),
        relative_folder: crate::RelativeFolder::test("Half Copied"),
        grouping: None,
        files: vec![file("01.flac"), file("02.flac")],
        tags: None,
    };
    let key = invalid.key();
    let generation = tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    tables.save_folder_scan_item(&root, generation, &ScanItem::Invalid(invalid)).await.unwrap();
    tables.finish_folder_scan(&root, generation, None).await.unwrap();

    let playing = PreviewState::Playing {
        target: PreviewTarget::whole_file(folder.join("02.flac")),
        duration_ms: 1_000,
    };
    PlaybackNowRow::new(db.clone()).apply(&PlaybackChange::Preview(playing)).await.unwrap();

    let read = tables
        .subscribe_invalid_candidate(Some(key))
        .next_inline()
        .await
        .expect("the query stays open")
        .result
        .unwrap()
        .expect("the key names a folder that failed validation");
    assert_eq!(
        read.files
            .iter()
            .map(|file| {
                let previewing = file.audition.as_ref().map(|audition| audition.previewing);
                (file.file_id.as_str(), previewing)
            })
            .collect::<Vec<_>>(),
        [("01.flac", Some(false)), ("02.flac", Some(true))]
    );
}

/// A change to playback that is not its preview, such as the volume, leaves
/// the pane's read alone.
#[tokio::test]
async fn a_volume_change_leaves_the_pane_s_read_unmarked() {
    let (db, _temp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    stored_pane_candidate(&db).await;
    let mut live = tables.subscribe_import_candidate(Some(pane_candidate_key()));
    live.next_inline().await.expect("the query stays open").result.unwrap().expect("listed");

    PlaybackNowRow::new(db.clone()).apply(&PlaybackChange::Volume(0.5)).await.unwrap();

    assert!(!live.is_marked_for_rerun(), "the pane reads only the preview of playback's row");
}
