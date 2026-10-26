use super::*;

/// A stored verdict reads back exactly, provenance and ledger included.
#[tokio::test]
async fn round_trip_preserves_the_verdict_including_provenance() {
    let (db, _tmp) = empty_db().await;
    let candidate =
        track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
    let hash = candidate.content_hash();
    let verdict = sample_verdict();
    let row = new_candidate_row(&hash, &host_root("/music/Some Album"), &verdict);
    store_candidate_state(&db, &candidate, &row.folder_path).await;

    crate::import::CandidatePreparations::new(db.clone())
        .store_verdict(&row)
        .await
        .unwrap();

    let loaded = db.load_import_candidate_states().await.unwrap();
    let loaded_row = loaded
        .get(&hash)
        .expect("row present under its content hash");
    assert_eq!(loaded_row.folder_path, host_root("/music/Some Album"));
    let identify = loaded_row
        .identify
        .as_ref()
        .expect("a stored verdict reads back as an identify result");
    // The write stamps it from the injected clock.
    assert_eq!(identify.identified_at, fixed_now());
    assert_eq!(
        identify.verdict, verdict,
        "the verdict must round-trip exactly, provenance included"
    );
}

/// Everything the rows and cards are grouped by reads back, so a stored verdict
/// groups as its run did.
#[tokio::test]
async fn round_trip_preserves_the_evidence_the_rows_are_paired_by() {
    use crate::import::album_links::{AlbumLink, AlbumLinks, AlbumStatement};
    use crate::import::MetadataRef;
    use crate::catalog::Catalog;
    use crate::pressing::{
        DiscogsDetail, Medium, Packaging, Region, ReleaseArea, ReleaseStatus, StatedFormat,
        StatedMedia,
    };

    let (db, _tmp) = empty_db().await;
    let candidate =
        track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
    let hash = candidate.content_hash();
    let mut musicbrainz = sample_match();
    musicbrainz.barcodes = vec!["012345678905".to_string()];
    musicbrainz.media = StatedMedia::PerMedium(vec![Some(Medium::Cd), None]);
    musicbrainz.status = Some(ReleaseStatus::Official);
    musicbrainz.packaging = Some(Packaging::JewelCase);
    musicbrainz.links = vec![
        MetadataRef::new(Catalog::Discogs, "42"),
        MetadataRef::new(Catalog::Discogs, "43"),
    ];
    let album = AlbumLinks::Read(vec![
        AlbumLink {
            album: MetadataRef::new(Catalog::Discogs, "7"),
            stated: AlbumStatement::Page,
        },
        AlbumLink {
            album: MetadataRef::new(Catalog::Discogs, "8"),
            stated: AlbumStatement::Wikidata {
                item: "Q1".to_string(),
            },
        },
        AlbumLink {
            album: MetadataRef::new(Catalog::Discogs, "9"),
            stated: AlbumStatement::Release {
                musicbrainz_release: "rel-1".to_string(),
                twin: MetadataRef::new(Catalog::Discogs, "44"),
            },
        },
        AlbumLink {
            album: MetadataRef::new(Catalog::Discogs, "10"),
            stated: AlbumStatement::Barcode {
                musicbrainz_release: "rel-1".to_string(),
                release: MetadataRef::new(Catalog::Discogs, "45"),
            },
        },
        AlbumLink {
            album: MetadataRef::new(Catalog::Discogs, "11"),
            stated: AlbumStatement::CatalogNumber {
                musicbrainz_release: "rel-1".to_string(),
                release: MetadataRef::new(Catalog::Discogs, "46"),
            },
        },
    ]);
    musicbrainz.album_links = album.clone();
    let mut discogs = sample_match();
    discogs.source = Catalog::Discogs;
    discogs.release_id = "42".to_string();
    discogs.source_group_id = Some("7".to_string());
    discogs.barcodes = vec!["0 12345 67890 5".to_string(), "5051961234567".to_string()];
    discogs.media = StatedMedia::Formats(vec![
        StatedFormat {
            medium: Some(Medium::Cd),
            quantity: 2,
        },
        StatedFormat {
            medium: None,
            quantity: 1,
        },
    ]);
    discogs.area = Some(ReleaseArea::Region(Region::UkAndEurope));
    discogs.status = Some(ReleaseStatus::Promotion);
    discogs.discogs_details = vec![DiscogsDetail::Reissue, DiscogsDetail::Size12In];
    // A cover with every copy the archive serves, one with Discogs's one
    // thumbnail, and one served at its one size.
    musicbrainz.cover_art = Some(crate::remote_image::RemoteCover::musicbrainz_release(
        "rel-1",
    ));
    discogs.cover_art = crate::discogs::remote_cover_from_urls(
        Some("https://images.example/front.jpg"),
        Some("https://images.example/front-150.jpg"),
        "release",
        42,
    );
    let mut undescribed = sample_match();
    undescribed.cover_art = crate::discogs::remote_cover_from_urls(
        Some("https://images.example/only.jpg"),
        None,
        "release",
        2,
    );
    undescribed.release_id = "rel-2".to_string();
    undescribed.year = Some(2001);
    undescribed.media = StatedMedia::Undescribed;
    undescribed.album_links = album;
    let mut unread = sample_match();
    unread.release_id = "rel-3".to_string();
    unread.source_group_id = Some("group-2".to_string());
    unread.album_links = AlbumLinks::Unread;
    let mut linked = sample_match();
    linked.source = Catalog::Discogs;
    linked.release_id = "44".to_string();
    linked.source_group_id = Some("7".to_string());
    let matches = vec![musicbrainz, discogs, undescribed, unread, linked];
    let returned = LookupProvenance {
        by_disc_id: true,
        ..LookupProvenance::CHOSEN
    };
    let verdict = TerminalVerdict::Found {
        findings: Findings {
            provenance: vec![
                returned.clone(),
                returned.clone(),
                returned.clone(),
                returned,
                LookupProvenance {
                    by_pressing: true,
                    ..LookupProvenance::CHOSEN
                },
            ],
            pressings: crate::import::release_group::form_rows(&matches),
            matches: matches.clone(),
            narrowed_out: NarrowedOut::default(),
            medium_conflict: None,
            named_notes: Vec::new(),
        },
        track_count: 11,
        ledger: None,
    };
    let row = new_candidate_row(&hash, &host_root("/music/Some Album"), &verdict);
    store_candidate_state(&db, &candidate, &row.folder_path).await;
    crate::import::CandidatePreparations::new(db.clone())
        .store_verdict(&row)
        .await
        .unwrap();

    let loaded = db.load_import_candidate_states().await.unwrap();
    let stored = &loaded[&hash]
        .identify
        .as_ref()
        .expect("a stored verdict reads back")
        .verdict;
    assert_eq!(*stored, verdict);
    let TerminalVerdict::Found {
        findings: Findings {
            matches: stored_matches,
            ..
        },
        ..
    } = stored
    else {
        panic!("the verdict found releases");
    };
    let live = crate::import::release_group::group_results(
        crate::import::release_group::unranked(matches.clone()),
        None,
    );
    let replayed = crate::import::release_group::group_results(
        crate::import::release_group::unranked(stored_matches.clone()),
        None,
    );
    assert_eq!(replayed, live);
    assert_eq!(
        live.len(),
        2,
        "the pair joins the two groups on one card; the unread group is its own"
    );
    assert_eq!(
        live[0]
            .pressings()
            .map(|pressing| pressing.releases.len())
            .collect::<Vec<_>>(),
        vec![2, 1, 1],
        "the linked pair is one row, the others their own"
    );
    assert_eq!(crate::import::release_group::pressing_count(matches), 4);
}

