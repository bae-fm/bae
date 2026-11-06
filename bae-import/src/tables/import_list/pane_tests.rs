//! One candidate as the pane reads it: its verdict resumed, its records as
//! the row names them, where the list places it, what is running for it, and
//! a folder that failed validation read whole.

use super::*;
use crate::folder_scanner::{FileVersion, ScanItem};
use crate::list::ImportListItem;
use crate::tables::import_list::tests::*;
use crate::tables::import_state::tests::*;
use crate::tables::tests::*;
use crate::ImportFailure;
use crate::{MetadataProvenance, TriageTab};
use bae_catalogs::catalog::LookupCatalog;
use bae_catalogs::records::PayloadSource;
use bae_identify::TerminalVerdict;
use bae_store::test_fixtures::empty_db;
use bae_store::test_fixtures::fixed_now;
use std::path::PathBuf;

/// A MusicBrainz release linking a Discogs release, in a group linking
/// AllMusic and Wikidata.
fn musicbrainz_release_linked_out(release_id: &str, group_id: &str) -> serde_json::Value {
    let mut release = musicbrainz_release(release_id, "Linked Album");
    release["release-group"] = serde_json::json!({ "id": group_id });
    release["relations"] = serde_json::json!([{
        "target-type": "url",
        "type": "discogs",
        "url": { "resource": "https://www.discogs.com/release/4242" }
    }]);
    release
}

fn release_group_linked_out() -> serde_json::Value {
    serde_json::json!({
        "id": "mb-group",
        "relations": [
            { "url": { "resource": "https://www.allmusic.com/album/mw0000424242" } },
            { "url": { "resource": "https://www.wikidata.org/wiki/Q424242" } }
        ]
    })
}

/// Which catalogs describe a pick is read off its stored release once:
/// the queue's row and the pane's detail name the same records, in the order
/// surfaces list catalogs — the pick's own, the release its document links,
/// and the two its release group links.
#[tokio::test]
async fn the_row_and_the_pane_name_the_same_records() {
    let (db, _tmp, root) = watched_root().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let candidate = scanned(&db, &root, "Album").await;
    save_verdict(&db, &candidate, "mb-verdict").await;
    tables
        .save_source_release(
            &bae_catalogs::payloads::ReleasePayloads::for_test(
                bae_catalogs::records::LookupRef::new(LookupCatalog::MusicBrainz, "mb-linked"),
                musicbrainz_release_linked_out("mb-linked", "mb-group").to_string(),
                vec![bae_catalogs::records::SourcePayload::new(
                    PayloadSource::MusicBrainzReleaseGroup,
                    "mb-group",
                    release_group_linked_out().to_string(),
                )],
            )
            .extract()
            .unwrap(),
        )
        .await
        .unwrap();
    let draft = tables
        .load_import_candidate_pane_rows(&candidate.files.content_hash())
        .await
        .unwrap()
        .draft
        .release_edit();
    crate::CandidatePreparations::new(db.clone())
        .replace_metadata(
            &candidate.files.content_hash(),
            &crate::CandidateKey::folder(&candidate.path),
            &draft,
            Some(&MetadataProvenance::ExternalRelease {
                record: bae_catalogs::records::LookupRef::new(
                    LookupCatalog::MusicBrainz,
                    "mb-linked".to_string(),
                ),
            }),
            &[],
        )
        .await
        .unwrap();
    crate::CandidatePreparations::new(db.clone())
        .replace_link(
            &candidate.files.content_hash(),
            Some(&crate::ReleaseLink::Pressing(bae_identify::PressingLink {
                record: bae_catalogs::records::LookupRef::new(
                    LookupCatalog::MusicBrainz,
                    "mb-linked",
                ),
                partners: vec![],
            })),
            &[],
        )
        .await
        .unwrap();

    let projection = tables.load_import_list_read(request(TriageTab::Pending).await).await.unwrap();
    let row = rows(&projection).remove(0);
    let crate::triage::TriageReading::Identified { records } = &row.reading else {
        panic!("a picked row reads as identified, got {:?}", row.reading);
    };
    assert_eq!(
        records.iter().map(|record| (record.catalog(), record.key())).collect::<Vec<_>>(),
        vec![
            (Catalog::MusicBrainz, "mb-linked"),
            (Catalog::Discogs, "4242"),
            (Catalog::AllMusic, "mw0000424242"),
            (Catalog::Wikidata, "Q424242"),
        ]
    );

    let detail = tables
        .load_import_candidate(&crate::CandidateKey::folder(&candidate.path))
        .await
        .unwrap()
        .expect("the scanned candidate has a pane")
        .resolve();
    let crate::CandidatePanePlacement::Pending { records: pane_records, .. } = &detail.placement
    else {
        panic!("a picked candidate is pending, got {:?}", detail.placement);
    };
    assert_eq!(pane_records, records, "the pane names the records the queue's row does");
}

