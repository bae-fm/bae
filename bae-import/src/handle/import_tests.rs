//! Starting an import: the claim one import holds on a candidate, the checks
//! the start makes of its files and tags, what the import commits from the
//! draft, and the automatic starts refused with and without a failed import.

use super::*;
use crate::handle::tests::*;
use crate::identification::tests::*;
use crate::service::artist_pictures::tests::{large_picture, picked_discogs_release};
use bae_catalogs::fake_catalogs::FakeCatalogs;
use bae_catalogs::test_releases::seed_mb_release_with_works;
use bae_catalogs::test_releases::seed_two_track_discogs_release;
use bae_catalogs::test_releases::seed_two_track_mb_release;
use bae_catalogs::test_releases::two_track_mb_release_json;
use bae_util::test_channels::HANG_GUARD;

/// A file the scan read that changes before the import stages it fails the
/// import before any row: the release's rows describe the file the scan
/// read, and coven would record the one it finds. The files are checked
/// against the scan before the artist's picture is asked for, and staged
/// after it answers, so a change while the picture is held reaches only the
/// staging.
#[tokio::test(flavor = "multi_thread")]
async fn a_file_changed_after_its_scan_fails_the_import_before_any_row() {
    let pick = picked_discogs_release().await;
    let picture_url = pick.images.serve_image(&pick.picture_path, large_picture());
    let hold = pick.images.hold_image(&pick.picture_path);
    pick.seed_artists(picture_url);

    let mut events = pick.handle.every_event();
    let import_id = pick.handle.start_import(&pick.key).await.unwrap().expect_queued();
    tokio::time::timeout(HANG_GUARD, hold.arrived())
        .await
        .expect("the import asks for the track artist's picture");
    let changed = pick.tmp.path().join("watched/Album/02 Track.flac");
    std::fs::File::options()
        .write(true)
        .open(&changed)
        .and_then(|file| file.set_modified(std::time::SystemTime::now()))
        .expect("set the file's modification time");
    hold.release();

    let error = await_import_outcome(&mut events, &import_id)
        .await
        .expect_err("a file changed since its scan fails the import");
    assert!(error.contains("changed after its import candidate was scanned"), "{error}");
    let rows = pick.db.every_row_for_test().await.unwrap();
    for table in ["releases", "release_files"] {
        assert!(
            rows.get(table).is_none_or(Vec::is_empty),
            "the failed import wrote nothing to {table}"
        );
    }
    shut_down(pick.handle).await;
}

/// What the import writes is the draft: storing the release again after it
/// was applied, its tracklist and credits changed by its editors, changes
/// nothing the candidate imports (B2).
#[tokio::test(flavor = "multi_thread")]
async fn a_release_stored_again_after_apply_changes_nothing_the_draft_imports() {
    let fakes = FakeCatalogs::offline();
    let StoredCandidate { handle, candidate, key, tmp: _tmp, .. } =
        stored_candidate_with(fakes.providers(), bae_util::http::Http::for_test()).await;
    let tables = handle.services.tables.clone();
    seed_mb_release_with_works(
        &fakes,
        "edited-release",
        "edited-release",
        &["First Track", "Second Track"],
        &[1_000, 1_000],
    );
    pick_mb(&handle, &key, "edited-release").await;
    let hash = candidate.files.content_hash();
    let applied = stored_draft(&handle, &hash).await;

    // The catalog's editors retitle the tracks and credit other composers;
    // the release is fetched and stored again over the copy applied.
    seed_mb_release_with_works(
        &fakes,
        "edited-release",
        "changed",
        &["Changed One", "Changed Two"],
        &[1_000, 1_000],
    );
    let edited = bae_catalogs::records::LookupRef::new(
        bae_catalogs::catalog::LookupCatalog::MusicBrainz,
        "edited-release",
    );
    let payloads = handle
        .services
        .catalogs
        .session()
        .await
        .unwrap()
        .fetch_payloads(
            &edited,
            bae_catalogs::payloads::ReadDepth::Complete,
            bae_catalogs::rate_limiter::CallClass::REQUESTED,
        )
        .await
        .unwrap();
    tables.save_source_release(&payloads.extract().unwrap()).await.unwrap();
    assert_eq!(stored_draft(&handle, &hash).await, applied);

    let mut events = handle.every_event();
    let import_id = handle.start_import(&key).await.unwrap().expect_queued();
    let (release_id, _album_id) =
        await_import_outcome(&mut events, &import_id).await.expect("the import completes");
    let written = handle.services.records.track_sources_for_test(&release_id).await.unwrap();
    shut_down(handle).await;

    assert_eq!(
        written,
        [
            (
                "First Track".to_string(),
                vec!["edited-release-composer-1".to_string()],
                vec!["edited-release-work-1".to_string()],
            ),
            (
                "Second Track".to_string(),
                vec!["edited-release-composer-2".to_string()],
                vec!["edited-release-work-2".to_string()],
            ),
        ]
    );
}

