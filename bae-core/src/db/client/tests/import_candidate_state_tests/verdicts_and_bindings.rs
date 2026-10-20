use super::super::*;
use crate::identify::{
    DiscIdStepView, Findings, IdentifyRunView, LookupProvenance, LookupView, NarrowedOut,
    TerminalVerdict,
};
use crate::import::folder_scanner::{CandidateFile, CategorizedFiles, FileRole, ScannedFile};
use crate::import::search::MetadataResult;
use crate::import::watched_folder::host_root;
use coven::FixedClock;
use std::path::PathBuf;

/// A folder of plain track files, each `(relative_path, size)`.
fn track_files_candidate(files: &[(&str, u64)]) -> CategorizedFiles {
    CategorizedFiles {
        files: files
            .iter()
            .map(|(name, size)| CandidateFile {
                file: ScannedFile::new(PathBuf::from(*name), name.to_string(), *size, 1)
                    .with_test_flac_audio(),
                role: FileRole::Audio,
            })
            .collect(),
        parts: Vec::new(),
    }
}

/// The sample verdict's ledger: a disc ID read off a rip log that named its
/// release.
fn sample_ledger() -> IdentifyRunView {
    IdentifyRunView {
        providers: vec![Catalog::MusicBrainz],
        disc_id: DiscIdStepView::Read {
            disc_id: "disc-1".to_string(),
            lookup: LookupView::Found {
                count: 1,
                groups: crate::import::release_group::group_results(
                    crate::import::release_group::unranked(vec![sample_match()]), None,
                ),
            },
        },
        barcode: crate::identify::BarcodeStepView::Absent,
        catalog: crate::identify::CatalogStepView::NoneFound,
        isrc: crate::identify::IsrcStepView::Absent,
        search: crate::identify::SearchStepView::NotNeeded,
    }
}

fn sample_match() -> MetadataResult {
    MetadataResult {
        source: Catalog::MusicBrainz,
        release_id: "rel-1".to_string(),
        title: "Album".to_string(),
        artist: Some("Artist".to_string()),
        year: Some(1999),
        labels: vec![crate::pressing::ReleaseLabel::of(Some("Label"), Some("CAT-1"))],
        area: Some(crate::pressing::area("US")),
        status: None,
        packaging: None,
        discogs_details: Vec::new(),
        barcodes: Vec::new(),
        media: crate::pressing::StatedMedia::PerMedium(vec![Some(crate::pressing::Medium::Cd)]),
        links: Vec::new(),
        cover_art: None,
        source_group_id: Some("group-1".to_string()),
        album_links: crate::import::album_links::AlbumLinks::NotAsked,
        source_tracks: Some(crate::import::search::SourceTracks::Listed { count: 2 }),
        document_failure: None,
        album_first_year: Some(1998),
        track_titles: vec!["Track One".to_string(), "Track Two".to_string()],
        notes: vec!["Pressed By Plant Name".to_string()],
    }
}

fn sample_verdict() -> TerminalVerdict {
    TerminalVerdict::Found {
        findings: sample_findings(),
        track_count: 11,
        ledger: Some(sample_ledger()),
    }
}

fn sample_findings() -> Findings {
    Findings {
        matches: vec![sample_match()],
        provenance: vec![LookupProvenance {
            by_disc_id: true,
            by_barcode: true,
            by_catalog: true,
            by_isrc: false,
            by_search: false,
            by_pressing: false,
        }],
        pressings: vec![0],
        narrowed_out: NarrowedOut::default(),
        medium_conflict: None,
        named_notes: vec![crate::identify::NamedNote {
            release: crate::import::MetadataRef::new(Catalog::MusicBrainz, "rel-1"),
            note: "Pressed By Plant Name".to_string(),
        }],
    }
}

/// Settled signals with nothing found.
fn sample_signals() -> crate::signals::Signals {
    crate::signals::Signals {
        origin: crate::signals::AudioOrigin::default(),
        disc_id: crate::signals::DiscIdSignal::Absent,
        barcode: crate::signals::BarcodeSignal::Absent,
        text: crate::signals::TextSignal::Settled {
            catalogs: Vec::new(),
            free_text: Vec::new(),
        },
        text_pool: Vec::new(),
        isrcs: Vec::new(),
        track_titles: Vec::new(),
    }
}

