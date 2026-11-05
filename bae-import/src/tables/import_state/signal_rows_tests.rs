//! A candidate's settled signals as stored and read back.

use crate::tables::import_state::tests::*;
use crate::tables::tests::*;
use crate::tables::watched_path::host_root;
use crate::tables::NewImportCandidateVerdict;
use bae_catalogs::InternalFailure;
use bae_db::test_fixtures::empty_db;
use bae_identify::signals::{BarcodeSignal, DiscIdSignal, Signals, SourcedValue, TextSignal};

/// Every settled shape of every signal comes back as it went in, including
/// each way a lookup can fail.
#[tokio::test]
async fn every_settled_signal_shape_round_trips() {
    use bae_catalogs::{AudioOrigin, AudioSource, CdProof, DownloadProof, StoreMarker};
    let cd_rip = |proof, file: Option<&str>| AudioOrigin {
        source: Some(AudioSource::CdRip { proof, file: file.map(str::to_string) }),
        not_cd_rate: None,
    };
    let cases: Vec<(&str, AudioOrigin, DiscIdSignal, BarcodeSignal, TextSignal)> = vec![
        (
            "computed disc ID, settled barcodes and text",
            cd_rip(CdProof::RipLog, Some("rip.log")),
            DiscIdSignal::Computed {
                // One disc ID per disc, each with the log it was read off.
                discs: vec![
                    bae_identify::signals::ComputedDiscId {
                        disc_id: "disc-hash".to_string(),
                        source_file: Some("rip.log".to_string()),
                    },
                    bae_identify::signals::ComputedDiscId {
                        disc_id: "second-disc-hash".to_string(),
                        source_file: Some("Disc 2/rip.log".to_string()),
                    },
                ],
            },
            BarcodeSignal::Settled {
                codes: vec![
                    // One code carries the image it was read off; one carries none.
                    SourcedValue::in_file(
                        "0123456789012".to_string(),
                        "Scans/back.jpg".to_string(),
                    ),
                    SourcedValue::new("9876543210987".to_string()),
                ],
            },
            TextSignal::Settled {
                catalogs: vec!["CAT-1".to_string()],
                free_text: vec!["Album Title".to_string(), "Artist Name".to_string()],
            },
        ),
        (
            "absent everywhere",
            AudioOrigin::default(),
            DiscIdSignal::Absent,
            BarcodeSignal::Absent,
            TextSignal::Settled { catalogs: Vec::new(), free_text: Vec::new() },
        ),
        (
            "every signal failed to read",
            cd_rip(CdProof::AccurateRipReport, None),
            DiscIdSignal::Failed {
                failure: InternalFailure { detail: "the rip log did not read".to_string() },
            },
            BarcodeSignal::Failed {
                failure: InternalFailure { detail: "the artwork did not read".to_string() },
                codes: vec![SourcedValue::new("0123456789012".to_string())],
            },
            TextSignal::Failed {
                failure: InternalFailure { detail: "the artwork did not read".to_string() },
                catalogs: vec!["CAT-2".to_string()],
                free_text: vec!["Some Line".to_string()],
            },
        ),
        (
            "a sheet a CD ripper wrote",
            cd_rip(CdProof::RipperSheet, Some("Album.cue")),
            DiscIdSignal::Absent,
            BarcodeSignal::Absent,
            TextSignal::Settled { catalogs: Vec::new(), free_text: Vec::new() },
        ),
        (
            "a sheet left unhashed over audio no CD holds",
            AudioOrigin { source: None, not_cd_rate: Some(96_000) },
            DiscIdSignal::NotCdAudio,
            BarcodeSignal::Absent,
            TextSignal::Settled { catalogs: Vec::new(), free_text: Vec::new() },
        ),
        (
            "a store's download at a rate no CD plays at",
            AudioOrigin {
                source: Some(AudioSource::Download(DownloadProof::Store {
                    marker: StoreMarker::ITunesPurchase,
                    file: "01.m4a".to_string(),
                })),
                not_cd_rate: Some(96_000),
            },
            DiscIdSignal::Absent,
            BarcodeSignal::Absent,
            TextSignal::Settled { catalogs: Vec::new(), free_text: Vec::new() },
        ),
        (
            "a label's delivered download",
            AudioOrigin {
                source: Some(AudioSource::Download(DownloadProof::DeliverySet)),
                not_cd_rate: None,
            },
            DiscIdSignal::Absent,
            BarcodeSignal::Absent,
            TextSignal::Settled { catalogs: Vec::new(), free_text: Vec::new() },
        ),
    ];

    // The tags' ISRCs ride along, in their order, a code tagged twice kept
    // twice; or none.
    let isrcs =
        [vec!["IT0000000002", "IT0000000001", "IT0000000002"], vec!["YU0000000001"], Vec::new()];
    for (at, (what, origin, disc_id, barcode, text)) in cases.into_iter().enumerate() {
        let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
        let tables = crate::tables::ImportTables::new(db.clone());
        let (_, hash) = stored_pane_candidate(&db).await;
        let signals = Signals {
            origin,
            disc_id,
            barcode,
            text,
            text_pool: Vec::new(),
            isrcs: isrcs[at % isrcs.len()].iter().map(|code| code.to_string()).collect(),
            track_titles: Vec::new(),
        };

        assert!(store_verdict_with_signals(&db, &hash, signals.clone()).await, "{what}");

        let stored = tables
            .load_import_candidate_state(&hash)
            .await
            .unwrap()
            .unwrap()
            .signals
            .unwrap_or_else(|| panic!("{what}: the signals read back"));
        assert_eq!(stored, Signals { text_pool: Vec::new(), ..signals }, "{what}");
    }
}