/// A draft read from the files' tags, and a draft typed over a blank one,
/// import exactly as the draft stands: nothing is read from the tags again,
/// and nothing a blank release would say fills a field (B14).
#[tokio::test(flavor = "multi_thread")]
async fn a_tags_draft_and_a_blank_draft_import_exactly_the_draft() {
    for from_tags in [true, false] {
        let StoredCandidate { handle, key, tmp: _tmp, .. } = stored_candidate().await;
        if from_tags {
            handle.select_candidate_file_tags(key.clone()).await.unwrap();
        } else {
            handle.clear_candidate_metadata(key.clone()).await.unwrap();
        }
        handle
            .set_candidate_album_artists(
                &key,
                vec![crate::Credited::plain(crate::ArtistAssignment::named("Typed Artist"))],
            )
            .await
            .unwrap();
        handle
            .set_candidate_edit_field(
                &key,
                crate::DraftFieldEdit::Text {
                    field: crate::CandidateEditField::AlbumTitle,
                    value: "Typed Album".to_string(),
                },
            )
            .await
            .unwrap();
        let draft = pane(&handle, &key).await.draft().unwrap().metadata_draft.clone();
        let mut track = draft.tracks[1].clone();
        track.title = "Typed Track".to_string();
        handle.set_candidate_track_edit(&key, track).await.unwrap();
        let draft = pane(&handle, &key).await.draft().unwrap().metadata_draft.clone();

        let mut events = handle.every_event();
        let import_id = handle.start_import(&key).await.unwrap().expect_queued();
        let (release_id, album_id) =
            await_import_outcome(&mut events, &import_id).await.expect("the import completes");
        let tracks = handle.services.records.get_tracks_for_release(&release_id).await.unwrap();
        let album = handle
            .services
            .records
            .find_album_by_id(&album_id)
            .await
            .unwrap()
            .expect("the album is written");
        shut_down(handle).await;

        assert_eq!(album.title, "Typed Album", "from tags: {from_tags}");
        // A track left untitled is titled after its file when its audio is
        // bound; every other value is the draft's.
        assert_eq!(
            tracks
                .iter()
                .map(|track| (track.title.clone(), track.track_number.get()))
                .collect::<Vec<_>>(),
            draft
                .tracks
                .iter()
                .zip(["01 Track", "02 Track"])
                .zip(1..)
                .map(|((track, file), number)| {
                    let title = if track.title.is_empty() { file } else { &track.title };
                    (title.to_string(), number)
                })
                .collect::<Vec<_>>(),
            "from tags: {from_tags}"
        );
        assert_eq!(tracks[1].title, "Typed Track");
    }
}

