//! A verdict's write: refused for a claimed candidate, landing whatever the
//! run's caller does, taking over from the run's recorded state, and where it
//! leaves the pane.

use super::*;
use crate::identification::tests::*;
use crate::tables::NewImportCandidateVerdict;
use crate::IdentifyRunId;
use bae_catalogs::search::{MetadataResult, SourceTracks};
use bae_catalogs::test_releases::{mb_release_json, mb_search_json};
use bae_identify::signals::{DiscIdSignal, Signals};
use bae_identify::TerminalVerdict;
use bae_util::test_channels::HANG_GUARD;
use std::path::Path;
use std::sync::Arc;

impl Fixture {
    /// Store the verdict a settled lead produces, without running the pipeline.
    pub(crate) async fn store_settled_verdict(&self, dir: &Path, release_id: &str, group_id: &str) {
        self.store_settled_verdict_listing(
            dir,
            release_id,
            group_id,
            SourceTracks::Listed { count: 2 },
        )
        .await;
    }

    /// `store_settled_verdict`, with the lead stating `source_tracks`.
    pub(crate) async fn store_settled_verdict_listing(
        &self,
        dir: &Path,
        release_id: &str,
        group_id: &str,
        source_tracks: SourceTracks,
    ) {
        self.store_verdict_settled_on(
            dir,
            release_id,
            group_id,
            source_tracks,
            SettledDraft::Picked,
        )
        .await;
    }

    async fn store_verdict_settled_on(
        &self,
        dir: &Path,
        release_id: &str,
        group_id: &str,
        source_tracks: SourceTracks,
        settled_draft: SettledDraft,
    ) {
        let candidate = self
            .import
            .answerable_candidate(&crate::CandidateKey::folder(dir))
            .await
            .expect("the candidate state is readable")
            .expect("the scanned candidate is answerable");
        let mut draft = candidate.blank_draft();
        draft.album_title = "Album".to_string();
        draft.album_artist_assignments =
            vec![crate::Credited::plain(crate::ArtistAssignment::Credit {
                credit: bae_catalogs::credits::ArtistCredit {
                    credited_as: None,
                    name: "Artist".to_string(),
                    sort_name: None,
                    musicbrainz_artist_id: None,
                    discogs_artist_id: None,
                },
            })];
        for (index, track) in draft.tracks.iter_mut().enumerate() {
            track.edit.title = format!("Track {}", index + 1);
        }
        let verdict = TerminalVerdict::Found {
            findings: bae_identify::Findings {
                matches: vec![MetadataResult {
                    source: bae_catalogs::catalog::LookupCatalog::MusicBrainz,
                    release_id: release_id.to_string(),
                    title: "Album".to_string(),
                    artist: Some(bae_types::credits::CreditLine::of_name("Artist")),
                    year: None,
                    labels: Vec::new(),
                    area: None,
                    status: None,
                    packaging: None,
                    discogs_details: Vec::new(),
                    barcodes: Vec::new(),
                    media: bae_types::pressing::StatedMedia::Undescribed,
                    links: Vec::new(),
                    cover_art: None,
                    source_group_id: Some(group_id.to_string()),
                    album_links: bae_catalogs::album_links::AlbumLinks::NotAsked,
                    source_tracks: Some(source_tracks),
                    document_failure: None,
                    album_first_year: None,
                    track_titles: Vec::new(),
                    notes: Vec::new(),
                }],
                pressings: vec![0],
                narrowed_out: bae_identify::NarrowedOut::default(),
                medium_conflict: None,
                named_notes: Vec::new(),
            },
            track_count: 2,
            ledger: None,
        };
        let wrote = self
            .import
            .save_candidate_verdict_if_current(
                &crate::CandidateKey::folder(dir),
                IdentifyRunId::for_test(1),
                &NewImportCandidateVerdict {
                    content_hash: self.content_hash(dir),
                    file_edit_revision: 0,
                    candidate_key: crate::CandidateKey::folder(dir),
                    verdict,
                    // A disc ID naming its log, for the evidence chip.
                    signals: Signals {
                        disc_id: DiscIdSignal::Computed {
                            discs: vec![bae_identify::signals::ComputedDiscId {
                                disc_id: SEEDED_DISC_ID.to_string(),
                                source_file: Some(SEEDED_DISC_ID_FILE.to_string()),
                            }],
                        },
                        ..settled_signals()
                    },
                    pick: matches!(settled_draft, SettledDraft::Picked).then(|| {
                        let record = bae_catalogs::records::LookupRef::new(
                            bae_catalogs::catalog::LookupCatalog::MusicBrainz,
                            release_id.to_string(),
                        );
                        crate::tables::VerdictPick {
                            link: bae_identify::PressingLink {
                                record: record.clone(),
                                partners: vec![],
                            },
                            metadata: crate::CandidateMetadataDraft {
                                draft,
                                provenance: Some(crate::MetadataProvenance::ExternalRelease {
                                    record,
                                }),
                                cover: None,
                            },
                        }
                    }),
                    releases: Vec::new(),
                },
            )
            .await
            .unwrap();
        assert!(matches!(wrote, crate::VerdictWrite::Stored { .. }), "the seeded verdict lands");
    }

