use super::*;
use crate::import::{AudioFile, CandidateAsRead, TrackArtistAssignments};

fn as_read(detail: &crate::import::ImportCandidateDetail) -> CandidateAsRead {
    CandidateAsRead {
        content_hash: detail.candidate.files.content_hash(),
        file_edit_revision: detail.candidate.file_edit_revision,
        metadata_revision: detail.metadata_revision,
    }
}

pub(super) async fn preparation(
    handle: &ImportServiceHandle,
    hash: &str,
) -> crate::db::DbCandidateImportPreparation {
    handle
        .library_manager
        .load_import_candidate_preparation(hash)
        .await
        .unwrap()
        .unwrap()
}

async fn three_track_candidate() -> StoredCandidate {
    let mut fixture = stored_candidate().await;
    let mut third = fixture.candidate.files.files[1].clone();
    let path = fixture.candidate.path.join("03 Track.flac");
    std::fs::copy(&third.file.path, &path).unwrap();
    let metadata = std::fs::metadata(&path).unwrap();
    third.file.path = path.clone();
    third.file.relative_path = "03 Track.flac".into();
    third.file.modified_at_ns =
        crate::import::folder_scanner::file_modified_at_ns(&path, &metadata).unwrap();
    fixture.candidate.files.files.insert(2, third);
    rescan_into(&fixture.manager, fixture.candidate.clone()).await;
    fixture
}

struct SourceTags {
    number: Option<u32>,
}