/// An Import whose claim cannot be written is refused with the failure: no
/// import owns the candidate, none is queued, and the candidate reads as it
/// did before it was asked for.
#[tokio::test(flavor = "multi_thread")]
async fn an_import_whose_claim_cannot_be_written_fails_and_claims_nothing() {
    let (handle, tmp, key, _hash) = pane_fixture().await;
    let before = handle.candidate_runtime(&key).await.unwrap();
    let mut events = handle.every_event();
    crate::tables::ImportTables::fail_standing_writes_storing_for_test(
        tmp.path(),
        crate::tables::import_state::StandingColumn::ImportStep,
        "queued",
    )
    .await;

    let error = handle.start_import(&key).await.unwrap_err();

    assert!(error.to_string().contains("the disk is full"), "{error}");
    assert_eq!(handle.candidate_runtime(&key).await.unwrap(), before);
    assert_eq!(handle.standing_facts_for_test(&key).await, Default::default());
    assert!(
        !std::iter::from_fn(|| events.try_recv().ok())
            .any(|event| matches!(event, ImportEvent::ImportProgress { .. })),
        "no import was queued"
    );
    shut_down(handle).await;
}

/// Every release in the library.
async fn release_count(handle: &ImportServiceHandle) -> usize {
    bae_store::test_fixtures::row_count(&handle.services.store, "releases").await
}

#[tokio::test(flavor = "multi_thread")]
async fn concurrent_imports_keep_one_owner_and_later_imports_of_its_files_are_refused() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    handle
        .set_candidate_album_artists(
            &key,
            vec![crate::Credited::plain(crate::ArtistAssignment::named("Artist Name"))],
        )
        .await
        .unwrap();
    let mut events = handle.every_event();

    let presses = (0..5).map(|_| {
        let handle = handle.clone();
        let key = key.clone();
        tokio::spawn(async move { handle.start_import(&key).await })
    });
    let mut started = Vec::new();
    for press in presses.collect::<Vec<_>>() {
        match press.await.unwrap() {
            Ok(start) => started.push(start.expect_queued()),
            Err(crate::ImportError::CandidateImportInProgress) => {}
            Err(error) => panic!("a repeated press is refused as in progress: {error}"),
        }
    }
    assert_eq!(started.len(), 1, "exactly one press starts an import");
    let (release_id, _) = await_import_outcome(&mut events, &started[0])
        .await
        .unwrap_or_else(|error| panic!("import failed: {error}"));

    for selected in [false, true] {
        let refused = if selected {
            handle.import_selected(&key).await
        } else {
            handle.start_import(&key).await
        };
        assert!(
            matches!(
                &refused,
                Err(crate::ImportError::FilesAlreadyImported { release_id: imported })
                    if *imported == release_id
            ),
            "a later import of the same files is refused before it runs: {refused:?}"
        );
    }
    assert_eq!(release_count(&handle).await, 1);
    shut_down(handle).await;
}