/// The row with a draft picking `release_id`, as a run that settled on it
/// writes.
fn concluding(mut row: NewImportCandidateVerdict, release_id: &str) -> NewImportCandidateVerdict {
    row.pick = Some(crate::db::VerdictPick {
        link: release_link(release_id),
        metadata: crate::import::CandidateMetadataDraft {
            draft: candidate_draft("", ""),
            source_discogs_artist_ids: Default::default(),
            provenance: Some(release_pick(release_id)),
            cover: None,
            assets: crate::import::CandidatePreparedAssets::default(),
        },
    });
    row
}

fn new_candidate_row(
    content_hash: &str,
    folder_path: &str,
    verdict: &TerminalVerdict,
) -> NewImportCandidateVerdict {
    NewImportCandidateVerdict {
        content_hash: content_hash.to_string(),
file_edit_revision: 0,
        folder_path: folder_path.to_string(),
        verdict: verdict.clone(),
        signals: sample_signals(),
        pick: None,
    }
}

#[path = "verdict_round_trips.rs"]
mod verdict_round_trips;

/// Resizing a file changes `content_hash`, so the new hash finds no row and the
/// old row stays under its own key.
#[tokio::test]
async fn resizing_a_file_orphans_the_old_row_under_a_new_hash() {
    let (db, _tmp) = empty_db().await;
    let original = track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
    let original_hash = original.content_hash();
    let row = new_candidate_row(
        &original_hash,
        &host_root("/music/Some Album"),
        &sample_verdict(),
    );
    store_candidate_state(&db, &original, &row.folder_path).await;
    crate::import::CandidatePreparations::new(db.clone())
        .store_verdict(&row)
        .await
        .unwrap();

    let resized = track_files_candidate(&[("01 Track.flac", 999_999), ("02 Track.flac", 234_567)]);
    let resized_hash = resized.content_hash();
    assert_ne!(
        original_hash, resized_hash,
        "resizing a file must change the hash"
    );

    let loaded = db.load_import_candidate_states().await.unwrap();
    assert!(
        loaded.contains_key(&original_hash),
        "the old row is still present under its own key"
    );
    assert!(
        !loaded.contains_key(&resized_hash),
        "the new hash must find no row -- the candidate needs re-identifying"
    );
}

/// Store MusicBrainz release `release_id` as a fetch would, so a pick can name
/// it.
async fn fetched(db: &Database, release_id: &str) {
    db.save_source_release(
        &crate::import::payloads::ReleasePayloads::for_test(
            crate::import::MetadataRef::new(crate::import::Catalog::MusicBrainz, release_id),
            serde_json::json!({
                "id": release_id,
                "title": "Album",
                "artist-credit": [{ "name": "Artist" }],
                "media": [],
                "cover-art-archive": { "front": false, "darkened": false }
            })
            .to_string(),
            Vec::new(),
        )
        .extract()
        .unwrap(),
    )
    .await
    .unwrap();
}

fn release_link(release_id: &str) -> crate::import::PressingLink {
    crate::import::PressingLink {
        record: crate::import::MetadataRef::new(
            crate::import::Catalog::MusicBrainz,
            release_id.to_string(),
        ),
        partners: vec![],
    }
}

fn release_pick(release_id: &str) -> crate::import::MetadataProvenance {
    crate::import::MetadataProvenance::ExternalRelease {
        record: crate::import::MetadataRef::new(
            crate::import::Catalog::MusicBrainz,
            release_id.to_string(),
        ),
    }
}

