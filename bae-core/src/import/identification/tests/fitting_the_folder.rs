// ── Fitting the folder's tracks ──────────────────────────────────────────────
//
// A release whose read tracklist holds other tracks than the folder is no
// match: the run leaves it out and reads the next best release instead.

/// The folder's track count breaks a tie the lookups leave: of two pressings
/// a barcode names alike, the one whose tracklist holds as many tracks as the
/// folder is offered, and the run settles on it. The other is no match, so it
/// is not set aside under the offered one either.
#[tokio::test(flavor = "multi_thread")]
async fn the_pressing_whose_tracklist_fits_the_folder_is_offered() {
    let fixture = Fixture::new("fitting-pressing").await;
    fixture
        .import
        .register_artwork_analyzer(Arc::new(BarcodeAnalyzer {
            barcode: PAIRED_BARCODE.to_string(),
        }));
    let dir = fixture.barcode_candidate("From Barcode");
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/release/mb-long?",
        200,
        release_json("mb-long", "rg-fit", &[probed, 0, 1_000]),
    );
    fixture.provider.route(
        "/release/mb-fits?",
        200,
        release_json("mb-fits", "rg-fit", &[probed, 0]),
    );
    fixture.provider.route(
        "/release?",
        200,
        barcode_search_json_placed(&[
            ("mb-long", "rg-fit", PAIRED_BARCODE, "GB"),
            ("mb-fits", "rg-fit", PAIRED_BARCODE, "DE"),
        ]),
    );
    fixture.scan(1).await;

    fixture.drain_automatic().await;

    let row = fixture
        .stored_for(&dir)
        .await
        .expect("the candidate stores a row");
    let verdict = identify_result(&row).verdict.clone();
    let TerminalVerdict::Found {
        findings:
            crate::identify::Findings {
                matches,
                narrowed_out,
                ..
            },
        ..
    } = &verdict
    else {
        panic!("expected a Found verdict, got {verdict:?}");
    };
    assert_eq!(
        matches
            .iter()
            .map(|result| result.release_id.as_str())
            .collect::<Vec<_>>(),
        vec!["mb-fits"]
    );
    assert!(narrowed_out.is_empty(), "{narrowed_out:?}");
    assert_eq!(
        row.metadata_author,
        crate::import::MetadataAuthor::Identification,
        "the one fitting pressing settles the draft"
    );
    assert_eq!(fixture.count_release_lookups("mb-fits"), 1);
}