/// Two folders whose files hash alike share one preparation. Both asked to
/// import before either runs, both run: the one that writes second finds the
/// hash taken and fails with the typed error, and the files make one release.
#[tokio::test(flavor = "multi_thread")]
async fn two_imports_of_the_same_files_asked_before_either_runs_make_one_release() {
    let (parts, db, tmp) = setup_test_import().await;
    let tables = crate::tables::ImportTables::new(db);
    let (original, original_key, _) = picked_candidate(&tables, &tmp, "Album").await;
    let copy = twin_of(&original, "Copy");
    let copy_key = crate::CandidateKey::folder(&copy.path);
    rescan_all_into(&tables, &[original, copy]).await;
    let handle = parts.start().await.unwrap();
    // Each folder's own tags are read; the draft they share is then given its
    // artist. Nothing changes it once an import is asked for.
    for key in [&original_key, &copy_key] {
        handle.select_candidate_file_tags(key.clone()).await.unwrap();
    }
    handle
        .set_candidate_album_artists(
            &original_key,
            vec![crate::Credited::plain(crate::ArtistAssignment::named("Artist Name"))],
        )
        .await
        .unwrap();
    let mut events = handle.every_event();

    handle.services.import_cancels.hold_runs();
    let first = handle.start_import(&original_key).await.unwrap().expect_queued();
    let second = handle.start_import(&copy_key).await.unwrap().expect_queued();
    handle.services.import_cancels.release_runs();

    let (release_id, _) = await_import_outcome(&mut events, &first)
        .await
        .unwrap_or_else(|error| panic!("the first import lands: {error}"));
    assert_eq!(
        await_import_outcome(&mut events, &second).await,
        Err(crate::ImportError::FilesAlreadyImported { release_id }.to_string()),
        "the second finds the files imported here"
    );
    assert_eq!(release_count(&handle).await, 1);
    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn import_worker_refuses_a_prepared_but_invalid_metadata_draft() {
    // The scan could not read the folder's tags, so its draft is blank.
    let StoredCandidate { handle, key, tmp: _tmp, .. } = unread_stored_candidate().await;

    let mut events = handle.every_event();
    let import_id = handle
        .start_import(&key)
        .await
        .expect("the complete candidate enters source validation")
        .expect_queued();
    let error = await_import_outcome(&mut events, &import_id)
        .await
        .expect_err("the invalid draft cannot import");

    assert!(error.contains("Album title is required"), "unexpected worker error: {error}");
    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn import_refuses_audio_changed_after_the_file_tags_pane_was_read() {
    let (handle, tmp, key, _hash) = pane_fixture().await;
    let tables = handle.services.tables.clone();
    let root = tmp.path().join("watched");
    let before = tables
        .load_candidate_file_tag_snapshot(&root, &key)
        .await
        .unwrap()
        .expect("the candidate remains stored")
        .snapshot
        .expect("the pane read stored a complete snapshot");

    let changed_audio = tmp.path().join("watched/Album/01 Track.flac");
    let audio = std::fs::OpenOptions::new().write(true).open(&changed_audio).unwrap();
    audio
        .set_times(
            std::fs::FileTimes::new()
                .set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(5)),
        )
        .unwrap();

    let mut events = handle.every_event();
    let import_id = handle
        .start_import(&key)
        .await
        .expect("the prepared candidate enters source validation")
        .expect_queued();
    let error = await_import_outcome(&mut events, &import_id)
        .await
        .expect_err("the changed audio cannot import");
    let after = tables
        .load_candidate_file_tag_snapshot(&root, &key)
        .await
        .unwrap()
        .expect("the candidate remains stored")
        .snapshot
        .expect("a refused import preserves the pane snapshot");
    shut_down(handle).await;

    assert_eq!(after, before, "refusing import must not replace the snapshot the pane displayed");
    assert!(
        error.contains("changed after its import candidate was scanned"),
        "source validation names the stale scan: {error}"
    );
}

/// A bulk import of a selection reaching a row identification is still
/// answering skips that row and says why, leaving it unclaimed for the person
/// to import once the run settles; a row an import already owns is refused the
/// same way rather than claimed twice.
#[tokio::test(flavor = "multi_thread")]
async fn a_bulk_import_skips_a_row_being_identified_and_says_so() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    admit(&handle, &key).await;

    let refused = handle.import_selected(&key).await;

    assert!(matches!(refused, Err(crate::ImportError::CandidateBeingIdentified)));
    let runtime =
        handle.candidate_runtime(&key).await.unwrap().expect("the queued run is still the key's");
    assert!(runtime.import.is_none(), "the skipped row is not claimed");
    assert!(
        matches!(runtime.identification, Some(crate::IdentificationRuntime::Queued(_))),
        "and its run is not cancelled"
    );

    handle.withdraw_identification(&key).await.unwrap();
    handle.claim_candidate_for_import(&key, "import-1").await;
    assert!(matches!(
        handle.import_selected(&key).await,
        Err(crate::ImportError::CandidateImportInProgress)
    ));
    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn numeric_vinyl_import_preserves_unknown_sides_and_track_order() {
    let fakes = FakeCatalogs::offline();
    let (handle, _tmp, key, hash) =
        pane_fixture_with(fakes.providers(), bae_util::http::Http::for_test()).await;
    let release_id = "numeric-vinyl-release";
    fakes
        .seed_musicbrainz_release(release_id, serde_json::json!({
        "id": release_id, "title": "Numbered Record",
        "artist-credit": [{ "name": "Record Artist", "artist": { "id": "record-artist", "name": "Record Artist" } }],
        "media": [{ "format": "12\" Vinyl", "tracks": [
            { "position": 1, "number": "9", "recording": { "title": "First Listed" } },
            { "position": 2, "number": "3", "recording": { "title": "Second Listed" } }
        ] }],
        "cover-art-archive": { "front": false, "darkened": false }
    }).to_string());
    handle
        .select_candidate_release(
            key.clone(),
            bae_identify::PressingLink {
                record: bae_catalogs::records::LookupRef::new(
                    bae_catalogs::catalog::LookupCatalog::MusicBrainz,
                    release_id,
                ),
                partners: vec![],
            },
            crate::CatalogApplySteps::unread(),
        )
        .await
        .unwrap();
    let draft = crate::preparations::stored_preparation(&handle.services.tables, &hash)
        .await
        .unwrap()
        .unwrap()
        .metadata
        .draft;
    assert_eq!(
        draft.pressing.facts.media,
        bae_types::pressing::made_of(bae_types::pressing::Medium::Vinyl, 1).media
    );
    assert_eq!(
        bae_catalogs::release_values::place_labels(
            &draft.mediums,
            draft.tracks.iter().map(|track| track.place)
        ),
        ["A9", "A3"],
        "one record whose positions name no letter is one side, numbered as printed"
    );
    let mut events = handle.every_event();
    let import_id = handle.start_import(&key).await.unwrap().expect_queued();
    let (release_id, _) = await_import_outcome(&mut events, &import_id).await.unwrap();
    let tracks = handle.services.records.get_tracks_for_release(&release_id).await.unwrap();
    assert_eq!(
        tracks
            .iter()
            .map(|track| (track.title.as_str(), track.track_number.get(), track.side.get()))
            .collect::<Vec<_>>(),
        [("First Listed", 9, 1), ("Second Listed", 3, 1)]
    );
    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_import_commits_what_its_picked_releases_store_now() {
    // The pick offers the partner's album address, which the archive holds
    // no image at.
    let archive = bae_util::http::serve_not_found().await;
    let http = bae_util::http::Http::for_test().serve("coverartarchive.org", &archive);
    let fakes = FakeCatalogs::over(http.clone());
    let (handle, _tmp, key, _hash) = pane_fixture_with(fakes.providers(), http).await;
    handle
        .services
        .catalogs
        .store_discogs_key("test-discogs-token", bae_preferences::DiscogsValidation::Valid)
        .await
        .unwrap();
    let primary = "70000004";
    let partner = "refetched-partner-release";
    let group = "refetched-partner-group";
    seed_two_track_discogs_release(&fakes, primary);
    seed_two_track_mb_release(&fakes, partner, group);
    let group_json = |allmusic: &str| {
        serde_json::json!({"id":group,"relations":[{"url":{"resource":format!("https://www.allmusic.com/album/{allmusic}")}}]}).to_string()
    };
    fakes.seed_musicbrainz_release_group(group, group_json("mw111"));
    handle
        .select_candidate_release(
            key.clone(),
            bae_identify::PressingLink {
                record: bae_catalogs::records::LookupRef::new(
                    bae_catalogs::catalog::LookupCatalog::Discogs,
                    primary,
                ),
                partners: vec![bae_catalogs::records::LookupRef::new(
                    bae_catalogs::catalog::LookupCatalog::MusicBrainz,
                    partner,
                )],
            },
            crate::CatalogApplySteps::unread(),
        )
        .await
        .unwrap();
    // The partner is fetched again after the pick, and its album now names
    // another album on AllMusic: a pick references its releases rather than
    // copying them, so the import commits what the partner says now.
    handle
        .services
        .tables
        .save_source_release(
            &bae_catalogs::payloads::ReleasePayloads::for_test(
                bae_catalogs::records::LookupRef::new(
                    bae_catalogs::catalog::LookupCatalog::MusicBrainz,
                    partner,
                ),
                two_track_mb_release_json(partner, group),
                vec![bae_catalogs::records::SourcePayload::new(
                    bae_catalogs::records::PayloadSource::MusicBrainzReleaseGroup,
                    group,
                    group_json("mw222"),
                )],
            )
            .extract()
            .unwrap(),
        )
        .await
        .unwrap();
    let mut events = handle.every_event();
    let import_id = handle.start_import(&key).await.unwrap().expect_queued();
    let (release_id, _) = await_import_outcome(&mut events, &import_id).await.unwrap();
    let records = handle.services.records.get_release_records(&release_id).await.unwrap();
    shut_down(handle).await;
    assert_eq!(
        records
            .iter()
            .find(|record| record.catalog() == bae_catalogs::catalog::Catalog::AllMusic)
            .map(|record| record.key()),
        Some("mw222")
    );
}

/// Files this device already imported are the Done candidate's own state: an
/// automatic start over them, from a second folder holding the same files,
/// is refused and records no failed import.
#[tokio::test(flavor = "multi_thread")]
async fn an_automatic_start_over_already_imported_files_records_no_failure() {
    let fixture = Fixture::new("auto-import-refused").await;
    let imported = fixture.disc_id_candidate("Album");
    fixture.identify_ready(&imported, "mb-refused", "rg-refused").await;
    let mut events = fixture.import.every_event();
    let import_id = fixture
        .import
        .start_import(&crate::CandidateKey::folder(&imported))
        .await
        .unwrap()
        .expect_queued();
    await_import_outcome(&mut events, &import_id).await.unwrap();
    let dir = fixture.disc_id_candidate("Album Copy");
    let key = crate::CandidateKey::folder(&dir);
    fixture.rescan(&fixture.import, 2).await;

    let error = fixture
        .import
        .import_identified(&key)
        .await
        .expect_err("files this device imported do not start a second import");

    assert!(matches!(error, crate::ImportError::FilesAlreadyImported { .. }), "{error:?}");
    let rows = fixture.db.every_row_for_test().await.unwrap();
    assert!(
        rows.get("import_candidate_failure").is_none_or(Vec::is_empty),
        "the refused start records no failed import"
    );
    assert!(
        fixture
            .import
            .candidate_runtime(&key)
            .await
            .unwrap()
            .is_none_or(|runtime| runtime.import.is_none()),
        "nothing claims the candidate"
    );
}

/// An import that already owns the candidate is not a failure of the
/// automatic one: it says how it goes itself.
#[tokio::test(flavor = "multi_thread")]
async fn an_automatic_import_behind_another_import_records_no_failure() {
    let fixture = Fixture::new("auto-import-behind").await;
    let dir = fixture.disc_id_candidate("Album");
    let key = crate::CandidateKey::folder(&dir);
    fixture.identify_ready(&dir, "mb-behind", "rg-behind").await;
    fixture.import.claim_candidate_for_import(&key, "import-1").await;

    let error = fixture
        .import
        .import_identified(&key)
        .await
        .expect_err("a candidate an import owns is not imported again");

    assert!(matches!(error, crate::ImportError::CandidateImportInProgress), "{error}");
    assert_eq!(fixture.import_failure(&dir).await, None);
}

/// An automatic start that a failure refuses has nobody to tell: a candidate
/// whose stored draft is damaged cannot be read, so the start is recorded as
/// the candidate's failed import, where the person sees it, and reported as
/// the bug it is.
#[tokio::test(flavor = "multi_thread")]
async fn an_automatic_start_a_damaged_row_refuses_records_the_failure_and_reports_it() {
    let fixture = Fixture::new("auto-import-damaged").await;
    let dir = fixture.disc_id_candidate("Album");
    let key = crate::CandidateKey::folder(&dir);
    let probed = fixture.probed_total_ms(&dir);
    let mut release: serde_json::Value = serde_json::from_str(
        &bae_catalogs::test_releases::mb_release_json("mb-damaged", "rg-damaged", &[probed, 0]),
    )
    .expect("the release fixture parses");
    release["cover-art-archive"]["front"] = serde_json::json!(true);
    let mut discid: serde_json::Value =
        serde_json::from_str(&discid_json("mb-damaged", "rg-damaged", &[probed, 0]))
            .expect("the disc ID fixture parses");
    discid["releases"][0]["cover-art-archive"]["front"] = serde_json::json!(true);
    fixture.provider.route("/release/mb-damaged/front", 200, cover_png());
    fixture.provider.route("/discid/", 200, discid.to_string());
    fixture.provider.route("/release/mb-damaged?", 200, release.to_string());
    fixture.scan(1).await;
    fixture.drain_automatic().await;
    assert_eq!(fixture.judgement_for(&dir).await, (true, None));
    crate::tables::import_state::tests::disown_the_draft(&fixture.db, &fixture.content_hash(&dir))
        .await;

    let error = fixture
        .import
        .import_identified(&key)
        .await
        .expect_err("a candidate that cannot be read does not start an import");

    assert!(
        matches!(error, crate::ImportError::Database(bae_store::DbError::Unreadable { .. })),
        "{error:?}"
    );
    fixture.import_failure(&dir).await.expect("the refused start is recorded");
    let bugs = fixture.bugs.reports();
    assert_eq!(bugs.len(), 1, "{bugs:?}");
    assert!(bugs[0].0.contains("start the automatic import"), "{bugs:?}");
}

/// The library closing under an automatic start is not a failure of the
/// candidate: nothing is recorded, nothing is reported, and nothing stays
/// claimed.
#[tokio::test(flavor = "multi_thread")]
async fn an_automatic_start_the_closing_library_stops_records_nothing() {
    let fixture = Fixture::new("auto-import-stopped").await;
    let dir = fixture.disc_id_candidate("Album");
    let key = crate::CandidateKey::folder(&dir);
    fixture.identify_ready(&dir, "mb-stopped", "rg-stopped").await;
    fixture.import.stop_and_join();

    let error = fixture
        .import
        .import_identified(&key)
        .await
        .expect_err("a stopped service queues no import");

    assert!(matches!(error, crate::ImportError::Stopped), "{error:?}");
    assert_eq!(fixture.import_failure(&dir).await, None);
    assert_eq!(fixture.bugs.reports(), Vec::new());
    assert!(
        fixture
            .import
            .candidate_runtime(&key)
            .await
            .unwrap()
            .is_none_or(|runtime| runtime.import.is_none()),
        "nothing claims the candidate"
    );
}

/// The import worker ending on a bug ends the import service with it: an
/// automatic start after it fails with the worker's bug, is recorded as the
/// candidate's failed import, and claims nothing, and the bug is reported
/// once, as it happened. Sabotage: answer a send to the ended worker as a
/// closing library, and the start fails as though nothing went wrong.
#[tokio::test(flavor = "multi_thread")]
async fn an_automatic_start_after_the_worker_panicked_fails_with_its_bug() {
    let fixture = Fixture::new("auto-import-worker-panicked").await;
    let dir = fixture.disc_id_candidate("Album");
    let key = crate::CandidateKey::folder(&dir);
    fixture.identify_ready(&dir, "mb-panicked", "rg-panicked").await;
    assert!(fixture.import.worker.send(crate::service::ImportWorkerMessage::Panic).is_ok());
    let bug = coven::task::Bug {
        operation: "import worker thread",
        message: "the import worker broke".to_string(),
    };
    assert_eq!(
        fixture.import.worker.ended().await,
        bae_util::worker_thread::WorkerEnd::Panicked(bug.clone())
    );

    let error =
        fixture.import.import_identified(&key).await.expect_err("an ended worker queues no import");

    assert!(
        matches!(&error, crate::ImportError::WorkerFailed(heard) if heard == &bug),
        "{error:?}"
    );
    assert!(fixture.import_failure(&dir).await.is_some(), "the failed start is recorded");
    assert_eq!(
        fixture.bugs.reports(),
        vec![(
            "import worker thread".to_string(),
            "import worker thread panicked: the import worker broke".to_string()
        )]
    );
    assert!(
        fixture
            .import
            .candidate_runtime(&key)
            .await
            .unwrap()
            .is_none_or(|runtime| runtime.import.is_none()),
        "nothing claims the candidate"
    );
}

impl Fixture {
    /// The failed import `dir`'s pane shows, if any.
    async fn import_failure(&self, dir: &Path) -> Option<crate::ImportFailure> {
        self.tables()
            .load_import_candidate_pane_rows(&self.content_hash(dir))
            .await
            .unwrap()
            .failure
    }
}