#[tokio::test]
async fn every_metadata_provenance_variant_survives_a_database_reopen() {
    let tmp = tempfile::TempDir::new().unwrap();
    let installation = coven::TestCoven::new();
    let layout = crate::config::AppDir::at(tmp.path()).unwrap().store_layout();
    let clock: coven::ClockRef = Arc::new(FixedClock::new(fixed_now().into()));
    let handle = crate::library::configure_store(installation.builder(layout.clone()))
        .clock(clock.clone()).create_store("Test Library").await.unwrap();
    let id = handle.store_id();
    let db = Database::from_handle(handle, clock.clone());
    fetched(&db, "release-a").await;
    let cases = [
        (
            track_files_candidate(&[("01 Track.flac", 100_001)]).content_hash(),
            &host_root("/music/Candidate A"),
            100_001,
            release_pick("release-a"),
        ),
        (
            track_files_candidate(&[("01 Track.flac", 100_002)]).content_hash(),
            &host_root("/music/Candidate B"),
            100_002,
            crate::import::MetadataProvenance::FileMetadata,
        ),
    ];

    for (content_hash, folder_path, size, provenance) in &cases {
        let candidate = track_files_candidate(&[("01 Track.flac", *size)]);
        assert_eq!(&candidate.content_hash(), content_hash);
        store_candidate_state(&db, &candidate, folder_path).await;
        let row = new_candidate_row(content_hash, folder_path, &sample_verdict());
        crate::import::CandidatePreparations::new(db.clone())
            .store_verdict(&row)
            .await
            .unwrap();
        crate::import::CandidatePreparations::new(db.clone())
            .replace_metadata(
                content_hash,
                folder_path,
                &metadata_draft("Album", "Artist"),
                Some(provenance),
            )
            .await
            .unwrap();
    }
    db.close().await.unwrap();
    drop(db);

    let handle = crate::library::configure_store(installation.builder(layout))
        .clock(clock.clone()).open(id).await.unwrap();
    let reopened = Database::from_handle(handle, clock);
    let loaded = reopened.load_import_candidate_states().await.unwrap();

    for (content_hash, _, _, provenance) in &cases {
        assert_eq!(
            loaded
                .get(content_hash)
                .expect("the candidate state survives the database reopen")
                .metadata_provenance
                .as_ref(),
            Some(provenance)
        );
    }
}

fn found_nothing() -> TerminalVerdict {
    TerminalVerdict::NotFoundAnywhere { ledger: None }
}

/// A re-run that finds nothing leaves the draft an earlier run wrote.
#[tokio::test]
async fn a_re_run_that_finds_nothing_leaves_the_draft_an_earlier_run_wrote() {
    let (db, _tmp) = empty_db().await;
    fetched(&db, "mb-rel-1").await;
    let candidate = track_files_candidate(&[("01 Track.flac", 123_456)]);
    let hash = store_candidate_state(&db, &candidate, &host_root("/music/Album")).await;

    let settled = concluding(
        new_candidate_row(&hash, &host_root("/music/Album"), &sample_verdict()),
        "mb-rel-1",
    );
    crate::import::CandidatePreparations::new(db.clone())
        .store_verdict(&settled)
        .await
        .unwrap();

    let re_run = new_candidate_row(&hash, &host_root("/music/Album"), &found_nothing());
    assert!(re_run.pick.is_none(), "nothing was found to pick");
    crate::import::CandidatePreparations::new(db.clone())
        .store_verdict(&re_run)
        .await
        .unwrap();

    let loaded = db.load_import_candidate_states().await.unwrap();
    assert_eq!(
        loaded
            .get(&hash)
            .expect("the row is still there")
            .metadata_provenance,
        Some(release_pick("mb-rel-1")),
        "a run that concluded no release wrote no draft"
    );
}

/// A re-run that finds nothing leaves a person's pick in place.
#[tokio::test]
async fn a_re_run_that_finds_nothing_leaves_a_person_s_pick_alone() {
    let (db, _tmp) = empty_db().await;
    fetched(&db, "mb-rel-chosen").await;
    let candidate = track_files_candidate(&[("01 Track.flac", 123_456)]);
    let hash = store_candidate_state(&db, &candidate, &host_root("/music/Album")).await;

    let initial = new_candidate_row(&hash, &host_root("/music/Album"), &found_nothing());
    crate::import::CandidatePreparations::new(db.clone())
        .store_verdict(&initial)
        .await
        .unwrap();
    crate::import::CandidatePreparations::new(db.clone())
        .replace_metadata(
            &hash,
            &host_root("/music/Album"),
            &metadata_draft("Album", "Artist"),
            Some(&release_pick("mb-rel-chosen")),
        )
        .await
        .unwrap();

    let re_run = new_candidate_row(&hash, &host_root("/music/Album"), &found_nothing());
    crate::import::CandidatePreparations::new(db.clone())
        .store_verdict(&re_run)
        .await
        .unwrap();

    let loaded = db.load_import_candidate_states().await.unwrap();
    assert_eq!(
        loaded
            .get(&hash)
            .expect("the row is still there")
            .metadata_provenance,
        Some(release_pick("mb-rel-chosen")),
        "a verdict that concluded no release must not unmake a choice a person made"
    );
}

