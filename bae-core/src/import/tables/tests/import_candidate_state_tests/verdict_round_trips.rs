use super::*;
use bae_catalogs::catalog::LookupCatalog;

/// A stored verdict reads back exactly, its ledger included.
#[tokio::test]
async fn round_trip_preserves_the_verdict_and_its_ledger() {
    let (db, _tmp) = empty_db().await;
    let tables = crate::import::tables::ImportTables::new(db.clone());
    let candidate =
        track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
    let hash = candidate.content_hash();
    let verdict = sample_verdict();
    let row = new_candidate_row(&hash, &host_root("/music/Some Album"), &verdict);
    store_candidate_state(&db, &candidate, &row.candidate_key.test_folder()).await;

    crate::import::CandidatePreparations::new(db.clone()).store_verdict(&row).await.unwrap();

    let loaded = tables.load_import_candidate_states().await.unwrap();
    let loaded_row = loaded.get(&hash).expect("row present under its content hash");
    assert_eq!(
        loaded_row.candidate_key,
        crate::import::CandidateKey::folder(&host_root("/music/Some Album"))
    );
    let identify =
        loaded_row.identify.as_ref().expect("a stored verdict reads back as an identify result");
    assert_eq!(
        identify.verdict, verdict,
        "the verdict must round-trip exactly, its ledger included"
    );
}

/// A stored verdict's match reads its album's first year from the catalog
/// release it names, as that release is stored now: none while it is not
/// stored, whatever the run read, and a later fetch's once one stores it.
#[tokio::test]
async fn a_match_reads_its_album_first_year_from_its_stored_catalog_release() {
    let (db, _tmp) = empty_db().await;
    let tables = crate::import::tables::ImportTables::new(db.clone());
    let candidate =
        track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
    let hash = candidate.content_hash();
    let mut findings = sample_findings();
    findings.matches[0].album_first_year = Some(1998);
    let verdict = TerminalVerdict::Found { findings, track_count: 11, ledger: None };
    let row = new_candidate_row(&hash, &host_root("/music/Some Album"), &verdict);
    store_candidate_state(&db, &candidate, &row.candidate_key.test_folder()).await;
    crate::import::CandidatePreparations::new(db.clone()).store_verdict(&row).await.unwrap();
    let first_year = || async {
        let loaded = tables.load_import_candidate_states().await.unwrap();
        let TerminalVerdict::Found { findings, .. } =
            &loaded[&hash].identify.as_ref().expect("a stored verdict reads back").verdict
        else {
            panic!("the stored verdict reads back as found");
        };
        findings.matches[0].album_first_year
    };
    assert_eq!(first_year().await, None);

    for year in [1998, 2001] {
        let mut release = fetched_release("rel-1");
        release.metadata.album.first_year = Some(year);
        release.metadata.album.year = Some(year);
        tables.save_source_release(&release).await.unwrap();
        assert_eq!(first_year().await, Some(year));
    }
}

/// Only a run's ranking reads a release's track titles and notes, so a stored
/// verdict keeps neither: its releases read back without them.
#[tokio::test]
async fn a_stored_verdict_keeps_no_track_titles_or_notes() {
    let read_back = TerminalVerdict::Found {
        findings: Findings { named_notes: Vec::new(), ..sample_findings() },
        track_count: 11,
        ledger: None,
    };
    let mut live = read_back.clone();
    let TerminalVerdict::Found { findings, .. } = &mut live else {
        unreachable!("built as a found verdict");
    };
    findings.matches[0].track_titles = vec!["Track One".to_string(), "Track Two".to_string()];
    findings.matches[0].notes = vec!["Pressed By Plant Name".to_string()];

    let (read, _) = stored_and_read(&live).await;
    assert_eq!(read, read_back);
}