/// A signal still scanning is refused, and the whole write, verdict included,
/// rolls back.
#[tokio::test]
async fn a_scanning_signal_is_refused_and_writes_nothing() {
    for scanning in [
        Signals {
            origin: bae_catalogs::AudioOrigin::default(),
            disc_id: DiscIdSignal::Absent,
            barcode: BarcodeSignal::Scanning { codes: Vec::new() },
            text: TextSignal::Settled { catalogs: Vec::new(), free_text: Vec::new() },
            text_pool: Vec::new(),
            isrcs: Vec::new(),
            track_titles: Vec::new(),
        },
        Signals {
            origin: bae_catalogs::AudioOrigin::default(),
            disc_id: DiscIdSignal::Absent,
            barcode: BarcodeSignal::Absent,
            text: TextSignal::Scanning { catalogs: Vec::new(), free_text: Vec::new() },
            text_pool: Vec::new(),
            isrcs: Vec::new(),
            track_titles: Vec::new(),
        },
    ] {
        let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
        let tables = crate::tables::ImportTables::new(db.clone());
        let candidate = pane_candidate();
        let hash = store_candidate_state(&db, &candidate, &pane_candidate_path()).await;

        let error = crate::CandidatePreparations::new(db.clone())
            .store_verdict(&NewImportCandidateVerdict {
                content_hash: hash.to_string(),
                file_edit_revision: 0,
                candidate_key: pane_candidate_key(),
                verdict: sample_verdict(),
                signals: scanning,
                pick: None,
                releases: Vec::new(),
            })
            .await
            .expect_err("a scanning signal is not storable");
        assert!(
            error.to_string().contains("still scanning"),
            "{error} should name what was refused"
        );
        let state = tables
            .load_import_candidate_state(&hash)
            .await
            .unwrap()
            .expect("the discovered candidate remains");
        assert!(state.identify.is_none(), "the refused write left no verdict");
    }
}

/// The candidate's text reads back whole, line by line, in order and with the
/// surface each was read off.
#[tokio::test]
async fn the_candidate_s_text_round_trips_line_by_line() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let candidate =
        track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
    let hash = candidate.content_hash();
    let pool = vec![
        bae_identify::signals::TextLine {
            text: "Artist One - Album One [L1-16033]".to_string(),
            origin: bae_identify::signals::TextOrigin::FolderName,
        },
        bae_identify::signals::TextLine {
            text: "Label One Records, Inc.".to_string(),
            origin: bae_identify::signals::TextOrigin::Artwork,
        },
    ];
    let mut row = new_candidate_row(&hash, &host_root("/music/Some Album"), &sample_verdict());
    row.signals.text_pool = pool.clone();
    store_candidate_state(&db, &candidate, &row.candidate_key.test_folder()).await;

    crate::CandidatePreparations::new(db.clone()).store_verdict(&row).await.unwrap();

    let loaded = tables.load_import_candidate_states().await.unwrap();
    let stored = loaded
        .get(&hash)
        .expect("row present under its content hash")
        .signals
        .as_ref()
        .expect("a stored verdict reads its signals back");
    assert_eq!(stored.text_pool, pool);
}