/// A verdict whose releases the folder's files rule out reads back as the
/// check the pane states beside Import, and the resumed state reads where
/// the audio came from, and the rate that rules a CD out, off the stored
/// signals.
#[tokio::test]
async fn a_verdict_the_folder_rules_out_reads_back_as_its_failed_check() {
    let (db, _tmp, root) = watched_root().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let mut candidate = candidate(&root, "Album");
    for file in &mut candidate.files.files {
        if let Some(audio) = &mut file.file.source_audio {
            audio.format.sample_rate_hz = 96_000;
            audio.format.bits_per_sample = Some(24);
        }
    }
    tables.add_watched_import_folder(&root).await.unwrap();
    let generation = tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    tables
        .save_folder_scan_item(&root, generation, &ScanItem::Valid(candidate.clone()))
        .await
        .unwrap();
    tables.finish_folder_scan(&root, generation, None).await.unwrap();
    let conflict = bae_identify::MediumConflict::NotCdAudio;
    let TerminalVerdict::Found { mut findings, track_count, ledger } = verdict("mb-verdict", None)
    else {
        unreachable!("verdict builds a found verdict");
    };
    findings.medium_conflict = Some(conflict);
    let stored = TerminalVerdict::Found { findings, track_count, ledger };
    assert!(crate::CandidatePreparations::new(db.clone())
        .store_verdict(&NewImportCandidateVerdict {
            content_hash: candidate.files.content_hash(),
            file_edit_revision: 0,
            candidate_key: candidate.key(),
            verdict: stored.clone(),
            signals: bae_identify::signals::Signals {
                origin: bae_catalogs::AudioOrigin { source: None, not_cd_rate: Some(96_000) },
                disc_id: bae_identify::signals::DiscIdSignal::Absent,
                barcode: bae_identify::signals::BarcodeSignal::Absent,
                text: bae_identify::signals::TextSignal::Settled {
                    catalogs: Vec::new(),
                    free_text: Vec::new(),
                },
                text_pool: Vec::new(),
                isrcs: Vec::new(),
                track_titles: Vec::new(),
            },
            pick: None,
            releases: Vec::new(),
        })
        .await
        .unwrap());

    let state = tables
        .load_import_candidate_state(&candidate.files.content_hash())
        .await
        .unwrap()
        .expect("the candidate has a state row");
    assert_eq!(state.identify.expect("the verdict reads back").verdict, stored);
    let pane = tables
        .load_import_candidate(&crate::CandidateKey::folder(&candidate.path))
        .await
        .unwrap()
        .expect("the scanned candidate reads back")
        .resolve();
    assert_eq!(
        pane.placement,
        crate::CandidatePanePlacement::Pending {
            folder_check: Some(bae_identify::FolderCheck::MediumDisagrees { folder: conflict }),
            records: Vec::new(),
        }
    );
    let detail = tables
        .load_import_candidate(&crate::CandidateKey::folder(&candidate.path))
        .await
        .unwrap()
        .expect("the candidate reads back");
    let bae_identify::IdentifyState::Found { context, .. } = &detail.resumed_identify_state else {
        panic!("a found verdict resumes as Found, got {:?}", detail.resumed_identify_state);
    };
    assert_eq!(
        context.origin,
        bae_catalogs::AudioOrigin { source: None, not_cd_rate: Some(96_000) },
        "the resumed state stands up beside its stored signals' origin"
    );
    assert_eq!(detail.resumed_identify_state.rate_ruling_out_cd(), Some(96_000));
}