    /// The verdict a settled lead produces with no pick behind it: the lead
    /// names `release_id`, and the draft is left as it is.
    pub(crate) async fn store_settled_lead_without_its_pick(
        &self,
        dir: &Path,
        release_id: &str,
        group_id: &str,
    ) {
        self.store_verdict_settled_on(
            dir,
            release_id,
            group_id,
            SourceTracks::Listed { count: 2 },
            SettledDraft::Untouched,
        )
        .await;
    }
}

/// Whether a seeded settled verdict carries the pick a settle writes with it.
enum SettledDraft {
    Picked,
    Untouched,
}

/// A verdict is refused for a candidate an import has claimed. The claim and
/// the check share the folder-state commit lock, so by the time a claim
/// returns there is no interval left in which a verdict can be stored for a
/// candidate the user has already committed to importing — and a verdict that
/// did land would describe files the import is in the middle of consuming.
#[tokio::test(flavor = "multi_thread")]
async fn a_verdict_is_refused_for_a_claimed_candidate() {
    let fixture = Fixture::new("verdict-refused-when-claimed").await;
    let dir = fixture.disc_id_candidate("Album");
    fixture.scan(1).await;
    let key = crate::CandidateKey::folder(&dir);
    let row = || NewImportCandidateVerdict {
        content_hash: fixture.content_hash(&dir),
        file_edit_revision: 0,
        candidate_key: key.clone(),
        verdict: multi_match_verdict(&["mb-claimed-1"], "rg-claimed-1"),
        signals: settled_signals(),
        pick: None,
        releases: Vec::new(),
    };

    assert!(
        matches!(
            fixture
                .import
                .save_candidate_verdict_if_current(&key, IdentifyRunId::for_test(1), &row())
                .await
                .unwrap(),
            crate::VerdictWrite::Stored { .. }
        ),
        "an unclaimed candidate still takes its verdict"
    );

    fixture.import.claim_candidate_for_import(&key, "import-1").await;

    assert_eq!(
        fixture
            .import
            .save_candidate_verdict_if_current(&key, IdentifyRunId::for_test(1), &row())
            .await
            .unwrap(),
        crate::VerdictWrite::Refused,
        "a claimed candidate refuses a verdict"
    );
}

/// Coven commits a write on its writer thread whether or not the future that
/// asked for it survives, and the candidate runtime holds a run's answer until
/// its write says the answer has landed. So the write has to end that wait
/// itself, not its caller: a task torn down the instant it has asked — the
/// app shutting down over a settle in flight — must still leave the key
/// stating what happened, or the row reads as a commit still pending, for
/// good, and offers the candidate nothing but Skip.
#[tokio::test(flavor = "multi_thread")]
async fn a_verdict_write_ends_its_own_save_when_its_caller_is_torn_down() {
    use std::future::Future;

    let fixture = Fixture::new("torn-down-writer").await;
    let dir = fixture.disc_id_candidate("Album");
    fixture.scan(1).await;
    let key = crate::CandidateKey::folder(&dir);
    let run = IdentifyRunId::for_test(1);
    let row = NewImportCandidateVerdict {
        content_hash: fixture.content_hash(&dir),
        file_edit_revision: 0,
        candidate_key: key.clone(),
        verdict: multi_match_verdict(&["mb-torn-1", "mb-torn-2"], "rg-torn-1"),
        signals: settled_signals(),
        pick: None,
        releases: Vec::new(),
    };

    // The run's terminal state is what puts the key on a pending save.
    fixture
        .import
        .hold_answer_for_test(
            &key,
            run,
            &row.verdict.clone().resume_state(
                Some(&row.signals),
                &bae_identify::LookupChoices::default(),
                bae_identify::signals::AudioFacts::default(),
            ),
        )
        .await;
    assert!(matches!(
        fixture.identification_status(&key).await,
        Some(crate::IdentificationStatus::Finalizing)
    ));

    // One poll asks for the write; dropping the future at the end of the block
    // is the caller being torn down.
    {
        let mut save =
            std::pin::pin!(fixture.import.save_candidate_verdict_if_current(&key, run, &row));
        let first_poll =
            std::future::poll_fn(|cx| std::task::Poll::Ready(save.as_mut().poll(cx))).await;
        assert!(first_poll.is_pending(), "the first poll asks for the write and waits on it");
    }

    fixture.await_not_standing(&[key]).await;
    assert!(
        fixture.stored_for(&dir).await.is_some_and(|row| row.identify.is_some()),
        "the verdict landed"
    );
}