/// A re-run that settles on a different release replaces the earlier run's
/// pick.
#[tokio::test]
async fn a_re_run_that_settles_elsewhere_replaces_the_pick_it_made() {
    let (db, _tmp) = empty_db().await;
    fetched(&db, "mb-rel-first").await;
    fetched(&db, "mb-rel-second").await;
    let candidate = track_files_candidate(&[("01 Track.flac", 123_456)]);
    let hash = store_candidate_state(&db, &candidate, &host_root("/music/Album")).await;

    let first = concluding(
        new_candidate_row(&hash, &host_root("/music/Album"), &sample_verdict()),
        "mb-rel-first",
    );
    crate::import::CandidatePreparations::new(db.clone())
        .store_verdict(&first)
        .await
        .unwrap();

    let second = concluding(
        new_candidate_row(&hash, &host_root("/music/Album"), &sample_verdict()),
        "mb-rel-second",
    );
    crate::import::CandidatePreparations::new(db.clone())
        .store_verdict(&second)
        .await
        .unwrap();

    let loaded = db.load_import_candidate_states().await.unwrap();
    assert_eq!(
        loaded
            .get(&hash)
            .expect("the row is still there")
            .metadata_provenance,
        Some(release_pick("mb-rel-second")),
        "the live verdict's own conclusion is the one that stands"
    );
}

/// A file decision clears the verdict and keeps applied metadata's provenance,
/// whoever applied it.
#[tokio::test]
async fn a_file_decision_preserves_applied_metadata_from_either_author() {
    let (db, _tmp) = empty_db().await;
    fetched(&db, "mb-rel-derived").await;
    fetched(&db, "mb-rel-chosen").await;
    let candidate = track_files_candidate(&[("01 Track.flac", 123_456)]);
    let hash = candidate.content_hash();
    let edits = crate::import::folder_scanner::CandidateFileEdits::default();
    store_candidate_state(&db, &candidate, &host_root("/music/Album")).await;

    let settled = concluding(
        new_candidate_row(&hash, &host_root("/music/Album"), &sample_verdict()),
        "mb-rel-derived",
    );
    crate::import::CandidatePreparations::new(db.clone())
        .store_verdict(&settled)
        .await
        .unwrap();
    let (metadata_revision, mapping_preparation) = current_mapping_preparation(&db, &hash).await;
    crate::import::CandidatePreparations::new(db.clone())
        .store_file_decisions(
            &as_read(&hash, metadata_revision),
            &host_root("/music/Album"),
            &edits,
            &[(host_root("/music/Album"), candidate.clone())],
            &mapping_preparation,
        )
        .await
        .unwrap();
    let loaded = db.load_import_candidate_states().await.unwrap();
    let row = loaded.get(&hash).expect("the row is still there");
    assert!(row.identify.is_none(), "the decision clears the verdict");
    assert_eq!(
        row.metadata_provenance,
        Some(release_pick("mb-rel-derived")),
        "the applied metadata keeps its source"
    );

    crate::import::CandidatePreparations::new(db.clone())
        .replace_metadata(
            &hash,
            &host_root("/music/Album"),
            &metadata_draft("Album", "Artist"),
            Some(&release_pick("mb-rel-chosen")),
        )
        .await
        .unwrap();
    let (metadata_revision, mapping_preparation) = current_mapping_preparation(&db, &hash).await;
    crate::import::CandidatePreparations::new(db.clone())
        .store_file_decisions(
            &crate::import::CandidateAsRead {
                content_hash: hash.clone(),
                file_edit_revision: 1,
                metadata_revision,
            },
            &host_root("/music/Album"),
            &edits,
            &[(host_root("/music/Album"), candidate.clone())],
            &mapping_preparation,
        )
        .await
        .unwrap();
    let loaded = db.load_import_candidate_states().await.unwrap();
    assert_eq!(
        loaded
            .get(&hash)
            .expect("the row is still there")
            .metadata_provenance,
        Some(release_pick("mb-rel-chosen")),
        "a file decision must not unmake a choice a person made"
    );
}