/// The detail resumes the stored verdict's matches.
#[tokio::test]
async fn the_detail_resumes_the_stored_verdict() {
    let (db, _tmp, root) = watched_root().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let candidate = scanned(&db, &root, "Album").await;
    save_verdict(&db, &candidate, "mb-verdict").await;

    let detail = tables
        .load_import_candidate(&crate::CandidateKey::folder(&candidate.path))
        .await
        .unwrap()
        .expect("the scanned candidate reads back");

    let bae_identify::IdentifyState::Found {
        findings: bae_identify::Findings { matches, .. }, ..
    } = &detail.resumed_identify_state
    else {
        panic!("expected the stored Found, got {:?}", detail.resumed_identify_state);
    };
    assert_eq!(
        matches.iter().map(|result| result.release_id.as_str()).collect::<Vec<_>>(),
        vec!["mb-verdict"]
    );
}

/// The detail resumes the ledger stored with the verdict.
#[tokio::test]
async fn the_detail_resumes_the_ledger_the_run_recorded() {
    let (db, _tmp, root) = watched_root().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let candidate = scanned(&db, &root, "Album").await;
    // The disc ID's lookup found the verdict's release, as the run last read
    // it.
    let found = verdict("mb-verdict", None)
        .findings()
        .expect("a found verdict holds findings")
        .matches
        .clone();
    let ledger = bae_identify::IdentifyRunView {
        providers: vec![LookupCatalog::MusicBrainz],
        disc_id: bae_identify::DiscIdStepView::Read {
            discs: vec![bae_identify::DiscIdRow {
                disc_id: "disc-1".to_string(),
                lookup: bae_identify::LookupView::answered(&found),
            }],
        },
        barcode: bae_identify::BarcodeStepView::Absent,
        catalog: bae_identify::CatalogStepView::NoneFound,
        isrc: bae_identify::IsrcStepView::Absent,
        search: bae_identify::SearchStepView::NotNeeded,
    };
    save_verdict_with_ledger(&db, &candidate, "mb-verdict", Some(ledger.clone())).await;

    let detail = tables
        .load_import_candidate(&crate::CandidateKey::folder(&candidate.path))
        .await
        .unwrap()
        .expect("the scanned candidate reads back");

    let bae_identify::IdentifyStateView::Found { run: Some(run), .. } =
        bae_identify::IdentifyStateView::from(detail.resumed_identify_state)
    else {
        panic!("a stored Found resumes with the ledger its run recorded");
    };
    assert_eq!(run, ledger);
    assert_eq!(run.providers, vec![LookupCatalog::MusicBrainz]);
    let bae_identify::DiscIdStepView::Read { discs } = run.disc_id else {
        panic!("the disc ID the run read");
    };
    assert!(matches!(discs.as_slice(), [bae_identify::DiscIdRow {
        disc_id,
        lookup: bae_identify::LookupView::Found { count: 1, .. },
    }] if disc_id == "disc-1"));
}

/// A verdict stored for another file-edit revision does not resume: one
/// stored over the folder's file decisions while a pass that read the folder
/// before they landed lists it.
#[tokio::test]
async fn a_verdict_from_another_revision_does_not_resume() {
    let lib = Library::new().await;
    let tables = crate::tables::ImportTables::new(lib.db.clone());
    lib.run(
        lib.scan("Album")
            + lib.file_decision("Album")
            + lib.scan_revised("Album", 0)
            + lib.concluded_at("Album", verdict("mb-verdict", None), 1),
    )
    .await;
    let key = lib.key("Album");

    let detail = tables
        .load_import_candidate(&key)
        .await
        .unwrap()
        .expect("the scanned candidate still reads back");

    assert!(matches!(detail.resumed_identify_state, bae_identify::IdentifyState::Idle));
    assert!(detail.verdict.is_none());
}

/// The pane of a candidate its import put in the library is placed Done, which
/// carries nothing of the candidate's draft: its folder check and its draft's
/// catalogs belong to a candidate still in the queue. Its import status names
/// the release imported from it, and it draws the folder's files from its
/// scan, holding no draft.
#[tokio::test]
async fn the_pane_of_an_imported_candidate_is_placed_done() {
    let lib = Library::new().await;
    let db = lib.db.clone();
    lib.run(lib.scan("release")).await;
    let candidate = lib.candidate("release");
    let key = candidate.key();
    let detail = |db: Store, key: crate::CandidateKey| async move {
        let tables = crate::tables::ImportTables::new(db);
        tables
            .load_import_candidate(&key)
            .await
            .unwrap()
            .expect("the scanned candidate has a pane")
            .resolve()
    };

    assert!(matches!(
        detail(db.clone(), key.clone()).await.placement,
        crate::CandidatePanePlacement::Pending { .. }
    ));

    lib.run(lib.imported("release", "2026-01-01T00:00:00Z")).await;
    let (release_id, album_id) = (lib.release_id("release"), lib.album_id("release"));
    let done = detail(db.clone(), key).await;
    assert_eq!(done.placement, crate::CandidatePanePlacement::Done);
    assert_eq!(
        done.import_status,
        Some(crate::CandidateImportStatus::Complete {
            release: crate::ImportedRelease {
                release_id: release_id.clone(),
                album_id: album_id.clone(),
            },
        })
    );
    assert!(!done.live.actions.contains(&crate::CandidateAction::Import));
    assert_eq!(
        done.stage,
        crate::CandidateStage::Imported {
            release: crate::ImportedRelease {
                release_id: release_id.clone(),
                album_id: album_id.clone(),
            },
            folder: crate::mapping::folder_files(&candidate.files),
        },
        "an imported candidate's pane reads no draft, whatever rows it holds"
    );
}

