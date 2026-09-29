mod preparation;
// What the pane stores under a candidate: the settled signals, the failure an import left, the cover, and the metadata and track
// rows the user typed.

use crate::import::folder_scanner::{CandidateFileEdits, SheetDisc};
use crate::import::{
    ArtistAssignment, AudioFile, CandidateEditField, CoverSelection,
    ExistingArtist, ImportFailure, ArtistCredit, RawPressingEdit, RawReleaseEdit, RawTrackEdit,
    TrackArtistAssignments,
};
use crate::signals::{BarcodeSignal, DiscIdSignal, InternalFailure, Signals, SourcedValue, TextSignal};

#[path = "pane_rows/artist_identity_conflicts.rs"]
mod artist_identity_conflicts;

fn pane_candidate() -> CategorizedFiles {
    track_files_candidate(&[("01 Track.flac", 111), ("CDImage.flac", 222)])
}
fn pane_candidate_path() -> String {
    PathBuf::from(host_root("/music"))
        .join("Album")
        .to_string_lossy()
        .into_owned()
}

fn settled_signals() -> Signals {
    Signals {
        origin: crate::signals::AudioOrigin::default(),
        disc_id: DiscIdSignal::Absent,
        barcode: BarcodeSignal::Absent,
        text: TextSignal::Settled {
            catalogs: Vec::new(),
            free_text: Vec::new(),
        },
        text_pool: Vec::new(),
        isrcs: Vec::new(),
        track_titles: Vec::new(),
    }
}

/// Store a verdict for `hash` carrying `signals`, and say whether it landed.
async fn store_verdict(db: &Database, hash: &str, signals: Signals) -> bool {
    crate::import::CandidatePreparations::new(db.clone())
        .store_verdict(&NewImportCandidateVerdict {
            content_hash: hash.to_string(),
file_edit_revision: 0,
            folder_path: pane_candidate_path(),
            verdict: sample_verdict(),
            signals,
            pick: None,
        })
        .await
        .unwrap()
}

fn edited_row(id: &str, title: &str, file: Option<AudioFile>) -> RawTrackEdit {
    RawTrackEdit {
        id: id.to_string(),
        title: title.to_string(),
        artist_assignments: TrackArtistAssignments::Explicit(vec![credit_named("Artist Name")]),
        side: Some(1),
        track_number: Some(1),
        file,
    }
}

fn credit_named(name: &str) -> ArtistAssignment {
    ArtistAssignment::Credit { credit: ArtistCredit {
            name: name.to_string(),
            sort_name: None,
            musicbrainz_artist_id: None,
            discogs_artist_id: None,
        },
    }
}

fn existing_artist() -> DbArtist {
    DbArtist {
        id: bae_test_support::test_uuid("library-artist"),
        name: "Library Artist".to_string(),
        sort_name: Some("Artist, Library".to_string()),
        discogs_artist_id: Some("discogs-library".to_string()),
        musicbrainz_artist_id: Some("mb-library".to_string()),
        created_at: fixed_now(),
    }
}

async fn stored_pane_candidate(db: &Database) -> (CategorizedFiles, String) {
    let root = host_root("/music");
    let item = scanned_candidate(&root, "Album");
    let crate::import::folder_scanner::ScanItem::Valid(candidate) = &item else {
        unreachable!("the fixture creates a valid candidate")
    };
    let files = candidate.files.clone();
    let hash = files.content_hash();
    db.add_watched_import_folder(&root).await.unwrap();
    let generation = db.begin_folder_scan(&root, crate::import::VolumeKind::Local).await.unwrap();
    db.save_folder_scan_item(&root, generation, &item)
        .await
        .unwrap()
        .expect("the current scan accepts the candidate");
    (files, hash)
}