/// The candidate's text reads back whole, line by line, in order and with the
/// surface each was read off.
#[tokio::test]
async fn the_candidate_s_text_round_trips_line_by_line() {
    let (db, _tmp) = empty_db().await;
    let candidate =
        track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
    let hash = candidate.content_hash();
    let pool = vec![
        crate::signals::TextLine {
            text: "AC-DC - Dirty Deeds Done Dirt Cheap [16033-2]".to_string(),
            origin: crate::signals::TextOrigin::FolderName,
        },
        crate::signals::TextLine {
            text: "Atlantic Records, Inc.".to_string(),
            origin: crate::signals::TextOrigin::Artwork,
        },
    ];
    let mut row = new_candidate_row(&hash, &host_root("/music/Some Album"), &sample_verdict());
    row.signals.text_pool = pool.clone();
    store_candidate_state(&db, &candidate, &row.folder_path).await;

    crate::import::CandidatePreparations::new(db.clone())
        .store_verdict(&row)
        .await
        .unwrap();

    let loaded = db.load_import_candidate_states().await.unwrap();
    let stored = loaded
        .get(&hash)
        .expect("row present under its content hash")
        .signals
        .as_ref()
        .expect("a stored verdict reads its signals back");
    assert_eq!(stored.text_pool, pool);
}