/// Everything the rows and cards are grouped by reads back, so a stored verdict
/// groups as its run did.
#[tokio::test]
async fn round_trip_preserves_the_evidence_the_rows_are_paired_by() {
    use bae_catalogs::album_links::AlbumLinks;
    use bae_catalogs::records::MetadataRef;
    use bae_catalogs::catalog::Catalog;
    use bae_types::pressing::{
        DiscogsDetail, Medium, Packaging, Region, ReleaseArea, ReleaseStatus, StatedFormat,
        StatedMedia,
    };

    let (db, _tmp) = empty_db().await;
    let tables = crate::import::tables::ImportTables::new(db.clone());
    let candidate =
        track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
    let hash = candidate.content_hash();
    let mut musicbrainz = sample_match();
    musicbrainz.barcodes = vec!["012345678905".to_string()];
    musicbrainz.labels = vec![
        bae_types::pressing::ReleaseLabel::of(Some("Label Name"), Some("CAT-1")),
        bae_types::pressing::ReleaseLabel::of(Some("Second Label"), None),
        bae_types::pressing::ReleaseLabel::of(None, Some("CAT-2")),
    ];
    musicbrainz.media = StatedMedia::PerMedium(vec![Some(Medium::Cd), None]);
    musicbrainz.status = Some(ReleaseStatus::Official);
    musicbrainz.packaging = Some(Packaging::JewelCase);
    musicbrainz.links =
        vec![MetadataRef::new(Catalog::Discogs, "42"), MetadataRef::new(Catalog::Discogs, "43")];
    let album = AlbumLinks::Read(vec![
        MetadataRef::new(Catalog::Discogs, "9"),
        MetadataRef::new(Catalog::Discogs, "7"),
        MetadataRef::new(Catalog::Discogs, "8"),
    ]);
    musicbrainz.album_links = album.clone();
    let mut discogs = sample_match();
    discogs.source = LookupCatalog::Discogs;
    discogs.release_id = "42".to_string();
    discogs.source_group_id = Some("7".to_string());
    discogs.barcodes = vec!["0 12345 67890 5".to_string(), "5051961234567".to_string()];
    discogs.media = StatedMedia::Formats(vec![
        StatedFormat { medium: Some(Medium::Cd), quantity: 2 },
        StatedFormat { medium: None, quantity: 1 },
    ]);
    discogs.area = Some(ReleaseArea::Region(Region::UkAndEurope));
    discogs.status = Some(ReleaseStatus::Promotion);
    discogs.discogs_details = vec![DiscogsDetail::Reissue, DiscogsDetail::Size12In];
    // A cover with every copy the archive serves, one with Discogs's one
    // thumbnail, and one served at its one size.
    musicbrainz.cover_art =
        Some(bae_catalogs::remote_image::RemoteCover::musicbrainz_release("rel-1"));
    discogs.cover_art = bae_catalogs::discogs::remote_cover_from_urls(
        Some("https://images.example/front.jpg"),
        Some("https://images.example/front-150.jpg"),
        "release",
        42,
    );
    let mut undescribed = sample_match();
    undescribed.cover_art = bae_catalogs::discogs::remote_cover_from_urls(
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
    linked.source = LookupCatalog::Discogs;
    linked.release_id = "44".to_string();
    linked.source_group_id = Some("7".to_string());
    let matches = vec![musicbrainz, discogs, undescribed, unread, linked];
    let verdict = TerminalVerdict::Found {
        findings: Findings {
            pressings: bae_identify::release_group::form_rows(&matches),
            matches: matches.clone(),
            narrowed_out: NarrowedOut::default(),
            medium_conflict: None,
            named_notes: Vec::new(),
        },
        track_count: 11,
        ledger: None,
    };
    let row = new_candidate_row(&hash, &host_root("/music/Some Album"), &verdict);
    store_candidate_state(&db, &candidate, &row.candidate_key.test_folder()).await;
    crate::import::CandidatePreparations::new(db.clone()).store_verdict(&row).await.unwrap();

    let loaded = tables.load_import_candidate_states().await.unwrap();
    let stored = &loaded[&hash].identify.as_ref().expect("a stored verdict reads back").verdict;
    assert_eq!(*stored, verdict);
    let TerminalVerdict::Found { findings: Findings { matches: stored_matches, .. }, .. } = stored
    else {
        panic!("the verdict found releases");
    };
    let live = bae_identify::release_group::group_results(
        bae_identify::release_group::unranked(matches.clone()),
        None,
    );
    let replayed = bae_identify::release_group::group_results(
        bae_identify::release_group::unranked(stored_matches.clone()),
        None,
    );
    assert_eq!(replayed, live);
    assert_eq!(
        live.len(),
        2,
        "the pair joins the two groups on one card; the unread group is its own"
    );
    assert_eq!(
        live[0].pressings().map(|pressing| pressing.releases.len()).collect::<Vec<_>>(),
        vec![2, 1, 1],
        "the linked pair is one row, the others their own"
    );
    assert_eq!(bae_identify::release_group::pressing_count(matches), 4);
}

/// The candidate's text reads back whole, line by line, in order and with the
/// surface each was read off.
#[tokio::test]
async fn the_candidate_s_text_round_trips_line_by_line() {
    let (db, _tmp) = empty_db().await;
    let tables = crate::import::tables::ImportTables::new(db.clone());
    let candidate =
        track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
    let hash = candidate.content_hash();
    let pool = vec![
        bae_identify::signals::TextLine {
            text: "AC-DC - Dirty Deeds Done Dirt Cheap [16033-2]".to_string(),
            origin: bae_identify::signals::TextOrigin::FolderName,
        },
        bae_identify::signals::TextLine {
            text: "Atlantic Records, Inc.".to_string(),
            origin: bae_identify::signals::TextOrigin::Artwork,
        },
    ];
    let mut row = new_candidate_row(&hash, &host_root("/music/Some Album"), &sample_verdict());
    row.signals.text_pool = pool.clone();
    store_candidate_state(&db, &candidate, &row.candidate_key.test_folder()).await;

    crate::import::CandidatePreparations::new(db.clone()).store_verdict(&row).await.unwrap();

    let loaded = tables.load_import_candidate_states().await.unwrap();
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
    let tables = crate::import::tables::ImportTables::new(db.clone());
    let candidate =
        track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
    let hash = candidate.content_hash();
    let verdict = TerminalVerdict::Failed {
        failures: vec![bae_identify::IdentifyFailure::Search(
            bae_catalogs::search::SourceFailure {
                source: bae_catalogs::catalog::Catalog::MusicBrainz,
                failure: bae_catalogs::LookupFailure::Provider { status: Some(503) },
            },
        )],
        findings: sample_findings(),
        track_count: 11,
        ledger: Some(sample_ledger()),
    };
    let row = new_candidate_row(&hash, &host_root("/music/Some Album"), &verdict);
    store_candidate_state(&db, &candidate, &row.candidate_key.test_folder()).await;
    crate::import::CandidatePreparations::new(db.clone()).store_verdict(&row).await.unwrap();

    let loaded = tables.load_import_candidate_states().await.unwrap();
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
    let tables = crate::import::tables::ImportTables::new(db.clone());
    let candidate =
        track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
    let hash = candidate.content_hash();
    let failure = bae_catalogs::InternalFailure {
        detail: "checking the library: the store is locked".to_string(),
    };
    let verdict = TerminalVerdict::Error { failure };
    let row = new_candidate_row(&hash, &host_root("/music/Some Album"), &verdict);
    store_candidate_state(&db, &candidate, &row.candidate_key.test_folder()).await;
    crate::import::CandidatePreparations::new(db.clone()).store_verdict(&row).await.unwrap();

    let loaded = tables.load_import_candidate_states().await.unwrap();
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
    let tables = crate::import::tables::ImportTables::new(db.clone());
    let candidate =
        track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
    let hash = candidate.content_hash();
    let verdict =
        TerminalVerdict::Found { findings: sample_findings(), track_count: 11, ledger: None };
    let row = new_candidate_row(&hash, &host_root("/music/Some Album"), &verdict);
    store_candidate_state(&db, &candidate, &row.candidate_key.test_folder()).await;

    crate::import::CandidatePreparations::new(db.clone()).store_verdict(&row).await.unwrap();

    let loaded = tables.load_import_candidate_states().await.unwrap();
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
    let tables = crate::import::tables::ImportTables::new(db.clone());
    let candidate =
        track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
    let hash = candidate.content_hash();
    let mut left_out = sample_match();
    left_out.release_id = "rel-narrowed".to_string();
    let verdict = TerminalVerdict::Found {
        findings: Findings {
            narrowed_out: NarrowedOut { matches: vec![left_out], pressings: vec![0] },
            ..sample_findings()
        },
        track_count: 11,
        ledger: Some(sample_ledger()),
    };
    let row = new_candidate_row(&hash, &host_root("/music/Some Album"), &verdict);
    store_candidate_state(&db, &candidate, &row.candidate_key.test_folder()).await;

    crate::import::CandidatePreparations::new(db.clone()).store_verdict(&row).await.unwrap();

    let loaded = tables.load_import_candidate_states().await.unwrap();
    let identify = loaded
        .get(&hash)
        .expect("row present under its content hash")
        .identify
        .as_ref()
        .expect("a stored verdict reads back as an identify result");
    assert_eq!(identify.verdict, verdict);
    let TerminalVerdict::Found { findings: Findings { matches, narrowed_out, .. }, .. } =
        &identify.verdict
    else {
        panic!("a found verdict reads back as one");
    };
    assert_eq!(
        matches.iter().map(|result| result.release_id.as_str()).collect::<Vec<_>>(),
        vec!["rel-1"]
    );
    assert_eq!(
        narrowed_out.matches.iter().map(|result| result.release_id.as_str()).collect::<Vec<_>>(),
        vec!["rel-narrowed"]
    );
}

/// `release` as one catalog's record of its own.
fn release_on(source: bae_catalogs::catalog::LookupCatalog, release_id: &str) -> MetadataResult {
    MetadataResult {
        source,
        release_id: release_id.to_string(),
        source_group_id: Some(format!("group-{release_id}")),
        ..sample_match()
    }
}

/// Store `verdict` for a fresh candidate and read it back, with the
/// listing of each match row in position order.
async fn stored_and_read(verdict: &TerminalVerdict) -> (TerminalVerdict, Vec<String>) {
    let (db, _tmp) = empty_db().await;
    let tables = crate::import::tables::ImportTables::new(db.clone());
    let candidate =
        track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
    let hash = candidate.content_hash();
    let row = new_candidate_row(&hash, &host_root("/music/Some Album"), verdict);
    store_candidate_state(&db, &candidate, &row.candidate_key.test_folder()).await;
    crate::import::CandidatePreparations::new(db.clone()).store_verdict(&row).await.unwrap();
    let loaded = tables.load_import_candidate_states().await.unwrap();
    let read =
        loaded[&hash].identify.as_ref().expect("a stored verdict reads back").verdict.clone();
    let listings = db
        .read(move |sql| {
            sql.query(
                "SELECT release_id || ' ' || listing FROM import_candidate_match \
                 WHERE content_hash = ? ORDER BY position",
                [hash],
                |row| row.get::<_, String>(0),
            )
            .map_err(super::super::super::DbError::from)
        })
        .await
        .unwrap();
    (read, listings)
}

/// Every cell of the band reads back as the run left it: found, no match,
/// failed and not asked, each clue's rows in order with the values left out,
/// the catalog numbers offered and the ISRCs. A found cell lists every
/// release it found — one the run offers, one agreement set aside, and one
/// listed in neither, which only its cell shows and counts.
#[tokio::test]
async fn the_ledger_round_trips_every_cell_and_every_release_it_found() {
    use bae_identify::{
        BarcodeStepView, CatalogCandidateView, CatalogStepView, DiscIdRow, IsrcStepView,
        NotAskedReason, ProviderCell, SearchStepView, SignalValueRow,
    };
    use bae_catalogs::LookupFailure;
    

    let offered = release_on(LookupCatalog::MusicBrainz, "rel-offered");
    let catalogued = release_on(LookupCatalog::Discogs, "42");
    let set_aside = release_on(LookupCatalog::MusicBrainz, "rel-set-aside");
    let ruled_out = MetadataResult {
        source_tracks: Some(bae_catalogs::search::SourceTracks::Listed { count: 9 }),
        ..release_on(LookupCatalog::MusicBrainz, "rel-ruled-out")
    };
    let found = |releases: &[&MetadataResult]| {
        LookupView::answered(&releases.iter().map(|release| (*release).clone()).collect())
    };
    let left_out = LookupView::NotAsked { reason: NotAskedReason::LeftOut };
    let cell = |source, lookup| ProviderCell { source, lookup };
    let ledger = IdentifyRunView {
        providers: vec![LookupCatalog::MusicBrainz, LookupCatalog::Discogs],
        disc_id: DiscIdStepView::Read {
            discs: vec![
                DiscIdRow { disc_id: "disc-1".to_string(), lookup: found(&[&offered, &set_aside]) },
                DiscIdRow {
                    disc_id: "disc-2".to_string(),
                    lookup: LookupView::Failed { failure: LookupFailure::Timeout },
                },
            ],
        },
        barcode: BarcodeStepView::Rows {
            scanning: false,
            rows: vec![
                SignalValueRow {
                    value: "012345678905".to_string(),
                    excluded: false,
                    cells: vec![
                        cell(LookupCatalog::MusicBrainz, found(&[&ruled_out, &offered])),
                        cell(
                            LookupCatalog::Discogs,
                            LookupView::Failed {
                                failure: LookupFailure::Provider { status: Some(503) },
                            },
                        ),
                    ],
                },
                SignalValueRow {
                    value: "5051961234567".to_string(),
                    excluded: true,
                    cells: vec![
                        cell(LookupCatalog::MusicBrainz, left_out.clone()),
                        cell(LookupCatalog::Discogs, left_out.clone()),
                    ],
                },
            ],
        },
        catalog: CatalogStepView::Numbers {
            scanning: false,
            rows: vec![
                SignalValueRow {
                    value: "CAT-1".to_string(),
                    excluded: false,
                    cells: vec![
                        cell(LookupCatalog::MusicBrainz, LookupView::NoMatch),
                        cell(LookupCatalog::Discogs, found(&[&catalogued])),
                    ],
                },
                SignalValueRow {
                    value: "CAT-2".to_string(),
                    excluded: true,
                    cells: vec![
                        cell(LookupCatalog::MusicBrainz, left_out.clone()),
                        cell(LookupCatalog::Discogs, left_out),
                    ],
                },
            ],
            candidates: vec![CatalogCandidateView { value: "CAT-9".to_string() }],
        },
        isrc: IsrcStepView::Read {
            isrcs: vec!["USAAA0000001".to_string(), "USAAA0000002".to_string()],
            lookup: LookupView::NotAsked { reason: NotAskedReason::NoCatalog },
        },
        search: SearchStepView::NotNeeded,
    };
    let verdict = TerminalVerdict::Failed {
        failures: vec![bae_identify::IdentifyFailure::DiscId(LookupFailure::Timeout)],
        findings: Findings {
            matches: vec![offered.clone(), catalogued.clone()],
            pressings: vec![0, 1],
            narrowed_out: NarrowedOut { matches: vec![set_aside.clone()], pressings: vec![0] },
            medium_conflict: None,
            named_notes: Vec::new(),
        },
        track_count: 11,
        ledger: Some(ledger),
    };

    let (read, listings) = stored_and_read(&verdict).await;

    assert_eq!(read, verdict);
    // The Disc ID agreement is read from the stored ledger: the releases a
    // disc ID's lookup found agree on it, and no other.
    let bae_identify::IdentifyStateView::Failed { agreements, .. } =
        bae_identify::IdentifyStateView::from(
            read.clone().resume_state(Default::default(), Default::default()),
        )
    else {
        panic!("a failed verdict resumes as Failed");
    };
    let by_disc_id = |id: &str| {
        agreements
            .iter()
            .find(|(release, _)| release == id)
            .unwrap_or_else(|| panic!("{id} has badges: {agreements:?}"))
            .1
            .fields
            .disc_id
    };
    assert!(by_disc_id("rel-offered"));
    assert!(by_disc_id("rel-set-aside"));
    assert!(!by_disc_id("42"), "only the catalog number found it");
    assert_eq!(
        listings,
        vec![
            "rel-offered offered",
            "42 offered",
            "rel-set-aside narrowed_out",
            "rel-ruled-out unlisted",
        ],
        "each release is stored once, the one no list holds after the listed ones"
    );
    let BarcodeStepView::Rows { rows, .. } = &read.ledger().expect("a ledger").barcode else {
        panic!("the barcode step read rows");
    };
    let LookupView::Found { count, .. } = &rows[0].cells[0].lookup else {
        panic!("the barcode's MusicBrainz lookup found releases");
    };
    assert_eq!(*count, 2, "the release no list holds still counts in its cell");
    assert!(
        rows[0].cells[0]
            .lookup
            .releases()
            .iter()
            .any(|release| release.release_id == "rel-ruled-out"),
        "and its cell lists it"
    );
    assert!(
        !read.named_releases().iter().any(|release| release.release_id == "rel-ruled-out"),
        "but the verdict names it nowhere else"
    );
}

/// How each clue stood reads back for every state but rows of lookups:
/// a sheet no CD plays, a barcode source with no code, no catalog number, no
/// ISRC and no title; and a title search reads back its words and its cells.
#[tokio::test]
async fn the_ledger_round_trips_how_each_clue_stood() {
    use bae_identify::{
        BarcodeStepView, CatalogStepView, IsrcStepView, ProviderCell, SearchStepView,
    };
    

    let quiet = IdentifyRunView {
        providers: vec![LookupCatalog::MusicBrainz],
        disc_id: DiscIdStepView::NotCdAudio,
        barcode: BarcodeStepView::NoCodes,
        catalog: CatalogStepView::NoneFound,
        isrc: IsrcStepView::Absent,
        search: SearchStepView::NoTitle,
    };
    let verdict = TerminalVerdict::ManualOnly { track_count: 11, ledger: Some(quiet.clone()) };
    assert_eq!(stored_and_read(&verdict).await.0, verdict);

    let searched = release_on(LookupCatalog::Discogs, "77");
    let verdict = TerminalVerdict::Found {
        findings: Findings {
            matches: vec![searched.clone()],
            pressings: vec![0],
            ..Findings::default()
        },
        track_count: 11,
        ledger: Some(IdentifyRunView {
            providers: vec![LookupCatalog::MusicBrainz, LookupCatalog::Discogs],
            disc_id: DiscIdStepView::Absent,
            barcode: BarcodeStepView::Absent,
            search: SearchStepView::Searched {
                album: "Album".to_string(),
                artist: String::new(),
                cells: vec![
                    ProviderCell { source: LookupCatalog::MusicBrainz, lookup: LookupView::NoMatch },
                    ProviderCell {
                        source: LookupCatalog::Discogs,
                        lookup: LookupView::answered(&vec![searched]),
                    },
                ],
            },
            ..quiet
        }),
    };
    let (read, listings) = stored_and_read(&verdict).await;
    assert_eq!(read, verdict);
    assert_eq!(listings, vec!["77 offered"]);
}

/// A ledger a settled run cannot have recorded is refused: a lookup still
/// under way.
#[tokio::test]
async fn a_ledger_with_a_lookup_under_way_is_not_stored() {
    let (db, _tmp) = empty_db().await;
    let candidate =
        track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
    let hash = candidate.content_hash();
    let mut ledger = sample_ledger();
    ledger.disc_id = DiscIdStepView::Read {
        discs: vec![bae_identify::DiscIdRow {
            disc_id: "disc-1".to_string(),
            lookup: LookupView::LookingUp,
        }],
    };
    let verdict = TerminalVerdict::NotFoundAnywhere { ledger: Some(ledger) };
    let row = new_candidate_row(&hash, &host_root("/music/Some Album"), &verdict);
    store_candidate_state(&db, &candidate, &row.candidate_key.test_folder()).await;
    assert!(crate::import::CandidatePreparations::new(db.clone())
        .store_verdict(&row)
        .await
        .is_err());
}