async fn store_candidate_state(
    db: &Database,
    files: &CategorizedFiles,
    folder_path: &str,
) -> String {
    let path = PathBuf::from(folder_path);
    let root = path
        .parent()
        .expect("the candidate fixture has a watched root")
        .to_string_lossy()
        .into_owned();
    let name = path
        .file_name()
        .expect("the candidate fixture has a folder name")
        .to_string_lossy()
        .into_owned();
    let item = crate::import::folder_scanner::ScanItem::Valid(super::candidate_with(
        &root,
        &name,
        files.clone(),
        crate::import::folder_scanner::ReleaseFileScope::Direct,
    ));
    let hash = files.content_hash();
    db.add_watched_import_folder(&root).await.unwrap();
    let generation = db.begin_folder_scan(&root, crate::import::VolumeKind::Local).await.unwrap();
    db.save_folder_scan_item(&root, generation, &item)
        .await
        .unwrap()
        .expect("the current scan accepts the candidate");
    hash
}

/// `metadata_draft` as a candidate stores it.
fn candidate_draft(title: &str, artist: &str) -> crate::import::CandidateDraft {
    crate::import::pane::candidate_draft_from_edit(metadata_draft(title, artist))
        .unwrap()
        .draft
}

fn metadata_draft(title: &str, artist: &str) -> RawReleaseEdit {
    RawReleaseEdit {
        album_title: title.to_string(),
        album_artist_assignments: if artist.is_empty() {
            Vec::new()
        } else {
            vec![credit_named(artist)]
        },
        album_year: String::new(),
        pressing: RawPressingEdit {
            year: String::new(),
            labels: Vec::new(),
            facts: Default::default(),
            barcode: String::new(),
        },
        tracks: vec![RawTrackEdit {
            id: "candidate-track-0".to_string(),
            title: "Track title".to_string(),
            artist_assignments: TrackArtistAssignments::AlbumArtists,
            side: Some(1),
            track_number: Some(1),
            file: Some(AudioFile::Standalone {
                file_id: "01 Track.flac".into(),
            }),
        }],
    }
}

#[tokio::test]
async fn a_verdict_cannot_create_state_for_an_absent_candidate() {
    let (db, _tmp) = empty_db().await;
    let hash = pane_candidate().content_hash();

    assert!(!store_verdict(&db, &hash, settled_signals()).await);
    assert!(db
        .load_import_candidate_state(&hash)
        .await
        .unwrap()
        .is_none());
}

