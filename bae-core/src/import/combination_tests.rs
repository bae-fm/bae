use super::*;
use crate::import::folder_registry::host_root;
use crate::import::folder_scanner::{CandidateFile, ReleaseFileScope, ScannedFile};
use std::path::PathBuf;

fn folder(name: &str) -> FolderCandidate {
    let path = PathBuf::from(host_root("/music")).join(name);
    FolderCandidate {
        path: path.clone(),
        file_root: path.clone(),
        name: name.into(),
        watched_folder_path: host_root("/music"),
        scope: ReleaseFileScope::Recursive,
        file_edit_revision: 0,
        display_path: name.into(),
        resolved_boundaries: Vec::new(),
        combine_ancestor_key: None,
        files: CategorizedFiles {
            files: ["01.flac", "02.flac"]
                .into_iter()
                .map(|name| CandidateFile {
                    file: ScannedFile::new(path.join(name), name.into(), 100, 12)
                        .with_test_flac_audio(),
                    role: FileRole::Audio,
                    proposed_audio: true,
                })
                .collect(),
        },
    }
}

#[test]
fn combines_only_selected_files_without_changing_their_physical_paths() {
    let selected = [folder("Volume B"), folder("Volume A")];
    let combined =
        CandidateCombination::prepare(&selected, CombinationTrackOrder::SeparateDiscs).unwrap();
    assert_eq!(combined.parts[0].folder_name, "Volume B");
    assert_eq!(combined.parts[1].folder_name, "Volume A");
    assert_eq!(combined.parts[0].first_disc, 1);
    assert_eq!(combined.parts[1].first_disc, 2);
    assert_eq!(combined.files.files.len(), 4);
    assert_eq!(
        combined
            .files
            .release_files()
            .map(|file| &file.path)
            .collect::<Vec<_>>(),
        selected
            .iter()
            .flat_map(|folder| folder.files.release_files().map(|file| &file.path))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        combined
            .tracks
            .iter()
            .map(|track| track.side)
            .collect::<Vec<_>>(),
        [1, 1, 2, 2]
    );
    assert_eq!(
        super::super::track_slots::audio_units(&combined.files),
        combined
            .tracks
            .iter()
            .map(|track| track.file.clone().unwrap())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        combined
            .files
            .release_files()
            .map(|file| file.relative_path.as_str())
            .collect::<Vec<_>>(),
        [
            "01 - Volume B/01.flac",
            "01 - Volume B/02.flac",
            "02 - Volume A/01.flac",
            "02 - Volume A/02.flac"
        ]
    );
}

#[test]
fn continuous_order_numbers_every_selected_track_in_one_sequence() {
    let combined = CandidateCombination::prepare(
        &[folder("Volume B"), folder("Volume A")],
        CombinationTrackOrder::Continuous,
    )
    .unwrap();
    assert!(combined.tracks.iter().all(|track| track.side == 1));
    assert_eq!(
        combined
            .tracks
            .iter()
            .map(|track| track.track_number)
            .collect::<Vec<_>>(),
        [Some(1), Some(2), Some(3), Some(4)]
    );
}

#[test]
fn review_reorders_the_exact_selection_and_rejects_missing_or_duplicate_sources() {
    let review = CombinationReview::new(vec![folder("Volume A"), folder("Volume B")]).unwrap();
    let original = review.candidate_keys();
    let reversed = original.iter().rev().cloned().collect::<Vec<_>>();
    let preview = review
        .preview(&reversed, CombinationTrackOrder::SeparateDiscs)
        .unwrap();
    assert_eq!(
        preview
            .parts
            .iter()
            .map(|part| &part.candidate_key)
            .collect::<Vec<_>>(),
        reversed.iter().collect::<Vec<_>>()
    );
    assert_eq!(
        preview.tracks[0].file.as_ref().unwrap().file_id(),
        "01 - Volume B/01.flac"
    );
    assert_eq!(review.candidate_keys(), original);
    assert!(review
        .preview(&original[..1], CombinationTrackOrder::Continuous)
        .is_err());
    assert!(review
        .preview(
            &[original[0].clone(), original[0].clone()],
            CombinationTrackOrder::Continuous
        )
        .is_err());
    assert!(review
        .preview(
            &[original[0].clone(), host_root("/music/Unselected")],
            CombinationTrackOrder::Continuous
        )
        .is_err());
}