#[tokio::test]
async fn an_active_import_omits_its_previous_persisted_failure_from_the_detail() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let (_, hash) = stored_pane_candidate(&db).await;
    tables
        .save_import_candidate_failure(
            &hash,
            0,
            &ImportFailure::error_only("the prior attempt failed", fixed_now()),
        )
        .await
        .unwrap();

    let key = pane_candidate_key();
    tables.claim_candidate_import(&key, "import-1", None).await.unwrap();
    tables
        .move_candidate_import(
            &key,
            "import-1",
            Some(crate::ImportStep::Running(crate::ImportPhase::ReadingFiles)),
        )
        .await
        .unwrap();
    let detail = tables
        .load_import_candidate(&key)
        .await
        .unwrap()
        .expect("the stored candidate has a detail")
        .resolve();

    assert!(detail.live.facts.importing());
    assert_eq!(
        detail.live.actions,
        vec![crate::CandidateAction::CancelImport, crate::CandidateAction::RevealFolder]
    );
    assert!(detail.draft().unwrap().failure.is_none());
}

/// The pane draws a queued import differently from a running one, and names
/// a running one's step, so its import status says the step the import is at.
#[tokio::test]
async fn the_pane_s_import_status_says_the_step_the_import_is_at() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    stored_pane_candidate(&db).await;
    let key = pane_candidate_key();

    tables.claim_candidate_import(&key, "import-1", None).await.unwrap();
    for step in [
        crate::ImportStep::Preparing(crate::PrepareStep::Queued),
        crate::ImportStep::Preparing(crate::PrepareStep::ValidatingSourceFiles),
        crate::ImportStep::Running(crate::ImportPhase::MeasuringLoudness),
        crate::ImportStep::Running(crate::ImportPhase::Finalizing),
    ] {
        tables.move_candidate_import(&key, "import-1", Some(step)).await.unwrap();
        let detail = tables
            .load_import_candidate(&key)
            .await
            .unwrap()
            .expect("the stored candidate has a detail")
            .resolve();
        assert_eq!(detail.import_status, Some(crate::CandidateImportStatus::Importing { step }));
    }
}

