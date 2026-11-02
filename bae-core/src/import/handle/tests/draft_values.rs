// The draft holds every value the import writes; nothing reads the source
// again after a source is applied.

/// A MusicBrainz release of `titles.len()` tracks on one CD, each stating its
/// length, credited to its own composer and performing its own work, the
/// composers' and works' ids starting `credits`.
fn seed_mb_release_with_works(
    fakes: &FakeCatalogs,
    release_id: &str,
    credits: &str,
    titles: &[&str],
    lengths_ms: &[u64],
) {
    let tracks: Vec<_> = titles
        .iter()
        .zip(lengths_ms)
        .enumerate()
        .map(|(index, (title, length))| {
            let number = index + 1;
            serde_json::json!({
                "position": number,
                "number": number.to_string(),
                "length": length,
                "recording": {
                    "title": title,
                    "relations": [
                        {
                            "target-type": "artist",
                            "type": "composer",
                            "artist": {
                                "id": format!("{credits}-composer-{number}"),
                                "name": format!("Composer Name {number}"),
                            },
                        },
                        {
                            "target-type": "work",
                            "type": "performance",
                            "work": {
                                "id": format!("{credits}-work-{number}"),
                                "title": format!("Work Title {number}"),
                            },
                        },
                    ],
                },
            })
        })
        .collect();
    fakes.seed_musicbrainz_release(
        release_id,
        serde_json::json!({
            "id": release_id,
            "title": "Album Title",
            "artist-credit": [{
                "name": "Artist Name",
                "artist": { "id": "mb-artist-1", "name": "Artist Name" },
            }],
            "media": [{ "format": "CD", "tracks": tracks }],
            "cover-art-archive": { "front": false, "darkened": false },
        })
        .to_string(),
    );
}

async fn pick_mb(
    handle: &ImportServiceHandle,
    key: &crate::import::CandidateKey,
    release_id: &str,
) {
    handle
        .select_candidate_release(
            key.clone(),
            crate::identify::PressingLink {
                record: bae_catalogs::records::LookupRef::new(
                    bae_catalogs::catalog::LookupCatalog::MusicBrainz,
                    release_id,
                ),
                partners: vec![],
            },
            crate::import::CatalogApplySteps::unread(),
        )
        .await
        .unwrap();
}

async fn stored_draft(handle: &ImportServiceHandle, hash: &str) -> crate::import::CandidateDraft {
    crate::import::preparation::import_preparation(&handle.tables, hash)
        .await
        .unwrap()
        .expect("the candidate is prepared")
        .draft
}

/// A draft read from a release whose second file a sheet carves into two
/// tracks: the release's three tracks lie on `01 Track.flac` and the sheet's
/// two slices.
async fn three_track_draft() -> (StoredCandidate, String) {
    let fakes = FakeCatalogs::offline();
    let mut fixture = stored_candidate_with(fakes.providers()).await;
    add_sheet(&fixture.handle.tables, &mut fixture.candidate, SECOND_FILE_IN_TWO).await;
    fixture
        .handle
        .set_sheet_disc(
            fixture.key.clone(),
            "Disc.cue".to_string(),
            crate::import::folder_scanner::SheetDisc::Disc { number: 1 },
        )
        .await
        .unwrap();
    seed_mb_release_with_works(
        &fakes,
        "release-with-works",
        "release-with-works",
        &["First Track", "Second Track", "Third Track"],
        &[111_000, 222_000, 333_000],
    );
    pick_mb(&fixture.handle, &fixture.key, "release-with-works").await;
    let hash = fixture.candidate.files.content_hash();
    (fixture, hash)
}

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
            crate::import::folder_scanner::SheetDisc::Ignored,
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
            crate::import::folder_scanner::SheetDisc::Ignored,
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
        .flat_map(crate::import::MappingTrackSide::mappings)
        .collect();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].duration_ms, Some(111_000));
    let crate::import::MappingSource::File(file) = &rows[1].source else {
        panic!("the gained row is a whole file");
    };
    assert_eq!(rows[1].duration_ms, file.duration_ms);
    assert!(!rows[1].lengths_disagree);
}