/// A folder that combines two ten-track albums, whose name writes the first
/// album's title and a catalog number. The barcode returns that album alone,
/// which the folder's name states more of, and the release that holds both
/// albums, whose title the name does not write; both carry the number. The
/// ten-track album ranks first and is read, lists other tracks than the
/// folder, and drops out; the two-album release is read next, fits, and is
/// offered alone and applied.
#[tokio::test(flavor = "multi_thread")]
async fn a_combined_folder_settles_on_the_release_that_holds_all_its_tracks() {
    let fixture = Fixture::new("combined-folder").await;
    fixture
        .import
        .register_artwork_analyzer(Arc::new(BarcodeAnalyzer {
            barcode: PAIRED_BARCODE.to_string(),
        }));
    let dir = fixture.barcode_candidate("Artist - Album One [CAT-100]");
    for at in 3..=20 {
        copy_fixture(
            &Path::new("tests/fixtures/flac").join(FLAC_FIXTURES[at % 2]),
            &dir.join(format!("{at:02} Test Track {at}.flac")),
        );
    }
    let lengths: Vec<u64> = (1..=20)
        .map(|at| {
            let name = match at {
                1 | 2 => FLAC_FIXTURES[at - 1].to_string(),
                _ => format!("{at:02} Test Track {at}.flac"),
            };
            crate::audio_codec::probe_audio_from_path(dir.join(name).to_str().unwrap())
                .expect("fixture FLAC probes")
                .duration
                .as_millis() as u64
        })
        .collect();
    let release = |release_id: &str, group_id: &str, title: &str, media: &[&[u64]]| {
        let media: Vec<serde_json::Value> = media
            .iter()
            .map(|lengths| {
                let tracks: Vec<serde_json::Value> = lengths
                    .iter()
                    .enumerate()
                    .map(|(at, length)| {
                        serde_json::json!({
                            "position": at + 1,
                            "number": (at + 1).to_string(),
                            "title": format!("Track {}", at + 1),
                            "length": length,
                        })
                    })
                    .collect();
                serde_json::json!({ "tracks": tracks })
            })
            .collect();
        serde_json::json!({
            "id": release_id,
            "title": title,
            "artist-credit": [{ "name": "Artist" }],
            "release-group": { "id": group_id },
            "barcode": PAIRED_BARCODE,
            "label-info": [{ "catalog-number": "CAT-100" }],
            "media": media,
            "relations": [],
            "cover-art-archive": { "front": false, "darkened": false },
        })
    };
    fixture.provider.route(
        "/release/mb-album-one?",
        200,
        release("mb-album-one", "rg-album-one", "Album One", &[&lengths[..10]]).to_string(),
    );
    fixture.provider.route(
        "/release/mb-both-albums?",
        200,
        release(
            "mb-both-albums",
            "rg-both-albums",
            "Album One & Album Two",
            &[&lengths[..10], &lengths[10..]],
        )
        .to_string(),
    );
    let found = |release: serde_json::Value| {
        let mut found = release;
        found
            .as_object_mut()
            .expect("a release is an object")
            .retain(|field, _| !matches!(field.as_str(), "media" | "relations"));
        found
    };
    fixture.provider.route(
        "/release?",
        200,
        serde_json::json!({
            "releases": [
                found(release("mb-album-one", "rg-album-one", "Album One", &[])),
                found(release(
                    "mb-both-albums",
                    "rg-both-albums",
                    "Album One & Album Two",
                    &[],
                )),
            ]
        })
        .to_string(),
    );
    fixture.scan(1).await;

    fixture.drain_automatic().await;

    let row = fixture
        .stored_for(&dir)
        .await
        .expect("the candidate stores a row");
    let verdict = identify_result(&row).verdict.clone();
    let TerminalVerdict::Found { findings, .. } = &verdict else {
        panic!("expected a Found verdict, got {verdict:?}");
    };
    assert_eq!(
        findings
            .matches
            .iter()
            .map(|result| result.release_id.as_str())
            .collect::<Vec<_>>(),
        vec!["mb-both-albums"]
    );
    assert!(
        findings.narrowed_out.is_empty(),
        "the ten-track album is no match: {:?}",
        findings.narrowed_out
    );
    assert_eq!(fixture.count_release_lookups("mb-album-one"), 1, "it was read");
    assert_eq!(fixture.judgement_for(&dir).await, (true, None));
    assert_eq!(
        row.metadata_author,
        crate::import::MetadataAuthor::Identification,
        "the release that holds every track settles the draft"
    );
}

/// A sole release listing more tracks than the folder holds is no match: the
/// run reads it, finds nothing left, and the folder is not found.
#[tokio::test(flavor = "multi_thread")]
async fn a_sole_release_that_does_not_fit_the_folder_is_not_found() {
    let fixture = Fixture::new("unfit-sole").await;
    fixture
        .import
        .register_artwork_analyzer(Arc::new(BarcodeAnalyzer {
            barcode: PAIRED_BARCODE.to_string(),
        }));
    let dir = fixture.barcode_candidate("From Barcode");
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/release/mb-long?",
        200,
        release_json("mb-long", "rg-long", &[probed, 0, 1_000]),
    );
    fixture.provider.route(
        "/release?",
        200,
        barcode_search_json(&[("mb-long", "rg-long", PAIRED_BARCODE)]),
    );
    fixture.scan(1).await;

    fixture.drain_automatic().await;

    let row = fixture
        .stored_for(&dir)
        .await
        .expect("the candidate stores a row");
    let verdict = identify_result(&row).verdict.clone();
    assert!(
        matches!(verdict, TerminalVerdict::NotFoundAnywhere { .. }),
        "a release that does not fit is no match, got {verdict:?}"
    );
    assert_eq!(fixture.count_release_lookups("mb-long"), 1, "it was read");
    assert_ne!(
        row.metadata_author,
        crate::import::MetadataAuthor::Identification,
        "nothing is applied"
    );
}