/// Once a run's verdict lands in its row, the recorded runtime state clears:
/// the row owns the answer, and Lookup serves it from there. Nothing in memory
/// is left to shadow a row that later changes.
#[tokio::test(flavor = "multi_thread")]
async fn a_stored_verdict_takes_over_from_the_recorded_runtime_state() {
    let fixture = Fixture::new("verdict-takes-over").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route("/discid/", 200, discid_json("mb-own-1", "rg-own-1", &[probed, 0]));
    fixture.provider.route(
        "/release/mb-own-1?",
        200,
        mb_release_json("mb-own-1", "rg-own-1", &[probed, 0]),
    );
    fixture.scan(1).await;
    let key = crate::CandidateKey::folder(&dir);

    fixture.drain_automatic().await;

    assert!(fixture.identified_for(&dir).await.is_some(), "the candidate really was identified");
    let pane = fixture.pane(&dir).await.expect("the identified candidate reads back");
    assert_eq!(
        pane.draft().unwrap().metadata_draft.album_title,
        "Album",
        "the automatic release choice seeds the editable metadata draft"
    );
    fixture.await_no_runtime(&key).await;
}

/// A run that applied its own pick and whose release passed every check
/// against the folder leaves the draft and its Import as all there is to see, so the stored pane moves to
/// the draft in the same write — from Find online, where the person was
/// watching the run.
#[tokio::test(flavor = "multi_thread")]
async fn a_settled_pick_that_asks_nothing_opens_the_pane_on_the_draft() {
    let fixture = Fixture::new("settled-draft").await;
    let dir = fixture.disc_id_candidate("Album");
    let key = crate::CandidateKey::folder(&dir);
    let probed = fixture.probed_total_ms(&dir);
    fixture.scan(1).await;
    fixture.archive("mb-settled-1", "rg-settled-1", &[probed, 0]).await;
    fixture.import.move_candidate_pane(&key, crate::PaneMove::FindOnline).await.unwrap();

    fixture.store_settled_verdict(&dir, "mb-settled-1", "rg-settled-1").await;

    assert_eq!(fixture.judgement_for(&dir).await, (true, None));
    assert_eq!(
        fixture
            .pane(&dir)
            .await
            .expect("the candidate reads back")
            .draft()
            .unwrap()
            .session
            .presentation,
        crate::MetadataPresentation::Draft
    );
}

/// A picked release that failed a check against the folder — here, it lists
/// no tracks to count against the folder's — leaves the pane where the person
/// left it.
#[tokio::test(flavor = "multi_thread")]
async fn a_settled_pick_that_asks_something_leaves_the_pane_where_it_was() {
    let fixture = Fixture::new("settled-asks").await;
    let dir = fixture.disc_id_candidate("Album");
    let key = crate::CandidateKey::folder(&dir);
    fixture.scan(1).await;
    fixture.archive("mb-settled-2", "rg-settled-2", &[0, 0]).await;
    fixture.import.move_candidate_pane(&key, crate::PaneMove::FindOnline).await.unwrap();

    fixture
        .store_settled_verdict_listing(
            &dir,
            "mb-settled-2",
            "rg-settled-2",
            bae_catalogs::search::SourceTracks::Nothing,
        )
        .await;

    assert_eq!(
        fixture.judgement_for(&dir).await,
        (false, Some(bae_identify::FolderCheck::SourceTracksUnknown))
    );
    assert_eq!(
        fixture
            .pane(&dir)
            .await
            .expect("the candidate reads back")
            .draft()
            .unwrap()
            .session
            .presentation,
        crate::MetadataPresentation::FindOnline
    );
}

/// An import started while a settled verdict's release is being fetched stops
/// its row. The queue does not cancel a settling answer; the write takes the
/// commit lock the claim was taken under, re-reads the candidate, and finds an
/// import owns it.
#[tokio::test(flavor = "multi_thread")]
async fn an_import_started_while_a_verdict_is_in_flight_stores_nothing() {
    let fixture = Fixture::new("import-mid-write").await;
    fixture.import.register_artwork_analyzer(Arc::new(BarcodeAnalyzer {
        barcode: "0123456789012".to_string(),
    }));
    let dir = fixture.barcode_candidate("From Barcode");
    let hash = fixture.content_hash(&dir);
    fixture.provider.route("/release?", 200, mb_search_json("mb-mid-write", "rg-mid-write"));
    fixture.provider.route(
        "/release/mb-mid-write?",
        200,
        mb_release_json("mb-mid-write", "rg-mid-write", &[1, 1]),
    );
    fixture.scan(1).await;
    // Holding the release fetch the settle makes before its write puts the
    // import start between the settled verdict and its row.
    fixture.provider.hold("/release/mb-mid-write?");

    let pass = fixture.drain_automatic_task();
    fixture.provider.wait_for_request("/release/mb-mid-write?", 1).await;
    start_import_for(&fixture, &dir).await;
    fixture.provider.release();
    tokio::time::timeout(HANG_GUARD, pass)
        .await
        .expect("pass finishes after the import claims the candidate")
        .unwrap();

    let state = fixture
        .stored()
        .await
        .remove(&hash)
        .expect("the discovered draft remains available to the import");
    assert!(
        state.identify.is_none(),
        "the in-flight verdict does not replace the importing candidate's draft"
    );
}