/// A failed verdict reads back with what its answering lookups found.
#[tokio::test]
async fn a_failed_verdict_round_trips_what_its_answering_lookups_found() {
    let (db, _tmp) = empty_db().await;
    let candidate =
        track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
    let hash = candidate.content_hash();
    let verdict = TerminalVerdict::Failed {
        failures: vec![crate::identify::IdentifyFailure::Search(
            crate::import::search::SourceFailure {
                source: crate::catalog::Catalog::MusicBrainz,
                failure: crate::signals::LookupFailure::Provider { status: Some(503) },
            },
        )],
        findings: sample_findings(),
        track_count: 11,
        ledger: Some(sample_ledger()),
    };
    let row = new_candidate_row(&hash, &host_root("/music/Some Album"), &verdict);
    store_candidate_state(&db, &candidate, &row.folder_path).await;
    crate::import::CandidatePreparations::new(db.clone())
        .store_verdict(&row)
        .await
        .unwrap();

    let loaded = db.load_import_candidate_states().await.unwrap();
    let identify = loaded
        .get(&hash)
        .expect("row present under its content hash")
        .identify
        .as_ref()
        .expect("a stored verdict reads back as an identify result");
    assert_eq!(identify.verdict, verdict);
}

/// A run that ended because bae broke reads back with why.
#[tokio::test]
async fn an_error_verdict_round_trips_its_failure() {
    let (db, _tmp) = empty_db().await;
    let candidate =
        track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
    let hash = candidate.content_hash();
    let failure = crate::signals::InternalFailure {
        detail: "checking the library: the store is locked".to_string(),
    };
    let verdict = TerminalVerdict::Error { failure };
    let row = new_candidate_row(&hash, &host_root("/music/Some Album"), &verdict);
    store_candidate_state(&db, &candidate, &row.folder_path).await;
    crate::import::CandidatePreparations::new(db.clone())
        .store_verdict(&row)
        .await
        .unwrap();

    let loaded = db.load_import_candidate_states().await.unwrap();
    let identify = loaded
        .get(&hash)
        .expect("row present under its content hash")
        .identify
        .as_ref()
        .expect("a stored verdict reads back as an identify result");
    assert_eq!(identify.verdict, verdict);
}

/// A verdict recorded with no ledger reads back with none.
#[tokio::test]
async fn a_verdict_with_no_ledger_reads_back_without_one() {
    let (db, _tmp) = empty_db().await;
    let candidate =
        track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
    let hash = candidate.content_hash();
    let verdict = TerminalVerdict::Found {
        findings: sample_findings(),
        track_count: 11,
        ledger: None,
    };
    let row = new_candidate_row(&hash, &host_root("/music/Some Album"), &verdict);
    store_candidate_state(&db, &candidate, &row.folder_path).await;

    crate::import::CandidatePreparations::new(db.clone())
        .store_verdict(&row)
        .await
        .unwrap();

    let loaded = db.load_import_candidate_states().await.unwrap();
    let identify = loaded
        .get(&hash)
        .expect("row present under its content hash")
        .identify
        .as_ref()
        .expect("a stored verdict reads back as an identify result");
    assert_eq!(identify.verdict, verdict);
}

/// Narrowed-out releases read back apart from the matches.
#[tokio::test]
async fn a_verdict_round_trips_its_narrowed_out_releases_apart_from_its_matches() {
    let (db, _tmp) = empty_db().await;
    let candidate =
        track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
    let hash = candidate.content_hash();
    let mut left_out = sample_match();
    left_out.release_id = "rel-narrowed".to_string();
    let verdict = TerminalVerdict::Found {
        findings: Findings {
            narrowed_out: NarrowedOut {
                matches: vec![left_out],
                provenance: vec![LookupProvenance {
                    by_disc_id: true,
                    by_barcode: false,
                    by_catalog: false,
                    by_isrc: false,
                    by_search: false,
                    by_pressing: false,
                }],
                pressings: vec![0],
            },
            ..sample_findings()
        },
        track_count: 11,
        ledger: Some(sample_ledger()),
    };
    let row = new_candidate_row(&hash, &host_root("/music/Some Album"), &verdict);
    store_candidate_state(&db, &candidate, &row.folder_path).await;

    crate::import::CandidatePreparations::new(db.clone())
        .store_verdict(&row)
        .await
        .unwrap();

    let loaded = db.load_import_candidate_states().await.unwrap();
    let identify = loaded
        .get(&hash)
        .expect("row present under its content hash")
        .identify
        .as_ref()
        .expect("a stored verdict reads back as an identify result");
    assert_eq!(identify.verdict, verdict);
    let TerminalVerdict::Found {
        findings: Findings {
            matches,
            narrowed_out,
            ..
        },
        ..
    } = &identify.verdict
    else {
        panic!("a found verdict reads back as one");
    };
    assert_eq!(
        matches
            .iter()
            .map(|result| result.release_id.as_str())
            .collect::<Vec<_>>(),
        vec!["rel-1"]
    );
    assert_eq!(
        narrowed_out
            .matches
            .iter()
            .map(|result| result.release_id.as_str())
            .collect::<Vec<_>>(),
        vec!["rel-narrowed"]
    );
}