#[test]
fn rejects_duplicate_candidates_and_overlapping_files() {
    let first = folder("Volume A");
    assert!(CandidateCombination::prepare(
        &[first.clone(), first.clone()],
        CombinationTrackOrder::SeparateDiscs
    )
    .is_err());
    let mut second = folder("Volume B");
    second.files.files[0] = first.files.files[0].clone();
    assert!(
        CandidateCombination::prepare(&[first, second], CombinationTrackOrder::SeparateDiscs)
            .is_err()
    );
}

#[test]
fn file_metadata_preserves_reviewed_numbering_instead_of_original_disc_tags() {
    use crate::import::file_tag_snapshot::{FileObservation, FileTagFact, FileTagSnapshot};
    use crate::import::release_candidate::{CombinedCandidate, ReleaseCandidate};
    let clock = coven::FixedClock(
        chrono::DateTime::parse_from_rfc3339("2026-01-15T12:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc),
    );
    for order in [
        CombinationTrackOrder::SeparateDiscs,
        CombinationTrackOrder::Continuous,
    ] {
        let combination =
            CandidateCombination::prepare(&[folder("Volume B"), folder("Volume A")], order)
                .unwrap();
        let expected = combination
            .tracks
            .iter()
            .map(|track| (track.side, track.track_number))
            .collect::<Vec<_>>();
        let snapshot = FileTagSnapshot {
            scan_generation: 1,
            file_edit_revision: 0,
            embedded_cover: None,
            files: combination
                .files
                .audio()
                .enumerate()
                .map(|(index, file)| FileTagFact {
                    observation: FileObservation {
                        relative_path: file.relative_path.clone(),
                        size: file.size,
                        modified_at_ns: file.modified_at_ns,
                    },
                    title: Some(format!("Tagged Track {index}")),
                    track_artist: Some("Test Artist".into()),
                    album_title: Some("Original Album".into()),
                    album_artist: Some("Test Artist".into()),
                    year: None,
                    track_number: Some(1),
                    disc_number: Some(7),
                })
                .collect(),
        };
        let candidate = ReleaseCandidate::Combined(CombinedCandidate {
            key: "combination:test".into(),
            name: "Collected Volumes".into(),
            watched_folder_path: host_root("/music"),
            order,
            combination,
            file_edit_revision: 0,
        });
        let edit = candidate
            .file_tag_edit(&snapshot, &clock, &coven::UuidProvider)
            .unwrap();
        assert_eq!(
            edit.tracks
                .iter()
                .map(|track| (track.side, track.track_number))
                .collect::<Vec<_>>(),
            expected
        );
        assert_eq!(edit.tracks[2].title, "Tagged Track 2");
    }
}

#[test]
fn continuous_metadata_keeps_the_sources_corrected_cue_order() {
    use crate::cue_flac::{CuePregap, CueSheet, CueTrack, CueTrackMode};
    use crate::import::folder_scanner::SheetAudioFile;

    let mut first = folder("Volume A");
    for (file_name, disc) in [("01.flac", 2), ("02.flac", 1)] {
        let sheet_name = format!("{file_name}.cue");
        first.files.files.push(CandidateFile {
            file: ScannedFile::new(first.path.join(&sheet_name), sheet_name, 100, 12),
            role: FileRole::TrackSheet {
                sheet: CueSheet {
                    title: None,
                    performer: None,
                    catalog: None,
                    date: None,
                    tracks: vec![CueTrack {
                        number: 1,
                        mode: CueTrackMode::Audio,
                        title: None,
                        performer: None,
                        indexes: Vec::new(),
                        file_reference: file_name.into(),
                        start_cue_frames: 0,
                        pregap: CuePregap::None,
                        end_cue_frames: None,
                    }],
                },
                binding: SheetBinding::Resolved {
                    files: vec![SheetAudioFile {
                        file_reference: file_name.into(),
                        file_id: file_name.into(),
                    }],
                },
                disc: SheetDisc::Disc { number: disc },
            },
            proposed_audio: false,
        });
    }
    let combined = CandidateCombination::prepare(
        &[first, folder("Volume B")],
        CombinationTrackOrder::Continuous,
    )
    .unwrap();
    assert_eq!(
        combined.tracks[0].file.as_ref().unwrap().file_id(),
        "01 - Volume A/02.flac"
    );
    assert_eq!(
        super::super::track_slots::audio_units(&combined.files),
        combined
            .tracks
            .iter()
            .map(|track| track.file.clone().unwrap())
            .collect::<Vec<_>>()
    );
}

async fn stored_selection() -> (crate::db::Database, tempfile::TempDir, Vec<FolderCandidate>) {
    let temp = tempfile::TempDir::new().unwrap();
    let clock = chrono::DateTime::parse_from_rfc3339("2026-01-15T12:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let db = crate::db::Database::new_test(
        temp.path().join("test.db").to_str().unwrap(),
        std::sync::Arc::new(coven::FixedClock(clock)),
        std::sync::Arc::new(coven::UuidProvider),
    )
    .await
    .unwrap();
    db.add_watched_import_folder(&host_root("/music"))
        .await
        .unwrap();
    let generation = db.begin_folder_scan(&host_root("/music")).await.unwrap();
    let candidates = vec![folder("Volume B"), folder("Volume A")];
    for candidate in &candidates {
        db.save_folder_scan_item(
            &host_root("/music"),
            generation,
            &super::super::folder_scanner::ScanItem::Valid(candidate.clone()),
        )
        .await
        .unwrap();
    }
    db.finish_folder_scan(&host_root("/music"), generation, None)
        .await
        .unwrap();
    (db, temp, candidates)
}

#[tokio::test]
async fn stored_combination_is_one_release_and_separating_restores_sources() {
    let (db, _temp, candidates) = stored_selection().await;
    let original = db
        .load_import_candidate(&host_root("/music/Volume B"))
        .await
        .unwrap()
        .unwrap();
    db.combine_candidates(
        "combination:test".into(),
        "Collected Volumes".into(),
        candidates.clone(),
        CombinationTrackOrder::SeparateDiscs,
    )
    .await
    .unwrap();
    let detail = db
        .load_import_candidate("combination:test")
        .await
        .unwrap()
        .unwrap();
    assert!(detail.actionable);
    assert_eq!(detail.metadata_draft.album_title, "Collected Volumes");
    assert_eq!(
        detail
            .metadata_draft
            .tracks
            .iter()
            .map(|track| track.side)
            .collect::<Vec<_>>(),
        [1, 1, 2, 2]
    );
    assert_eq!(
        db.load_import_list(Default::default())
            .await
            .unwrap()
            .summary
            .counts
            .pending,
        1
    );
    assert_eq!(db.load_sweepable_candidates().await.unwrap().len(), 0);
    assert_eq!(
        db.load_folder_scan_items(&host_root("/music"))
            .await
            .unwrap()
            .len(),
        2
    );
    db.separate_combined_candidate("combination:test")
        .await
        .unwrap();
    assert!(db
        .load_import_candidate("combination:test")
        .await
        .unwrap()
        .is_none());
    assert_eq!(
        db.load_import_list(Default::default())
            .await
            .unwrap()
            .summary
            .counts
            .pending,
        2
    );
    assert_eq!(
        db.load_import_candidate(&host_root("/music/Volume B"))
            .await
            .unwrap()
            .unwrap()
            .metadata_draft,
        original.metadata_draft
    );
}

#[tokio::test]
async fn unchanged_rescan_keeps_combination_and_missing_source_blocks_it() {
    use super::super::folder_scanner::ScanItem;
    let (db, _temp, candidates) = stored_selection().await;
    db.combine_candidates(
        "combination:test".into(),
        "Collected Volumes".into(),
        candidates.clone(),
        CombinationTrackOrder::Continuous,
    )
    .await
    .unwrap();
    let generation = db.begin_folder_scan(&host_root("/music")).await.unwrap();
    for candidate in &candidates {
        db.save_folder_scan_item(
            &host_root("/music"),
            generation,
            &ScanItem::Discovered(candidate.clone()),
        )
        .await
        .unwrap();
        db.save_folder_scan_item(
            &host_root("/music"),
            generation,
            &ScanItem::Valid(candidate.clone()),
        )
        .await
        .unwrap();
    }
    db.finish_folder_scan(&host_root("/music"), generation, None)
        .await
        .unwrap();
    assert!(
        db.load_import_candidate("combination:test")
            .await
            .unwrap()
            .unwrap()
            .actionable
    );
    let generation = db.begin_folder_scan(&host_root("/music")).await.unwrap();
    db.save_folder_scan_item(
        &host_root("/music"),
        generation,
        &ScanItem::Valid(candidates[0].clone()),
    )
    .await
    .unwrap();
    db.finish_folder_scan(&host_root("/music"), generation, None)
        .await
        .unwrap();
    let detail = db
        .load_import_candidate("combination:test")
        .await
        .unwrap()
        .unwrap();
    assert!(!detail.actionable);
    assert!(detail.source_error.as_ref().unwrap().contains("Volume A"));
    let blocked = detail.resolve(&Default::default());
    assert_eq!(
        blocked.composition_action,
        Some(CombinationAction::Separate)
    );
    assert!(blocked.row.actions.is_empty());
    assert!(blocked.row.skip_action.is_none());
    assert!(db.load_release_candidate("combination:test").await.is_err());
    assert_eq!(
        db.load_import_list(Default::default())
            .await
            .unwrap()
            .summary
            .counts
            .pending,
        1
    );
}

#[tokio::test]
async fn blocked_combination_keeps_embedded_artwork_readable_for_separation() {
    use crate::import::file_tag_snapshot::{
        EmbeddedCoverFact, FileObservation, FileTagFact, FileTagSnapshot,
    };
    let (db, _temp, candidates) = stored_selection().await;
    let root = host_root("/music");
    let key = "combination:embedded";
    db.combine_candidates(
        key.into(),
        "Collected Volumes".into(),
        candidates.clone(),
        CombinationTrackOrder::SeparateDiscs,
    )
    .await
    .unwrap();
    let stored = db
        .load_candidate_file_tag_snapshot(&root, key)
        .await
        .unwrap()
        .unwrap();
    let files = stored.candidate.files();
    let cover_id = files.audio().next().unwrap().relative_path.clone();
    let snapshot = FileTagSnapshot {
        scan_generation: stored.scan_generation,
        file_edit_revision: 0,
        files: files
            .audio()
            .map(|file| FileTagFact {
                observation: FileObservation {
                    relative_path: file.relative_path.clone(),
                    size: file.size,
                    modified_at_ns: file.modified_at_ns,
                },
                title: None,
                track_artist: None,
                album_title: None,
                album_artist: None,
                year: None,
                track_number: None,
                disc_number: None,
            })
            .collect(),
        embedded_cover: Some(EmbeddedCoverFact {
            source_relative_path: cover_id.clone(),
            content_type: crate::util::content_type::ContentType::Png,
            data: vec![1, 2, 3],
        }),
    };
    assert!(db
        .replace_candidate_file_tag_snapshot(&root, key, &snapshot)
        .await
        .unwrap());
    db.save_import_candidate_cover(
        &files.content_hash(),
        &crate::import::CoverSelection::Embedded(cover_id),
    )
    .await
    .unwrap();
    assert!(db
        .load_import_candidate(key)
        .await
        .unwrap()
        .unwrap()
        .cover
        .is_some());
    let generation = db.begin_folder_scan(&root).await.unwrap();
    db.save_folder_scan_item(
        &root,
        generation,
        &super::super::folder_scanner::ScanItem::Valid(candidates[0].clone()),
    )
    .await
    .unwrap();
    db.finish_folder_scan(&root, generation, None)
        .await
        .unwrap();
    let blocked = db.load_import_candidate(key).await.unwrap().unwrap();
    assert!(!blocked.actionable);
    assert!(blocked.cover.is_some());
    assert_eq!(
        blocked.resolve(&Default::default()).composition_action,
        Some(CombinationAction::Separate)
    );
    db.separate_combined_candidate(key).await.unwrap();
}

#[tokio::test]
async fn changed_review_is_rejected_without_hiding_any_sources() {
    let (db, _temp, mut candidates) = stored_selection().await;
    candidates[0].files.files[0].file.size += 1;
    assert!(db
        .combine_candidates(
            "combination:test".into(),
            "Collected Volumes".into(),
            candidates,
            CombinationTrackOrder::SeparateDiscs
        )
        .await
        .is_err());
    assert!(db
        .load_import_candidate("combination:test")
        .await
        .unwrap()
        .is_none());
    assert_eq!(
        db.load_import_list(Default::default())
            .await
            .unwrap()
            .summary
            .counts
            .pending,
        2
    );
}
