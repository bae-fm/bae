//! What one settled answer does to its job: the job ends, and a run that
//! stores an auto-importable verdict imports its candidate when "Import
//! automatically when identified" is on, or when a person asked for the
//! import while the pick was read.

use super::*;
use crate::identification::tests::*;
use bae_catalogs::test_releases::{mb_barcode_search_json, mb_release_json};
use bae_util::test_channels::HANG_GUARD;
use std::path::PathBuf;
use std::sync::Arc;

/// Every import the worker reports on for `key` until one ends: the ids it
/// reported under, and the error the ending one failed with, if it failed.
async fn await_import_ending(
    events: &mut crate::ImportEvents,
    key: &crate::CandidateKey,
) -> (std::collections::BTreeSet<String>, Option<String>) {
    let mut ids = std::collections::BTreeSet::new();
    tokio::time::timeout(HANG_GUARD, async {
        loop {
            let event = events.recv().await.expect("the import event bus stays open");
            let ImportEvent::ImportProgress { candidate_key, progress } = event else {
                continue;
            };
            if candidate_key != key.clone() {
                continue;
            }
            match progress {
                crate::ImportProgress::Preparing { import_id, .. }
                | crate::ImportProgress::Progress { import_id, .. }
                | crate::ImportProgress::RemoteUploadQueued { import_id, .. } => {
                    ids.insert(import_id);
                }
                crate::ImportProgress::Complete { import_id, .. } => {
                    ids.insert(import_id);
                    return None;
                }
                crate::ImportProgress::Failed { import_id, error } => {
                    ids.insert(import_id);
                    return Some(error);
                }
                crate::ImportProgress::Cancelled { import_id } => {
                    ids.insert(import_id);
                    return Some("cancelled".to_string());
                }
            }
        }
    })
    .await
    .map(|failure| (ids, failure))
    .expect("an import of the candidate ends")
}