/// The pane's candidate read moves between candidates on one subscription:
/// each value answers the key set last, and no key reads nothing.
#[tokio::test]
async fn import_candidate_moves_between_candidates_on_one_subscription() {
    let (db, _temp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = &crate::tables::watched_path::host_root("/music");
    tables.add_watched_import_folder(root).await.unwrap();
    let generation = tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();
    let mut keys = Vec::new();
    for name in ["first", "second"] {
        let item = scan_candidate(root, name);
        let crate::folder_scanner::ScanItem::Valid(candidate) = &item else {
            unreachable!("the fixture builds a valid candidate");
        };
        keys.push(candidate.key());
        tables.save_folder_scan_item(root, generation, &item).await.unwrap();
    }
    tables.finish_folder_scan(root, generation, None).await.unwrap();

    let mut live = tables.subscribe_import_candidate(None);
    let requests = live.requests();
    assert!(
        live.next().await.expect("the query stays open").result.unwrap().is_none(),
        "no key reads nothing"
    );

    for (key, name) in keys.iter().zip(["first", "second"]) {
        requests.set(Some(key.clone())).unwrap();
        let read = live.next().await.expect("the query stays open");
        assert_eq!(read.request.as_ref(), Some(key));
        let detail = read.result.unwrap().expect("the candidate reads");
        assert_eq!(detail.candidate.name, name);
    }

    requests.set(None).unwrap();
    assert!(live.next().await.expect("the query stays open").result.unwrap().is_none());
}

/// A folder that failed validation is listed under Skipped as what the scan
/// read of it — its files, what its tags call it, the image it leads with —
/// selected like any row when the list selects it, and read whole by the pane that looks into it. A
/// scan that finds it again finds it unchanged.
#[tokio::test]
async fn a_skipped_folder_is_listed_and_read_as_what_its_scan_read() {
    use crate::folder_scanner::{
        AudioDamage, InvalidCandidate, ProposedRole, SkippedFile, SkippedTags,
    };

    let (db, _tmp, root) = watched_root().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    tables.add_watched_import_folder(&root).await.unwrap();
    let folder = PathBuf::from(&root).join("Half Copied");
    let version =
        |name: &str, size| FileVersion { path: folder.join(name), size, modified_at_ns: 1 };
    let invalid = InvalidCandidate {
        path: folder.clone(),
        name: "Half Copied".to_string(),
        watched_folder_path: root.clone(),
        relative_folder: crate::RelativeFolder::test("Half Copied"),
        grouping: None,
        files: vec![
            SkippedFile {
                relative_path: "01.flac".to_string(),
                file: version("01.flac", 1_000),
                role: ProposedRole::Audio,
            },
            SkippedFile {
                relative_path: "02.flac".to_string(),
                file: version("02.flac", 4_096),
                role: ProposedRole::DamagedAudio { damage: AudioDamage::Unreadable },
            },
            SkippedFile {
                relative_path: "front.jpg".to_string(),
                file: version("front.jpg", 4),
                role: ProposedRole::Image,
            },
        ],
        tags: Some(SkippedTags {
            album_title: "Album Title".to_string(),
            artist: Some("Artist Name".to_string()),
        }),
    };
    let key = invalid.key();
    let generation = tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    tables
        .save_folder_scan_item(&root, generation, &ScanItem::Invalid(invalid.clone()))
        .await
        .unwrap();
    tables.finish_folder_scan(&root, generation, None).await.unwrap();
    let mut selecting = request(TriageTab::Skipped).await;
    selecting.selected.push(key.clone());

    let list = tables.load_import_list_read(selecting).await.unwrap();
    let listed: Vec<_> = list
        .windows
        .iter()
        .flat_map(|window| &window.items)
        .filter_map(|item| match item {
            ImportListItem::Invalid { candidate, selected, .. } => {
                Some((candidate.clone(), *selected))
            }
            _ => None,
        })
        .collect();
    assert_eq!(listed, [(invalid.clone(), true)]);
    assert_eq!(
        invalid.cover_image(),
        Some(crate::CoverImageSource::Local { file: version("front.jpg", 4) })
    );

    let mut detail = tables.subscribe_invalid_candidate(Some(key.clone()));
    let read = detail
        .next()
        .await
        .expect("the query stays open")
        .result
        .unwrap()
        .expect("the key names a folder that failed validation");
    assert_eq!(read.candidate, invalid);
    assert_eq!(
        read.images.iter().map(|image| image.file_id.as_str()).collect::<Vec<_>>(),
        ["front.jpg"]
    );
    assert_eq!(
        read.files.iter().map(|file| (file.file_id.as_str(), file.role)).collect::<Vec<_>>(),
        [
            ("01.flac", crate::MappingRole::Audio),
            ("02.flac", crate::MappingRole::DamagedAudio { damage: AudioDamage::Unreadable }),
        ]
    );

    let generation = tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    let again = tables
        .save_folder_scan_item(&root, generation, &ScanItem::Invalid(invalid))
        .await
        .unwrap()
        .expect("the scan entry is written");
    assert!(!again.changed(), "the folder read the same is stored unchanged");
}

/// The pane looking into a folder that failed validation reads nothing for a
/// folder that passed it: the key is answered by its kind, without reading
/// the folder whole.
#[tokio::test]
async fn the_skipped_pane_reads_nothing_for_a_valid_folder() {
    let (db, _tmp, root) = watched_root().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let valid = scanned(&db, &root, "Passed").await;

    let read = tables
        .subscribe_invalid_candidate(Some(valid.key()))
        .next()
        .await
        .expect("the query stays open")
        .result
        .unwrap();
    assert!(read.is_none(), "{read:?}");
}