/// An edit made while a run is going does not refuse its answer: the run's
/// result is written when it finishes, over the edit, and no second run
/// starts.
#[tokio::test(flavor = "multi_thread")]
async fn an_edit_during_a_run_leaves_its_answer_to_land() {
    let fixture = Fixture::new("edit-during-run").await;
    let dir = fixture.disc_id_candidate("Album");
    let key = crate::CandidateKey::folder(&dir);
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route("/discid/", 200, discid_json("mb-1", "rg-1", &[probed, 0]));
    fixture.provider.route("/release/mb-1?", 200, mb_release_json("mb-1", "rg-1", &[probed, 0]));
    fixture.set_identify_automatically(false).await;
    fixture.scan(1).await;
    fixture.provider.hold("/release-group/rg-1/front");
    let mut events = fixture.import.every_event();

    fixture.start_explicit_lookup(&dir);
    fixture.provider.wait_for_request("/release-group/rg-1/front", 1).await;
    fixture
        .import
        .set_candidate_edit_field(
            &key,
            crate::DraftFieldEdit::Text {
                field: crate::CandidateEditField::AlbumTitle,
                value: "Retitled".to_string(),
            },
        )
        .await
        .unwrap();
    fixture.provider.release();

    let row = tokio::time::timeout(HANG_GUARD, fixture.await_identified_row(&dir))
        .await
        .expect("the run's answer lands");
    assert_eq!(row.metadata_author, crate::MetadataAuthor::Identification);
    assert_eq!(
        fixture
            .tables()
            .load_import_candidate_pane_rows(&fixture.content_hash(&dir))
            .await
            .unwrap()
            .draft
            .album_title,
        "Album",
        "the later write, the run's, wins"
    );
    let runs: std::collections::HashSet<IdentifyRunId> = drain_events(&mut events)
        .into_iter()
        .filter_map(|event| match event {
            ImportEvent::IdentifyStateChanged { candidate_key, run, .. }
                if candidate_key == key =>
            {
                Some(run)
            }
            _ => None,
        })
        .collect();
    assert_eq!(runs.len(), 1, "one run, and no second one");
}

/// A run reads as identifying in every delivery until the one that carries
/// its stored verdict: no delivery shows the row neither going nor answered,
/// or both.
#[tokio::test(flavor = "multi_thread")]
async fn a_run_reads_as_identifying_until_its_verdict_is_stored() {
    let fixture = Fixture::new("standing-run-to-verdict").await;
    fixture.set_identify_automatically(false).await;
    let dir = fixture.disc_id_candidate("Album");
    let key = crate::CandidateKey::folder(&dir);
    fixture.route_disc_id_match(&dir, "mb-standing", "rg-standing", 2);
    fixture.scan(1).await;

    let list = list_subscription(&fixture, page_window(0, 50)).await;
    let initial = next_page(&list).await;
    let (_, at_rest) = &page_rows(&initial)[&key];
    assert_eq!(at_rest.facts.identification, None);
    assert_eq!(at_rest.standing, Some(crate::PendingStanding::NotLookedUp));

    fixture.identification().rerun_identify(key.clone());
    let mut identifying = 0;
    let answered = loop {
        let snapshot = next_page(&list).await;
        let (row, live) = page_rows(&snapshot).remove(&key).expect("the row stays on Found");
        if live.facts.identifying() {
            assert!(
                row.action_basis.lookup.is_none(),
                "a delivery shows the stored verdict beside the run still going: {live:?}"
            );
            identifying += 1;
            continue;
        }
        if identifying == 0 {
            continue;
        }
        assert!(
            row.action_basis.lookup.is_some(),
            "a delivery shows the row neither identifying nor answered: {live:?}"
        );
        break live;
    };
    assert!(identifying > 0, "the run read as identifying");
    assert_eq!(answered.facts.identification, None);
    assert_eq!(answered.standing, Some(crate::PendingStanding::Identified));
}