/// Every settled shape of every signal comes back as it went in, including
/// each way a lookup can fail.
#[tokio::test]
async fn every_settled_signal_shape_round_trips() {
    use crate::signals::{AudioOrigin, AudioSource, CdProof, DownloadProof, StoreMarker};
    let cd_rip = |proof, file: Option<&str>| AudioOrigin {
        source: Some(AudioSource::CdRip {
            proof,
            file: file.map(str::to_string),
        }),
        not_cd_rate: None,
    };
    let cases: Vec<(&str, AudioOrigin, DiscIdSignal, BarcodeSignal, TextSignal)> = vec![
        (
            "computed disc ID, settled barcodes and text",
            cd_rip(CdProof::RipLog, Some("rip.log")),
            DiscIdSignal::Computed {
                disc_id: "disc-hash".to_string(),
                source_file: Some("rip.log".to_string()),
            },
            BarcodeSignal::Settled {
                codes: vec![
                    // One code carries the image it was read off; one carries none.
                    SourcedValue::in_file("0123456789012".to_string(), "Scans/back.jpg".to_string()),
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
            TextSignal::Settled {
                catalogs: Vec::new(),
                free_text: Vec::new(),
            },
        ),
        (
            "every signal failed to read",
            cd_rip(CdProof::AccurateRipReport, None),
            DiscIdSignal::Failed {
                failure: InternalFailure {
                    detail: "the rip log did not read".to_string(),
                },
            },
            BarcodeSignal::Failed {
                failure: InternalFailure {
                    detail: "the artwork did not read".to_string(),
                },
                codes: vec![SourcedValue::new("0123456789012".to_string())],
            },
            TextSignal::Failed {
                failure: InternalFailure {
                    detail: "the artwork did not read".to_string(),
                },
                catalogs: vec!["CAT-2".to_string()],
                free_text: vec!["Some Line".to_string()],
            },
        ),
        (
            "a sheet a CD ripper wrote",
            cd_rip(CdProof::RipperSheet, Some("Album.cue")),
            DiscIdSignal::Absent,
            BarcodeSignal::Absent,
            TextSignal::Settled {
                catalogs: Vec::new(),
                free_text: Vec::new(),
            },
        ),
        (
            "a sheet left unhashed over audio no CD holds",
            AudioOrigin {
                source: None,
                not_cd_rate: Some(96_000),
            },
            DiscIdSignal::NotCdAudio,
            BarcodeSignal::Absent,
            TextSignal::Settled {
                catalogs: Vec::new(),
                free_text: Vec::new(),
            },
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
            TextSignal::Settled {
                catalogs: Vec::new(),
                free_text: Vec::new(),
            },
        ),
        (
            "a label's delivered download",
            AudioOrigin {
                source: Some(AudioSource::Download(DownloadProof::DeliverySet)),
                not_cd_rate: None,
            },
            DiscIdSignal::Absent,
            BarcodeSignal::Absent,
            TextSignal::Settled {
                catalogs: Vec::new(),
                free_text: Vec::new(),
            },
        ),
    ];

    // The tags' ISRCs ride along, in their order, a code tagged twice kept
    // twice; or none.
    let isrcs = [
        vec!["IT0000000002", "IT0000000001", "IT0000000002"],
        vec!["YU0000000001"],
        Vec::new(),
    ];
    for (at, (what, origin, disc_id, barcode, text)) in cases.into_iter().enumerate() {
        let (db, _tmp) = empty_db().await;
        let (_, hash) = stored_pane_candidate(&db).await;
        let signals = Signals {
            origin,
            disc_id,
            barcode,
            text,
            text_pool: Vec::new(),
            isrcs: isrcs[at % isrcs.len()]
                .iter()
                .map(|code| code.to_string())
                .collect(),
                track_titles: Vec::new(),
        };

        assert!(store_verdict(&db, &hash, signals.clone()).await, "{what}");

        let stored = db
            .load_import_candidate_state(&hash)
            .await
            .unwrap()
            .unwrap()
            .signals
            .unwrap_or_else(|| panic!("{what}: the signals read back"));
        assert_eq!(
            stored,
            Signals {
                text_pool: Vec::new(),
                ..signals
            },
            "{what}"
        );
    }
}

/// A signal still scanning is refused, and the whole write, verdict included,
/// rolls back.
#[tokio::test]
async fn a_scanning_signal_is_refused_and_writes_nothing() {
    for scanning in [
        Signals {
            origin: crate::signals::AudioOrigin::default(),
            disc_id: DiscIdSignal::Absent,
            barcode: BarcodeSignal::Scanning { codes: Vec::new() },
            text: TextSignal::Settled {
                catalogs: Vec::new(),
                free_text: Vec::new(),
            },
            text_pool: Vec::new(),
            isrcs: Vec::new(),
            track_titles: Vec::new(),
        },
        Signals {
            origin: crate::signals::AudioOrigin::default(),
            disc_id: DiscIdSignal::Absent,
            barcode: BarcodeSignal::Absent,
            text: TextSignal::Scanning {
                catalogs: Vec::new(),
                free_text: Vec::new(),
            },
            text_pool: Vec::new(),
            isrcs: Vec::new(),
            track_titles: Vec::new(),
        },
    ] {
        let (db, _tmp) = empty_db().await;
        let candidate = pane_candidate();
        let hash = store_candidate_state(&db, &candidate, &pane_candidate_path()).await;

        let error = crate::import::CandidatePreparations::new(db.clone())
            .store_verdict(&NewImportCandidateVerdict {
                content_hash: hash.to_string(),
file_edit_revision: 0,
                folder_path: pane_candidate_path(),
                verdict: sample_verdict(),
                signals: scanning,
                pick: None,
            })
            .await
            .expect_err("a scanning signal is not storable");
        assert!(
            error.to_string().contains("still scanning"),
            "{error} should name what was refused"
        );
        let state = db
            .load_import_candidate_state(&hash)
            .await
            .unwrap()
            .expect("the discovered candidate remains");
        assert!(
            state.identify.is_none(),
            "the refused write left no verdict"
        );
    }
}

/// A failed import is recorded on the discovered candidate, and a later one
/// replaces it.
#[tokio::test]
async fn a_failure_on_a_discovered_candidate_is_replaced_then_cleared() {
    let (db, _tmp) = empty_db().await;
    let (_, hash) = stored_pane_candidate(&db).await;

    db.save_import_candidate_failure(
        &hash,
        0,
        &ImportFailure::error_only("the folder vanished", fixed_now()),
    )
    .await
    .unwrap();

    let failure = db
        .load_import_candidate_pane_rows(&hash)
        .await
        .unwrap()
        .failure
        .expect("the failure is stored");
    assert_eq!(
        failure.reason,
        crate::import::ImportFailureReason::error("the folder vanished")
    );
    assert_eq!(failure.failed_at, fixed_now());
    assert!(db
        .load_import_candidate_pane_rows(&hash)
        .await
        .unwrap()
        .draft
        .release_edit()
        .is_blank());

    db.save_import_candidate_failure(
        &hash,
        0,
        &ImportFailure::error_only("the disc would not read", fixed_now()),
    )
    .await
    .unwrap();
    assert_eq!(
        db.load_import_candidate_pane_rows(&hash)
            .await
            .unwrap()
            .failure
            .unwrap()
            .reason,
        crate::import::ImportFailureReason::error("the disc would not read"),
        "the second failure replaces the first"
    );
}

#[tokio::test]
async fn an_active_import_omits_its_previous_persisted_failure_from_the_detail() {
    let (db, _tmp) = empty_db().await;
    let (_, hash) = stored_pane_candidate(&db).await;
    db.save_import_candidate_failure(
        &hash,
        0,
        &ImportFailure::error_only("the prior attempt failed", fixed_now()),
    )
    .await
    .unwrap();

    let key = pane_candidate_path();
    let detail = db
        .load_import_candidate(&key)
        .await
        .unwrap()
        .expect("the stored candidate has a detail")
        .resolve(&crate::import::TriageRuntimeFacts {
            identification: None,
            import: Some(crate::import::ImportStanding::Running),
        });

    assert!(detail.live.facts.importing());
    assert_eq!(
        detail.live.actions,
        vec![
            crate::import::CandidateAction::CancelImport,
            crate::import::CandidateAction::RevealFolder
        ]
    );
    assert!(detail.failure.is_none());
}

/// The pane draws a queued import differently from a running one, so its
/// import status says where the import stands.
#[tokio::test]
async fn the_pane_s_import_status_says_where_the_import_stands() {
    let (db, _tmp) = empty_db().await;
    stored_pane_candidate(&db).await;
    let key = pane_candidate_path();
    let projection = db
        .load_import_candidate(&key)
        .await
        .unwrap()
        .expect("the stored candidate has a detail");

    for standing in [
        crate::import::ImportStanding::Queued,
        crate::import::ImportStanding::Running,
        crate::import::ImportStanding::Writing,
    ] {
        let detail = projection.clone().resolve(&crate::import::TriageRuntimeFacts {
            identification: None,
            import: Some(standing),
        });
        assert_eq!(
            detail.import_status,
            Some(crate::import::CandidateImportStatus::Importing { standing })
        );
    }
}

fn remote_with_copies(url: &str) -> CoverSelection {
    use crate::import::cover_art::{DownscaledCopy, RemoteImageSet};
    CoverSelection::Remote(
        RemoteImageSet::with_copies(
            url.to_string(),
            [250, 500]
                .map(|max_edge| DownscaledCopy {
                    url: format!("{url}-{max_edge}"),
                    max_edge,
                })
                .to_vec(),
        ),
        Catalog::MusicBrainz,
    )
}

/// Every kind of cover choice comes back as it went in, a remote one with its
/// downscaled copies.
#[tokio::test]
async fn a_cover_choice_round_trips_in_every_shape() {
    for cover in [
        CoverSelection::Local("cover.jpg".to_string()),
        CoverSelection::Remote(
            crate::import::cover_art::RemoteImageSet::original(
                "https://example.invalid/front".to_string(),
            ),
            Catalog::Discogs,
        ),
        remote_with_copies("https://example.invalid/front"),
    ] {
        let (db, _tmp) = empty_db().await;
        let (_, hash) = stored_pane_candidate(&db).await;

        crate::import::CandidatePreparations::new(db.clone())
            .set_cover(&hash, &cover)
            .await
            .unwrap();

        assert_eq!(
            db.load_import_candidate_pane_rows(&hash)
                .await
                .unwrap()
                .cover,
            Some(cover)
        );
    }
}

/// Choosing again replaces the previous choice's copies rather than adding the
/// new ones beside them.
#[tokio::test]
async fn a_new_cover_choice_leaves_none_of_the_old_copies() {
    let (db, _tmp) = empty_db().await;
    let (_, hash) = stored_pane_candidate(&db).await;
    let preparations = crate::import::CandidatePreparations::new(db.clone());

    for cover in [
        remote_with_copies("https://example.invalid/first"),
        CoverSelection::Local("cover.jpg".to_string()),
        remote_with_copies("https://example.invalid/second"),
    ] {
        preparations.set_cover(&hash, &cover).await.unwrap();
        assert_eq!(
            db.load_import_candidate_pane_rows(&hash)
                .await
                .unwrap()
                .cover,
            Some(cover)
        );
    }
}

#[tokio::test]
async fn a_remote_cover_round_trips_the_exact_prepared_bytes() {
    let (db, _tmp) = empty_db().await;
    let (_, hash) = stored_pane_candidate(&db).await;
    let cover = CoverSelection::Remote(
        crate::import::cover_art::RemoteImageSet::original(
            "https://example.invalid/image".to_string(),
        ),
        Catalog::Discogs,
    );
    let image = crate::import::cover_art::RemoteImage {
        bytes: vec![1, 2, 3, 4],
        content_type: crate::util::content_type::ContentType::Jpeg,
    };

    crate::import::CandidatePreparations::new(db.clone())
        .set_prepared_cover(
            &host_root("/music"),
            &pane_candidate_path(),
            &as_read(&hash, 0),
            &cover,
            Some(&image),
        )
        .await
        .unwrap();

    assert_eq!(
        db.load_import_candidate_prepared_assets(&hash)
            .await
            .unwrap()
            .remote_cover,
        Some(image)
    );
}

#[tokio::test]
async fn a_remote_cover_without_exact_bytes_writes_nothing() {
    let (db, _tmp) = empty_db().await;
    let (_, hash) = stored_pane_candidate(&db).await;
    let cover = CoverSelection::Remote(
        crate::import::cover_art::RemoteImageSet::original(
            "https://example.invalid/image".to_string(),
        ),
        Catalog::Discogs,
    );

    crate::import::CandidatePreparations::new(db.clone())
        .set_prepared_cover(
            &host_root("/music"),
            &pane_candidate_path(),
            &as_read(&hash, 0),
            &cover,
            None,
        )
        .await
        .expect_err("a remote selection requires its exact bytes");

    let state = db
        .load_import_candidate_state(&hash)
        .await
        .unwrap()
        .expect("the candidate remains");
    assert_eq!(state.metadata_revision, 0);
    assert_eq!(
        db.load_import_candidate_pane_rows(&hash)
            .await
            .unwrap()
            .cover,
        None
    );
}

#[tokio::test]
async fn a_stale_remote_cover_write_leaves_the_current_selection_and_bytes() {
    let (db, _tmp) = empty_db().await;
    let (_, hash) = stored_pane_candidate(&db).await;
    let current_cover = CoverSelection::Remote(
        crate::import::cover_art::RemoteImageSet::original("https://example.invalid/current".to_string()),
        Catalog::Discogs,
    );
    let current_image = crate::import::cover_art::RemoteImage {
        bytes: vec![1, 2, 3],
        content_type: crate::util::content_type::ContentType::Jpeg,
    };
    crate::import::CandidatePreparations::new(db.clone())
        .set_prepared_cover(
            &host_root("/music"),
            &pane_candidate_path(),
            &as_read(&hash, 0),
            &current_cover,
            Some(&current_image),
        )
        .await
        .unwrap();

    let stale_cover = CoverSelection::Remote(
        crate::import::cover_art::RemoteImageSet::original("https://example.invalid/stale".to_string()),
        Catalog::MusicBrainz,
    );
    let stale_image = crate::import::cover_art::RemoteImage {
        bytes: vec![4, 5, 6],
        content_type: crate::util::content_type::ContentType::Png,
    };
    crate::import::CandidatePreparations::new(db.clone())
        .set_prepared_cover(
            &host_root("/music"),
            &pane_candidate_path(),
            &as_read(&hash, 0),
            &stale_cover,
            Some(&stale_image),
        )
        .await
        .expect_err("revision zero is stale after the first selection");

    let rows = db.load_import_candidate_pane_rows(&hash).await.unwrap();
    let assets = db
        .load_import_candidate_prepared_assets(&hash)
        .await
        .unwrap();
    assert_eq!(rows.cover, Some(current_cover));
    assert_eq!(assets.remote_cover, Some(current_image));
}

#[tokio::test]
async fn metadata_replacement_replaces_the_complete_artist_asset_set() {
    let (db, _tmp) = empty_db().await;
    fetched(&db, "release-1").await;
    fetched(&db, "release-2").await;
    let (_, hash) = stored_pane_candidate(&db).await;
    let mut draft = candidate_draft("Release Title", "Artist Name");
    let first = crate::import::PreparedArtistImage::Nothing {
        discogs_artist_id: "101".to_string(),
    };
    draft.album_artist_assignments[0] = crate::import::ArtistAssignment::Credit { credit: crate::import::ArtistCredit {
            name: "Artist Name".to_string(),
            sort_name: None,
            musicbrainz_artist_id: None,
            discogs_artist_id: Some("101".to_string()),
        },
    };
    let revision = crate::import::CandidatePreparations::new(db.clone())
        .apply_source(
            &host_root("/music"),
            &as_read(&hash, 0),
            &pane_candidate_path(),
            &crate::import::CandidateMetadataDraft {
                draft: draft.clone(),
                source_discogs_artist_ids: Default::default(),
                provenance: Some(release_pick("release-1")),
                cover: None,
                assets: crate::import::CandidatePreparedAssets {
                    applied_source: None,
                    remote_cover: None,
                    artist_images: vec![first],
                },
            },
        )
        .await
        .unwrap();

    draft.album_artist_assignments[0] = crate::import::ArtistAssignment::Credit { credit: crate::import::ArtistCredit {
            name: "Replacement Artist".to_string(),
            sort_name: None,
            musicbrainz_artist_id: None,
            discogs_artist_id: Some("202".to_string()),
        },
    };
    let second = crate::import::PreparedArtistImage::Nothing {
        discogs_artist_id: "202".to_string(),
    };
    let revision = crate::import::CandidatePreparations::new(db.clone())
        .apply_source(
            &host_root("/music"),
            &as_read(&hash, revision),
            &pane_candidate_path(),
            &crate::import::CandidateMetadataDraft {
                draft,
                source_discogs_artist_ids: Default::default(),
                provenance: Some(release_pick("release-2")),
                cover: None,
                assets: crate::import::CandidatePreparedAssets {
                    applied_source: None,
                    remote_cover: None,
                    artist_images: vec![second.clone()],
                },
            },
        )
        .await
        .unwrap();

    assert_eq!(
        db.load_import_candidate_prepared_assets(&hash)
            .await
            .unwrap()
            .artist_images,
        vec![second]
    );

    let third_assignment = crate::import::ArtistAssignment::Credit { credit: crate::import::ArtistCredit {
            name: "Edited Artist".to_string(),
            sort_name: None,
            musicbrainz_artist_id: None,
            discogs_artist_id: Some("303".to_string()),
        },
    };
    let third = crate::import::PreparedArtistImage::Nothing {
        discogs_artist_id: "303".to_string(),
    };
    crate::import::CandidatePreparations::new(db.clone())
        .set_album_artists_prepared(
            &host_root("/music"),
            &pane_candidate_path(),
            &as_read(&hash, revision),
            &[third_assignment],
            &std::collections::BTreeSet::new(),
            std::slice::from_ref(&third),
        )
        .await
        .unwrap();

    assert_eq!(
        db.load_import_candidate_prepared_assets(&hash)
            .await
            .unwrap()
            .artist_images,
        vec![third]
    );
}

#[tokio::test]
async fn preparation_round_trips_source_only_artist_answers() {
    let (db, _tmp) = empty_db().await;
    fetched(&db, "release-with-role").await;
    let (_, hash) = stored_pane_candidate(&db).await;
    let source_ids = std::collections::BTreeSet::from(["role-artist".to_string()]);
    let answer = crate::import::PreparedArtistImage::Nothing {
        discogs_artist_id: "role-artist".to_string(),
    };

    crate::import::CandidatePreparations::new(db.clone())
        .apply_source(
            &host_root("/music"),
            &as_read(&hash, 0),
            &pane_candidate_path(),
            &crate::import::CandidateMetadataDraft {
                draft: candidate_draft("Release Title", "Artist Name"),
                source_discogs_artist_ids: source_ids.clone(),
                provenance: Some(release_pick("release-with-role")),
                cover: None,
                assets: crate::import::CandidatePreparedAssets {
                    applied_source: None,
                    remote_cover: None,
                    artist_images: vec![answer.clone()],
                },
            },
        )
        .await
        .unwrap();

    let preparation = db
        .load_import_candidate_preparation(&hash)
        .await
        .unwrap()
        .expect("the candidate is prepared");
    assert_eq!(preparation.source_discogs_artist_ids, source_ids);
    assert_eq!(preparation.assets.artist_images, vec![answer]);

    crate::import::CandidatePreparations::new(db.clone())
        .set_album_artists_prepared(
            &host_root("/music"),
            &pane_candidate_path(),
            &crate::import::CandidateAsRead {
                content_hash: hash.clone(),
                file_edit_revision: preparation.file_edit_revision,
                metadata_revision: preparation.metadata_revision,
            },
            &preparation.draft.album_artist_assignments,
            &std::collections::BTreeSet::new(),
            &[],
        )
        .await
        .unwrap();

    let preparation = db
        .load_import_candidate_preparation(&hash)
        .await
        .unwrap()
        .expect("the candidate remains prepared");
    assert!(preparation.source_discogs_artist_ids.is_empty());
    assert!(preparation.assets.artist_images.is_empty());
}

/// A refusal because the release is already in the library is stored as the
/// album it matched, so the failure reads that album's title as it is now.
#[tokio::test]
async fn an_already_in_library_failure_reads_the_albums_current_title() {
    let (db, _tmp) = empty_db().await;
    let (_, hash) = stored_pane_candidate(&db).await;
    let artist = crate::db::DbArtist {
        id: "6c441836-aef7-4239-8a84-5336c4cce52c".to_string(),
        name: "Artist Name".to_string(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: None,
        created_at: fixed_now(),
    };
    db.insert_artist(&artist).await.unwrap();
    let album = crate::db::DbAlbum::new_test("Album Title", &artist.id);
    db.insert_album(&album).await.unwrap();

    db.save_import_candidate_failure(
        &hash,
        0,
        &ImportFailure {
            reason: crate::import::ImportFailureReason::AlreadyInLibrary {
                album_id: album.id.clone(),
                album_title: "Album Title".to_string(),
            },
            failed_at: fixed_now(),
            artist_identity_conflict: None,
        },
    )
    .await
    .unwrap();
    db.execute_local_sql_for_test(&format!(
        "UPDATE albums SET title = 'Album Title Renamed' WHERE id = '{}'",
        album.id
    ))
    .await
    .unwrap();

    let failure = db
        .load_import_candidate_pane_rows(&hash)
        .await
        .unwrap()
        .failure
        .expect("the failure is stored");
    assert_eq!(
        failure.reason,
        crate::import::ImportFailureReason::AlreadyInLibrary {
            album_id: album.id,
            album_title: "Album Title Renamed".to_string(),
        }
    );
}