impl crate::import::file_tag_snapshot::FileTagReader for SourceTags {
    fn read(
        &self,
        _path: &Path,
    ) -> Result<crate::import::file_tag_snapshot::FileTagRead, crate::import::ImportError> {
        Ok(crate::import::file_tag_snapshot::FileTagRead {
            title: None,
            track_artist: Some("Source Artist".into()),
            album_title: Some("Source Album".into()),
            album_artist: Some("Source Artist".into()),
            year: Some(2001),
            track_number: self.number,
            disc_number: Some(3),
            embedded_cover: None,
        })
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn removed_audio_remains_visible_in_the_candidate_pane() {
    use crate::import::mapping::{MappingSource, MappingTrackSection};

    let (handle, _tmp, key, _hash) = pane_fixture().await;
    let before = pane(&handle, &key).await;
    let removed = before.metadata_draft.tracks[0].clone();
    handle.drop_candidate_track(&key, removed.id).await.unwrap();

    let after = pane(&handle, &key).await;
    let visible = after
        .mapping
        .track_sections
        .iter()
        .flat_map(MappingTrackSection::mappings)
        .any(|row| {
            matches!(&row.source, MappingSource::File(file) if file.file_id == "01 Track.flac")
        });
    shut_down(handle).await;

    assert_eq!(after.metadata_draft.tracks.len(), 1);
    assert!(
        visible,
        "removed audio remains available beside the included tracks"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn restoring_middle_audio_initializes_only_that_row_from_current_preferences() {
    for (prefill, number, expected_number) in
        [(false, Some(9), 2), (true, None, 2), (true, Some(9), 9)]
    {
        let StoredCandidate {
            handle,
            manager,
            key,
            candidate,
            tmp: _tmp,
        } = three_track_candidate().await;
        manager.set_prefill_with_file_metadata(prefill).unwrap();
        handle
            .file_tag_snapshot_with_reader(&key, Arc::new(SourceTags { number }))
            .await
            .unwrap();
        handle
            .set_candidate_edit_field(
                &key,
                crate::import::CandidateEditField::AlbumTitle,
                "Typed Album".into(),
            )
            .await
            .unwrap();
        handle
            .set_candidate_album_artists(
                &key,
                vec![crate::import::ArtistAssignment::named("Typed Artist")],
            )
            .await
            .unwrap();
        let mut removed = pane(&handle, &key).await.metadata_draft.tracks[1].clone();
        removed.title = "Removed metadata".into();
        handle
            .set_candidate_track_edit(&key, removed.clone())
            .await
            .unwrap();
        handle
            .drop_candidate_track(&key, removed.id.clone())
            .await
            .unwrap();
        let hash = candidate.files.content_hash();
        let before = preparation(&handle, &hash).await;
        let offer = as_read(&pane(&handle, &key).await);

        handle
            .add_candidate_track(&key, removed.file.clone().unwrap(), offer)
            .await
            .unwrap();
        let after = preparation(&handle, &hash).await;
        let restored = &after.draft.tracks[1];
        assert_ne!(restored.edit.id, removed.id);
        assert_eq!(restored.edit.file, removed.file.unwrap());
        assert_eq!(restored.source_index, None);
        assert_eq!(restored.edit.title, if prefill { "02 Track" } else { "" });
        assert_eq!(restored.edit.track_number, expected_number);
        assert_eq!(restored.edit.side, if prefill { Some(3) } else { None });
        assert_eq!(
            restored.edit.artist_assignments,
            if prefill {
                TrackArtistAssignments::Explicit(vec![crate::import::ArtistAssignment::named(
                    "Source Artist",
                )])
            } else {
                TrackArtistAssignments::AlbumArtists
            }
        );
        let mut retained = after.draft.clone();
        retained.tracks.remove(1);
        assert_eq!(retained, before.draft);
        assert_eq!(after.assets, before.assets);
        assert_eq!(after.cover, before.cover);
        assert_eq!(after.metadata_provenance, before.metadata_provenance);
        assert_eq!(after.file_edit_revision, before.file_edit_revision);
        assert_eq!(after.metadata_revision, before.metadata_revision + 1);
        shut_down(handle).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn restoration_keeps_surviving_audio_swaps_in_place() {
    let StoredCandidate {
        handle,
        key,
        candidate,
        tmp: _tmp,
        ..
    } = three_track_candidate().await;
    let mut rows = pane(&handle, &key).await.metadata_draft.tracks;
    let removed = rows[1].clone();
    rows[0].file = rows[2].file.clone();
    handle
        .set_candidate_track_edit(&key, rows[0].clone())
        .await
        .unwrap();
    handle.drop_candidate_track(&key, removed.id).await.unwrap();
    let before = preparation(&handle, &candidate.files.content_hash()).await;
    handle
        .add_candidate_track(
            &key,
            removed.file.unwrap(),
            as_read(&pane(&handle, &key).await),
        )
        .await
        .unwrap();
    let after = preparation(&handle, &candidate.files.content_hash()).await;
    assert_eq!(after.draft.tracks[1..], before.draft.tracks);
    assert_eq!(after.draft.tracks[0].edit.file.file_id(), "02 Track.flac");
    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn repeated_and_concurrent_adds_do_not_duplicate_audio() {
    let (handle, _tmp, key, hash) = pane_fixture().await;
    let removed = pane(&handle, &key).await.metadata_draft.tracks[0].clone();
    handle.drop_candidate_track(&key, removed.id).await.unwrap();
    let offer = as_read(&pane(&handle, &key).await);
    let audio = removed.file.unwrap();
    let (left, right) = tokio::join!(
        handle.add_candidate_track(&key, audio.clone(), offer.clone()),
        handle.add_candidate_track(&key, audio.clone(), offer.clone()),
    );
    assert!(left.is_ok() || right.is_ok());
    let before_repeat = preparation(&handle, &hash).await;
    handle
        .add_candidate_track(&key, audio.clone(), offer)
        .await
        .unwrap();
    assert_eq!(preparation(&handle, &hash).await, before_repeat);
    assert_eq!(
        before_repeat
            .draft
            .tracks
            .iter()
            .filter(|row| row.edit.file == audio)
            .count(),
        1
    );
    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn restoration_rejects_a_stale_metadata_offer_without_overwriting_edits() {
    let (handle, _tmp, key, hash) = pane_fixture().await;
    let removed = pane(&handle, &key).await.metadata_draft.tracks[0].clone();
    handle.drop_candidate_track(&key, removed.id).await.unwrap();
    let offer = as_read(&pane(&handle, &key).await);
    handle
        .set_candidate_edit_field(
            &key,
            crate::import::CandidateEditField::AlbumTitle,
            "New title".into(),
        )
        .await
        .unwrap();
    let before = preparation(&handle, &hash).await;
    let error = handle
        .add_candidate_track(&key, removed.file.unwrap(), offer)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("metadata changed"), "{error}");
    assert_eq!(preparation(&handle, &hash).await, before);
    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn restoration_rejects_a_changed_file_revision_even_if_audio_is_still_available() {
    let (handle, _tmp, key, hash) = pane_fixture().await;
    let removed = pane(&handle, &key).await.metadata_draft.tracks[0].clone();
    handle.drop_candidate_track(&key, removed.id).await.unwrap();
    let offer = as_read(&pane(&handle, &key).await);
    handle
        .set_file_role(
            key.clone(),
            "02 Track.flac".into(),
            crate::import::folder_scanner::FileRoleChoice::NotATrack,
        )
        .await
        .unwrap();
    let before = preparation(&handle, &hash).await;
    let error = handle
        .add_candidate_track(&key, removed.file.unwrap(), offer)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("changed"), "{error}");
    assert_eq!(preparation(&handle, &hash).await, before);
    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn restoration_rejects_an_offer_from_before_a_rescan_changed_the_content_hash() {
    let StoredCandidate {
        handle,
        manager,
        mut candidate,
        key,
        tmp: _tmp,
    } = stored_candidate().await;
    let removed = pane(&handle, &key).await.metadata_draft.tracks[0].clone();
    handle.drop_candidate_track(&key, removed.id).await.unwrap();
    let offer = as_read(&pane(&handle, &key).await);
    let artwork = candidate.files.files.last_mut().unwrap();
    std::fs::OpenOptions::new()
        .append(true)
        .open(&artwork.file.path)
        .and_then(|mut file| std::io::Write::write_all(&mut file, &[0]))
        .unwrap();
    artwork.file.size += 1;
    rescan_into(&manager, candidate.clone()).await;
    let before = preparation(&handle, &candidate.files.content_hash()).await;
    assert_ne!(candidate.files.content_hash(), offer.content_hash);
    let error = handle
        .add_candidate_track(&key, removed.file.unwrap(), offer)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("changed"), "{error}");
    assert_eq!(
        preparation(&handle, &candidate.files.content_hash()).await,
        before
    );
    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn claimed_candidates_refuse_restoration_and_already_included_noops() {
    for removed in [false, true] {
        let (handle, _tmp, key, hash) = pane_fixture().await;
        let row = pane(&handle, &key).await.metadata_draft.tracks[0].clone();
        if removed {
            handle.drop_candidate_track(&key, row.id).await.unwrap();
        }
        let offer = as_read(&pane(&handle, &key).await);
        let before = preparation(&handle, &hash).await;
        handle.claim_candidate_for_import_for_test(&key).await;
        assert!(matches!(
            handle
                .add_candidate_track(&key, row.file.unwrap(), offer)
                .await,
            Err(crate::import::ImportError::CandidateImportInProgress)
        ));
        assert_eq!(preparation(&handle, &hash).await, before);
        shut_down(handle).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn restoration_uses_the_combined_candidate_disc_and_number() {
    for prefill in [false, true] {
        let (manager, _library) = setup_test_manager().await;
        let first_root = TempDir::new().unwrap();
        let second_root = TempDir::new().unwrap();
        let (_, first, _) = picked_candidate(&manager, &first_root, "Volume A").await;
        let (_, second, _) = picked_candidate(&manager, &second_root, "Volume B").await;
        let handle = manager
            .start_import_service(tokio::runtime::Handle::current())
            .await
            .unwrap();
        let key = handle
            .combine_candidates(vec![first, second])
            .await
            .unwrap();
        manager.set_prefill_with_file_metadata(prefill).unwrap();
        let before = pane(&handle, &key).await;
        let removed = before.metadata_draft.tracks[2].clone();
        assert_eq!((removed.side, removed.track_number), (Some(2), Some(1)));
        handle
            .drop_candidate_track(&key, removed.id.clone())
            .await
            .unwrap();
        handle
            .add_candidate_track(
                &key,
                removed.file.clone().unwrap(),
                as_read(&pane(&handle, &key).await),
            )
            .await
            .unwrap();
        let after = pane(&handle, &key).await;
        assert_eq!(after.metadata_draft.tracks.len(), 4);
        let restored = &after.metadata_draft.tracks[2];
        assert_eq!(restored.file, removed.file);
        assert_eq!((restored.side, restored.track_number), (Some(2), Some(1)));
        assert_ne!(restored.id, removed.id);
        assert_eq!(
            after.metadata_draft.tracks[..2],
            before.metadata_draft.tracks[..2]
        );
        assert_eq!(
            after.metadata_draft.tracks[3],
            before.metadata_draft.tracks[3]
        );
        shut_down(handle).await;
    }
}

pub(super) async fn cue_candidate() -> StoredCandidate {
    let mut fixture = stored_candidate().await;
    std::fs::write(
        fixture.candidate.path.join("Disc.cue"),
        concat!(
            "PERFORMER \"Cue Artist\"\nTITLE \"Cue Album\"\n",
            "FILE \"01 Track.flac\" WAVE\n",
            "  TRACK 07 AUDIO\n    TITLE \"Cue First\"\n    INDEX 01 00:00:00\n",
            "  TRACK 08 AUDIO\n    PERFORMER \"Slice Artist\"\n    INDEX 01 00:00:30\n",
            "FILE \"02 Track.flac\" WAVE\n",
            "  TRACK 09 AUDIO\n    TITLE \"Cue Last\"\n    INDEX 01 00:00:00\n",
        ),
    )
    .unwrap();
    fixture.candidate.files =
        crate::import::folder_scanner::collect_release_candidate_files_with_scope(
            &fixture.candidate.path,
            fixture.candidate.scope,
            &crate::import::folder_scanner::StoredCandidateEdits::none(),
        )
        .unwrap();
    rescan_into(&fixture.manager, fixture.candidate.clone()).await;
    fixture
}

#[tokio::test(flavor = "multi_thread")]
async fn restoring_cue_slices_preserves_their_exact_file_index_and_initial_metadata() {
    for prefill in [false, true] {
        let StoredCandidate {
            handle,
            manager,
            key,
            candidate,
            tmp: _tmp,
        } = cue_candidate().await;
        manager.set_prefill_with_file_metadata(prefill).unwrap();
        let rows = pane(&handle, &key).await.metadata_draft.tracks;
        assert_eq!(rows.len(), 3);
        for index in [1, 2] {
            let removed = rows[index].clone();
            handle
                .drop_candidate_track(&key, removed.id.clone())
                .await
                .unwrap();
            let offer = as_read(&pane(&handle, &key).await);
            handle
                .add_candidate_track(&key, removed.file.clone().unwrap(), offer)
                .await
                .unwrap();
            let after = preparation(&handle, &candidate.files.content_hash()).await;
            let restored = &after.draft.tracks[index];
            assert_eq!(
                restored.edit.file,
                AudioFile::SheetSlice {
                    file_id: if index == 1 {
                        "01 Track.flac"
                    } else {
                        "02 Track.flac"
                    }
                    .into(),
                    sheet_id: "Disc.cue".into(),
                    index: index as u32,
                }
            );
            assert_ne!(restored.edit.id, removed.id);
            assert_eq!(restored.source_index, None);
            assert_eq!(restored.edit.side, Some(1));
            assert_eq!(
                restored.edit.track_number,
                if prefill {
                    7 + index as i32
                } else {
                    index as i32 + 1
                }
            );
            assert_eq!(
                restored.edit.title,
                if prefill && index == 2 {
                    "Cue Last"
                } else {
                    ""
                }
            );
            if !prefill {
                assert_eq!(
                    restored.edit.artist_assignments,
                    TrackArtistAssignments::AlbumArtists
                );
            }
            if prefill && index == 1 {
                assert_eq!(
                    restored.edit.artist_assignments,
                    TrackArtistAssignments::Explicit(vec![crate::import::ArtistAssignment::named(
                        "Slice Artist"
                    )])
                );
            }
        }
        shut_down(handle).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn unavailable_cue_slices_cannot_be_restored_after_ignore_or_cleared_file_binding() {
    for clear_binding in [false, true] {
        let StoredCandidate {
            handle,
            key,
            candidate,
            tmp: _tmp,
            ..
        } = cue_candidate().await;
        let removed = pane(&handle, &key).await.metadata_draft.tracks[2].clone();
        handle.drop_candidate_track(&key, removed.id).await.unwrap();
        let old_offer = as_read(&pane(&handle, &key).await);
        if clear_binding {
            handle
                .set_sheet_binding(key.clone(), "Disc.cue".into(), "02 Track.flac".into(), None)
                .await
                .unwrap();
        } else {
            handle
                .set_sheet_disc(
                    key.clone(),
                    "Disc.cue".into(),
                    crate::import::folder_scanner::SheetDisc::Ignored,
                )
                .await
                .unwrap();
        }
        let current = pane(&handle, &key).await;
        assert!(current
            .metadata_draft
            .tracks
            .iter()
            .all(|track| matches!(track.file, Some(AudioFile::Standalone { .. }))));
        let before = preparation(&handle, &candidate.files.content_hash()).await;
        for offer in [old_offer, as_read(&current)] {
            assert!(handle
                .add_candidate_track(&key, removed.file.clone().unwrap(), offer)
                .await
                .is_err());
            assert_eq!(
                preparation(&handle, &candidate.files.content_hash()).await,
                before
            );
        }
        shut_down(handle).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn removing_all_cue_tracks_keeps_the_sheet_and_individual_sources_visible() {
    use crate::import::mapping::{MappingBecomes, MappingTrackSectionContent};
    let StoredCandidate {
        handle,
        key,
        tmp: _tmp,
        ..
    } = cue_candidate().await;
    let rows = pane(&handle, &key).await.metadata_draft.tracks;
    for row in &rows {
        handle
            .drop_candidate_track(&key, row.id.clone())
            .await
            .unwrap();
    }
    let empty = pane(&handle, &key).await;
    assert!(empty.metadata_draft.tracks.is_empty());
    assert_eq!(empty.mapping.track_sections.len(), 1);
    let MappingTrackSectionContent::Sheet { sheet, entries } =
        &empty.mapping.track_sections[0].content
    else {
        panic!("the selected sheet remains visible");
    };
    assert_eq!(sheet.sheet_id, "Disc.cue");
    assert_eq!(entries.len(), 3);
    assert!(entries
        .iter()
        .all(|row| matches!(row.becomes, MappingBecomes::NotIncluded { .. })));
    handle
        .add_candidate_track(&key, rows[1].file.clone().unwrap(), as_read(&empty))
        .await
        .unwrap();
    assert_eq!(pane(&handle, &key).await.metadata_draft.tracks.len(), 1);
    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn restoring_audio_keeps_the_applied_release_without_claiming_its_removed_source_track() {
    let (handle, _tmp, key, hash) = pane_fixture().await;
    handle
        .library_manager
        .set_discogs_key(
            "test-discogs-token",
            crate::config::DiscogsValidation::Valid,
        )
        .unwrap();
    let release_id = "70000106";
    handle.library_manager.providers().discogs().seed_release_cache(release_id, serde_json::json!({
        "id": 70000106, "title": "Selected Album", "year": 1996,
        "artists": [{ "id": 70000106, "name": "Selected Artist" }],
        "formats": [{ "name": "CD" }],
        "tracklist": [
            { "position": "1", "title": "Selected First", "duration": "0:01", "type_": "track" },
            { "position": "2", "title": "Selected Second", "duration": "0:01", "type_": "track" }
        ]
    }).to_string());
    handle.library_manager.providers().musicbrainz().seed_discogs_url_lookup(release_id, None);
    handle.library_manager.providers().discogs().seed_artist_image_response("70000106", None);
    handle
        .select_candidate_metadata_provenance(
            key.clone(),
            crate::import::MetadataProvenance::ExternalRelease {
                record: crate::import::MetadataRef::new(
                    crate::import::Catalog::Discogs,
                    release_id,
                ),
                partners: vec![],
            },
        )
        .await
        .unwrap();
    let selected = preparation(&handle, &hash).await;
    assert_eq!(selected.draft.tracks[0].source_index, Some(0));
    let removed = selected.draft.tracks[0].clone();
    handle
        .drop_candidate_track(&key, removed.edit.id.clone())
        .await
        .unwrap();
    let before = preparation(&handle, &hash).await;
    handle
        .add_candidate_track(&key, removed.edit.file, as_read(&pane(&handle, &key).await))
        .await
        .unwrap();
    let after = preparation(&handle, &hash).await;
    assert_eq!(after.draft.tracks[0].source_index, None);
    assert_eq!(after.draft.tracks[0].edit.title, "");
    assert_eq!(after.draft.tracks[1], before.draft.tracks[0]);
    assert_eq!(after.metadata_provenance, before.metadata_provenance);
    assert_eq!(after.assets, before.assets);
    assert_eq!(after.cover, before.cover);
    assert_eq!(after.draft.album_title, "Selected Album");
    assert_eq!(
        after.draft.album_artist_assignments,
        before.draft.album_artist_assignments
    );
    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn restoration_prepared_before_a_newer_edit_cannot_overwrite_it() {
    let StoredCandidate {
        mut handle,
        manager,
        key,
        candidate,
        tmp: _tmp,
    } = stored_candidate().await;
    let removed = pane(&handle, &key).await.metadata_draft.tracks[0].clone();
    handle.drop_candidate_track(&key, removed.id).await.unwrap();
    let offer = as_read(&pane(&handle, &key).await);
    manager.set_prefill_with_file_metadata(true).unwrap();
    let (entered_tx, entered_rx) = std::sync::mpsc::sync_channel(1);
    let resume = Arc::new(std::sync::Barrier::new(2));
    handle.file_tags = Arc::new(CountingFileTagReader::blocking(entered_tx, resume.clone()));
    let restoring = tokio::spawn({
        let handle = handle.clone();
        let key = key.clone();
        async move {
            handle
                .add_candidate_track(&key, removed.file.unwrap(), offer)
                .await
        }
    });
    entered_rx
        .recv_timeout(std::time::Duration::from_secs(2))
        .expect("restoration reached tag reading");
    let edit = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        handle.set_candidate_edit_field(
            &key,
            crate::import::CandidateEditField::AlbumTitle,
            "Changed during tag reading".into(),
        ),
    )
    .await;
    resume.wait();
    edit.expect("tag reading does not hold the candidate commit lock")
        .unwrap();
    let before = preparation(&handle, &candidate.files.content_hash()).await;
    let error = restoring.await.unwrap().unwrap_err();
    assert!(error.to_string().contains("metadata changed"), "{error}");
    assert_eq!(
        preparation(&handle, &candidate.files.content_hash()).await,
        before
    );
    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn prepared_track_insertion_and_artist_answers_roll_back_together() {
    let (handle, _tmp, key, hash) = pane_fixture().await;
    let source = preparation(&handle, &hash).await;
    let mut removed = source.draft.tracks[0].clone();
    handle
        .drop_candidate_track(&key, removed.edit.id.clone())
        .await
        .unwrap();
    let detail = pane(&handle, &key).await;
    let before = preparation(&handle, &hash).await;
    removed.edit.id = handle.ids.new_id();
    removed.source_index = None;
    let error = handle
        .preparations
        .add_track_prepared(
            &detail.candidate.watched_folder_path,
            &key,
            &as_read(&detail),
            &removed,
            0,
            &before.source_discogs_artist_ids,
            &[crate::import::PreparedArtistImage::Nothing {
                discogs_artist_id: "unreferenced-artist".into(),
            }],
        )
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("artist assets do not match"),
        "{error}"
    );
    assert_eq!(preparation(&handle, &hash).await, before);
    shut_down(handle).await;
}