impl Fixture {
    /// A disc-ID candidate whose one release `release_id` the run picks
    /// unattended, with the settle's read of that release held, and its run
    /// asked for and reading the pick. Returns its folder and key.
    async fn reading_its_pick(
        &self,
        release_id: &str,
        group_id: &str,
    ) -> (PathBuf, crate::CandidateKey) {
        self.set_identify_automatically(false).await;
        let dir = self.disc_id_candidate("Album");
        let key = crate::CandidateKey::folder(&dir);
        self.route_disc_id_match(&dir, release_id, group_id, 2);
        self.provider.hold(&format!("/release/{release_id}?"));
        self.scan(1).await;
        self.start_explicit_lookup(&dir);
        self.await_identification_status(
            &key,
            crate::IdentificationStatus::ReadingPick { import_asked: false },
        )
        .await;
        (dir, key)
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn an_automatic_run_that_settles_auto_importable_imports_its_candidate_once() {
    let fixture = Fixture::importing("auto-importable").await;
    fixture.set_import_when_identified(true).await;
    let dir = fixture.disc_id_candidate("Album");
    let key = crate::CandidateKey::folder(&dir);
    fixture.route_disc_id_match(&dir, "mb-auto", "rg-auto", 2);
    fixture.scan(1).await;
    let mut events = fixture.import.every_event();

    fixture.drain_automatic().await;
    assert_eq!(fixture.judgement_for(&dir).await, (true, None));

    let (imports, failure) = await_import_ending(&mut events, &key).await;
    assert_eq!(imports.len(), 1, "one import of the candidate: {imports:?}");
    assert_eq!(failure, None, "the import completes");

    fixture.rescan(&fixture.import, 1).await;
    fixture.drain_automatic().await;
    assert_no_import(&fixture, &mut events, &key, "the watched folder was read again").await;
}

/// With the setting off, an auto-importable verdict waits for a person.
#[tokio::test(flavor = "multi_thread")]
async fn an_automatic_run_that_settles_auto_importable_with_the_setting_off_imports_nothing() {
    let fixture = Fixture::new("auto-import-setting-off").await;
    let dir = fixture.disc_id_candidate("Album");
    let key = crate::CandidateKey::folder(&dir);
    fixture.route_disc_id_match(&dir, "mb-setting-off", "rg-setting-off", 2);
    fixture.scan(1).await;
    let mut events = fixture.import.every_event();

    fixture.drain_automatic().await;

    assert_eq!(fixture.judgement_for(&dir).await, (true, None));
    assert_no_import(&fixture, &mut events, &key, "the run settled with the setting off").await;
}

/// Two pressings are a question for the person, so nothing is imported.
#[tokio::test(flavor = "multi_thread")]
async fn an_automatic_run_that_settles_needing_you_imports_nothing() {
    let fixture = Fixture::new("auto-import-needs-you").await;
    fixture.set_import_when_identified(true).await;
    fixture.import.register_artwork_analyzer(Arc::new(BarcodeAnalyzer {
        barcode: PAIRED_BARCODE.to_string(),
    }));
    let dir = fixture.barcode_candidate("Album");
    let key = crate::CandidateKey::folder(&dir);
    fixture.provider.route(
        "/release?",
        200,
        mb_barcode_search_json(&[
            ("mb-two-1", "rg-two-1", PAIRED_BARCODE),
            ("mb-two-2", "rg-two-1", "9876543210987"),
        ]),
    );
    fixture.scan(1).await;
    let mut events = fixture.import.every_event();

    fixture.drain_automatic().await;

    assert_eq!(fixture.judgement_for(&dir).await, (false, None));
    assert_no_import(&fixture, &mut events, &key, "the run settled needing a person").await;
}

/// Turning the setting on imports only what settles from then on: a candidate
/// already identified and ready stays where it is for the person.
#[tokio::test(flavor = "multi_thread")]
async fn a_candidate_auto_importable_before_the_setting_was_on_is_not_imported() {
    let fixture = Fixture::new("auto-import-earlier").await;
    let dir = fixture.disc_id_candidate("Album");
    let key = crate::CandidateKey::folder(&dir);
    fixture.identify_ready(&dir, "mb-earlier", "rg-earlier").await;
    let mut events = fixture.import.every_event();

    fixture.set_import_when_identified(true).await;
    fixture.rescan(&fixture.import, 1).await;
    fixture.drain_automatic().await;

    assert_no_import(&fixture, &mut events, &key, "the setting was turned on").await;
}

/// Identify or a retry is a run like an automatic one: what it picks
/// unattended is imported, whether or not identification also runs on its own.
#[tokio::test(flavor = "multi_thread")]
async fn a_run_a_person_asked_for_imports_its_candidate_with_identifying_automatically_off() {
    let fixture = Fixture::importing("auto-import-requested").await;
    fixture.set_identify_automatically(false).await;
    fixture.set_import_when_identified(true).await;
    let dir = fixture.disc_id_candidate("Album");
    let key = crate::CandidateKey::folder(&dir);
    fixture.route_disc_id_match(&dir, "mb-requested", "rg-requested", 2);
    fixture.scan(1).await;
    let mut events = fixture.import.every_event();

    fixture.start_explicit_lookup(&dir);

    let (imports, failure) = await_import_ending(&mut events, &key).await;
    assert_eq!(imports.len(), 1, "one import of the candidate: {imports:?}");
    assert_eq!(failure, None, "the import completes");
}

/// With the setting off, what a person's run picks unattended waits for them.
#[tokio::test(flavor = "multi_thread")]
async fn a_run_a_person_asked_for_with_the_setting_off_imports_nothing() {
    let fixture = Fixture::new("auto-import-requested-setting-off").await;
    fixture.set_identify_automatically(false).await;
    let dir = fixture.disc_id_candidate("Album");
    let key = crate::CandidateKey::folder(&dir);
    fixture.route_disc_id_match(&dir, "mb-requested-off", "rg-requested-off", 2);
    fixture.scan(1).await;
    let mut events = fixture.import.every_event();

    fixture.start_explicit_lookup(&dir);
    tokio::time::timeout(HANG_GUARD, fixture.identification().drained_for_test())
        .await
        .expect("the person's run ends");

    assert_eq!(fixture.judgement_for(&dir).await, (true, None));
    assert_no_import(&fixture, &mut events, &key, "the person's run settled with the setting off")
        .await;
}

/// Import from the pane while the pick is read does not cancel the read nor
/// import the draft stored now: it waits, and the candidate imports once,
/// after the pick is stored, with importing when identified off.
#[tokio::test(flavor = "multi_thread")]
async fn import_from_the_pane_while_the_pick_is_read_imports_it_once_stored() {
    let fixture = Fixture::importing("import-asked-from-the-pane").await;
    let (dir, key) = fixture.reading_its_pick("mb-pane", "rg-pane").await;
    let mut events = fixture.import.every_event();

    let started = fixture.import.start_import(&key).await.expect("the import is asked for");
    assert_eq!(started, crate::ImportStart::WhenPickStored);
    assert_eq!(
        fixture.identification_status(&key).await,
        Some(crate::IdentificationStatus::ReadingPick { import_asked: true }),
        "the read goes on, asked to import"
    );
    assert!(
        fixture
            .import
            .candidate_runtime(&key)
            .await
            .unwrap()
            .is_some_and(|runtime| runtime.import.is_none()),
        "nothing is claimed while the pick is read"
    );

    fixture.provider.release();
    let (imports, failure) = await_import_ending(&mut events, &key).await;
    assert_eq!(imports.len(), 1, "one import of the candidate: {imports:?}");
    assert_eq!(failure, None, "the import completes");
    let row = fixture.stored_for(&dir).await.expect("the candidate's row");
    assert!(
        row.identify.is_none(),
        "the import's write deletes the verdict it imported the pick of"
    );
    assert_eq!(
        fixture
            .db
            .query_texts_for_test("SELECT catalog || ' ' || key FROM release_records")
            .await
            .unwrap(),
        vec!["musicbrainz mb-pane".to_string()],
        "what imported is the pick's draft, read from the picked release"
    );

    tokio::time::timeout(HANG_GUARD, fixture.identification().drained_for_test())
        .await
        .expect("the queue has nothing left to do");
    assert_no_import(&fixture, &mut events, &key, "the asked-for import ended").await;
}

/// Import of a selection reaching a row whose pick is read asks the same,
/// rather than refusing the row as one being identified.
#[tokio::test(flavor = "multi_thread")]
async fn importing_a_selection_while_the_pick_is_read_imports_it_once_stored() {
    let fixture = Fixture::importing("import-asked-from-a-selection").await;
    let (_dir, key) = fixture.reading_its_pick("mb-selection", "rg-selection").await;
    let mut events = fixture.import.every_event();

    let started =
        fixture.import.import_selected(&key).await.expect("the selected row is not refused");
    assert_eq!(started, crate::ImportStart::WhenPickStored);

    fixture.provider.release();
    let (imports, failure) = await_import_ending(&mut events, &key).await;
    assert_eq!(imports.len(), 1, "one import of the candidate: {imports:?}");
    assert_eq!(failure, None, "the import completes");

    tokio::time::timeout(HANG_GUARD, fixture.identification().drained_for_test())
        .await
        .expect("the queue has nothing left to do");
    assert_no_import(&fixture, &mut events, &key, "the asked-for import ended").await;
}

/// A read that fails stores no pick, so the Import asked for while it ran
/// imports nothing.
#[tokio::test(flavor = "multi_thread")]
async fn a_read_that_fails_after_import_was_asked_imports_nothing() {
    let fixture = Fixture::importing("import-asked-read-fails").await;
    fixture.set_identify_automatically(false).await;
    let dir = fixture.disc_id_candidate("Album");
    let key = crate::CandidateKey::folder(&dir);
    // The run reads its release from the Disc ID answer; the pick's read of
    // the release itself is refused.
    fixture.provider.route("/release/mb-fails?", 400, "{}");
    fixture.route_disc_id_match(&dir, "mb-fails", "rg-fails", 2);
    fixture.provider.hold("/release/mb-fails?");
    fixture.scan(1).await;
    fixture.start_explicit_lookup(&dir);
    fixture
        .await_identification_status(
            &key,
            crate::IdentificationStatus::ReadingPick { import_asked: false },
        )
        .await;
    let mut events = fixture.import.every_event();

    assert_eq!(
        fixture.import.start_import(&key).await.expect("the import is asked for"),
        crate::ImportStart::WhenPickStored
    );

    fixture.finish_reading().await;
    assert_ne!(
        fixture.stored_for(&dir).await.map(|row| row.metadata_author),
        Some(crate::MetadataAuthor::Identification),
        "no pick was stored"
    );
    assert_no_import(&fixture, &mut events, &key, "the pick's read failed").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn identified_progress_is_emitted_after_the_verdict_is_committed() {
    let fixture = Fixture::new("progress-after-commit").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-progress-commit", "rg-progress-commit", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-progress-commit?",
        200,
        mb_release_json("mb-progress-commit", "rg-progress-commit", &[probed, 0]),
    );
    fixture.scan(1).await;

    let mut counts = fixture.import.every_identification_count_for_test();
    let pass = fixture.drain_automatic_task();

    let mut opened = false;
    loop {
        let count = tokio::time::timeout(HANG_GUARD, counts.recv())
            .await
            .expect("identification progress arrives")
            .expect("the count stays open");
        match count {
            (0, 1) => opened = true,
            (0, 0) if opened => {
                assert!(
                    fixture.identified_for(&dir).await.is_some(),
                    "the identification result must be readable before the batch ends"
                );
                break;
            }
            _ => continue,
        }
    }

    pass.await.expect("the drain joins");
}

/// A finished candidate leaves nothing behind: its driver deregisters at its
/// verdict, and the queue holds no entry for it.
#[tokio::test(flavor = "multi_thread")]
async fn a_finished_candidate_leaves_no_driver_behind() {
    let fixture = Fixture::new("no-driver-left").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route("/discid/", 200, discid_json("mb-drv-1", "rg-drv-1", &[probed, 0]));
    fixture.provider.route(
        "/release/mb-drv-1?",
        200,
        mb_release_json("mb-drv-1", "rg-drv-1", &[probed, 0]),
    );
    fixture.scan(1).await;
    let key = crate::CandidateKey::folder(&dir);

    fixture.drain_automatic().await;

    assert!(fixture.identified_for(&dir).await.is_some(), "the candidate really was identified");
    assert!(
        !fixture.import.is_identifying(&key),
        "and its driver is gone: the run ended at the verdict it reached"
    );
    assert_eq!(
        fixture.identification_status(&key).await,
        None,
        "and the queue holds nothing for a candidate it has finished with"
    );
}

/// The setting that decides an automatic import failing to read is recorded
/// as the candidate's failed import, as any other refused start is.
#[tokio::test(flavor = "multi_thread")]
async fn an_unreadable_import_setting_is_the_candidates_failed_import() {
    let fixture = Fixture::importing("import-setting-unreadable").await;
    fixture.set_identify_automatically(false).await;
    let dir = fixture.disc_id_candidate("Album");
    let key = crate::CandidateKey::folder(&dir);
    fixture.scan(1).await;
    bae_preferences::plant_unknown_replay_gain_mode_for_test(fixture._temp.path());

    import_when_identified(&fixture.context, &key, false).await;

    let pane = fixture
        .context
        .tables
        .load_import_candidate_pane_rows(&fixture.content_hash(&dir))
        .await
        .unwrap();
    assert!(pane.failure.is_some(), "the refused start is recorded");
}