/// What the import writes is the draft: storing the release again after it
/// was applied, its tracklist and credits changed by its editors, changes
/// nothing the candidate imports (B2).
#[tokio::test(flavor = "multi_thread")]
async fn a_release_stored_again_after_apply_changes_nothing_the_draft_imports() {
    let fakes = FakeCatalogs::offline();
    let StoredCandidate { handle, candidate, key, tmp: _tmp, .. } =
        stored_candidate_with(fakes.providers()).await;
    let tables = handle.tables.clone();
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
        .catalogs
        .release_read_session(&edited)
        .await
        .unwrap()
        .fetch_payloads(
            &edited,
            bae_catalogs::payloads::ReadDepth::Complete,
            bae_catalogs::rate_limiter::CallPriority::Interactive,
        )
        .await
        .unwrap();
    tables.save_source_release(&payloads.extract().unwrap()).await.unwrap();
    assert_eq!(stored_draft(&handle, &hash).await, applied);

    let mut events = handle.every_event();
    let import_id = handle.start_import(&key).await.unwrap().expect_queued();
    let (release_id, _album_id) =
        await_import_outcome(&mut events, &import_id).await.expect("the import completes");
    let written = handle.database.track_sources_for_test(&release_id).await.unwrap();
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
                vec![crate::import::Credited::plain(crate::import::ArtistAssignment::named(
                    "Typed Artist",
                ))],
            )
            .await
            .unwrap();
        handle
            .set_candidate_edit_field(
                &key,
                crate::import::DraftFieldEdit::Text {
                    field: crate::import::CandidateEditField::AlbumTitle,
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
        let tracks = handle.database.get_tracks_for_release(&release_id).await.unwrap();
        let album = handle
            .database
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

/// The draft keeps its barcode rows as the form holds them, in order, a blank
/// row the person has not filled yet included, and keeps only what the next
/// edit leaves.
#[tokio::test(flavor = "multi_thread")]
async fn the_draft_keeps_its_barcode_rows_as_typed() {
    let (fixture, hash) = three_track_draft().await;
    let set = |barcodes: &[&str]| {
        let barcodes = barcodes.iter().map(|code| code.to_string()).collect();
        let handle = fixture.handle.clone();
        let key = fixture.key.clone();
        async move {
            handle
                .set_candidate_edit_field(&key, crate::import::DraftFieldEdit::Barcodes(barcodes))
                .await
                .unwrap();
        }
    };

    set(&["0123456789012", "", "5012345678900"]).await;
    assert_eq!(
        stored_draft(&fixture.handle, &hash).await.pressing.barcodes,
        ["0123456789012", "", "5012345678900"]
    );

    set(&["5012345678900"]).await;
    assert_eq!(stored_draft(&fixture.handle, &hash).await.pressing.barcodes, ["5012345678900"]);
}

/// The draft keeps its label rows as the form holds them, in order, each field
/// as typed, a blank row the person has not filled yet included, and keeps
/// only what the next edit leaves.
#[tokio::test(flavor = "multi_thread")]
async fn the_draft_keeps_its_label_rows_as_typed() {
    let (fixture, hash) = three_track_draft().await;
    let rows = |labels: &[(&str, &str)]| -> Vec<crate::import::RawLabelEdit> {
        labels
            .iter()
            .map(|(name, catalog_number)| crate::import::RawLabelEdit {
                name: name.to_string(),
                catalog_number: catalog_number.to_string(),
                ..Default::default()
            })
            .collect()
    };
    let set = |labels: Vec<crate::import::RawLabelEdit>| {
        let handle = fixture.handle.clone();
        let key = fixture.key.clone();
        async move {
            handle
                .set_candidate_edit_field(&key, crate::import::DraftFieldEdit::Labels(labels))
                .await
                .unwrap();
        }
    };

    let typed = rows(&[(" Label Name ", "CAT-001"), ("", ""), ("", "CAT 002")]);
    set(typed.clone()).await;
    assert_eq!(stored_draft(&fixture.handle, &hash).await.pressing.labels, typed);

    let kept = rows(&[("Other Label", "")]);
    set(kept.clone()).await;
    assert_eq!(stored_draft(&fixture.handle, &hash).await.pressing.labels, kept);
}