/// A moved folder hashes the same, so its saved row is still found.
#[tokio::test]
async fn a_moved_folder_hashes_identically_and_keeps_its_row() {
    let (db, _tmp) = empty_db().await;
    let at_old_location =
        track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
    let hash = at_old_location.content_hash();
    let row = new_candidate_row(
        &hash,
        &host_root("/music/Old Location/Some Album"),
        &sample_verdict(),
    );
    store_candidate_state(&db, &at_old_location, &row.folder_path).await;
    crate::import::CandidatePreparations::new(db.clone())
        .store_verdict(&row)
        .await
        .unwrap();

    let mut at_new_location = at_old_location.clone();
    for entry in &mut at_new_location.files {
        entry.file.path = PathBuf::from(host_root("/music/New Location/Some Album"))
            .join(&entry.file.relative_path);
    }
    assert_eq!(
        hash,
        at_new_location.content_hash(),
        "a moved folder must hash identically to itself before the move"
    );

    let loaded = db.load_import_candidate_states().await.unwrap();
    assert!(
        loaded.contains_key(&at_new_location.content_hash()),
        "the row saved before the move must still be reachable after it"
    );
}

/// A transport failure is stored as a failed verdict.
#[tokio::test]
async fn a_transport_failure_round_trips_as_a_failed_verdict() {
    use crate::identify::state::step as identify_step;
    use crate::identify::{IdentifyEvent, IdentifyState};
    use crate::signals::{BarcodeSignal, DiscIdSignal, LookupFailure, Signals, TextSignal};

    let (db, _tmp) = empty_db().await;
    let candidate = track_files_candidate(&[("01 Track.flac", 123_456)]);
    let hash = candidate.content_hash();
    store_candidate_state(&db, &candidate, &host_root("/music/Some Album")).await;

    let (state, _) = identify_step(
        IdentifyState::Idle,
        IdentifyEvent::Started {
            providers: vec![crate::import::Catalog::MusicBrainz],
            choices: crate::import::LookupChoices::default(),
            title_search: None,
        },
    );
    let (state, _) = identify_step(
        state,
        IdentifyEvent::SignalsUpdated {
            signals: Signals {
                origin: crate::signals::AudioOrigin::default(),
                disc_id: DiscIdSignal::Computed {
                    disc_id: "disc-hash".to_string(),
                    source_file: None,
                },
                barcode: BarcodeSignal::Absent,
                text: TextSignal::Settled {
                    catalogs: vec![],
                    free_text: vec![],
                },
                text_pool: Vec::new(),
                isrcs: Vec::new(),
                track_titles: Vec::new(),
            },
            audio: crate::signals::AudioFacts::default(),
            artwork: crate::signals::ArtworkScan::Absent,
        },
    );
    let (state, _) = identify_step(
        state,
        IdentifyEvent::DiscidLookupFailed {
            failure: LookupFailure::Provider { status: Some(503) },
        },
    );

    let verdict = TerminalVerdict::try_from(state).expect("the failure is terminal");
    let row = new_candidate_row(&hash, &host_root("/music/Some Album"), &verdict);
    crate::import::CandidatePreparations::new(db.clone())
        .store_verdict(&row)
        .await
        .unwrap();

    let loaded = db.load_import_candidate_states().await.unwrap();
    let loaded = loaded
        .get(&hash)
        .and_then(|state| state.identify.as_ref())
        .expect("the failed verdict is stored");
    assert_eq!(
        loaded.verdict, verdict,
        "the provider failure survives the database boundary"
    );
}

include!("bindings.rs");
