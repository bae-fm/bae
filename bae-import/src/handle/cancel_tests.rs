//! Cancelling an import: one waiting for the worker ends at once, one running
//! is dropped before it writes, and either leaves the candidate as it stood
//! before the import was asked for — nothing written, nothing running — ready
//! to be imported again.

use super::*;
use crate::handle::tests::*;
use bae_util::test_channels::HANG_GUARD;
use std::path::Path;
use std::path::PathBuf;
use tempfile::TempDir;

/// Two picked candidates with their artists named, on one handle: enough to
/// have one import running while another waits behind it.
async fn two_importable(
) -> (ImportServiceHandle, [TempDir; 2], crate::CandidateKey, crate::CandidateKey) {
    let (parts, database, tmp) = setup_test_import().await;
    let tables = crate::tables::ImportTables::new(database);
    let other = TempDir::new().unwrap();
    let (_, first, _) = picked_candidate(&tables, &tmp, "First Album").await;
    let (_, second, _) = picked_candidate(&tables, &other, "Second Album").await;
    let handle = parts.start().await.unwrap();
    for key in [&first, &second] {
        handle.select_candidate_file_tags(key.clone()).await.unwrap();
        handle
            .set_candidate_album_artists(
                key,
                vec![crate::Credited::plain(crate::ArtistAssignment::named("Artist"))],
            )
            .await
            .unwrap();
    }
    (handle, [tmp, other], first, second)
}

/// Wait for every one of `import_ids` to end cancelled, in whatever order
/// they end.
async fn await_cancelled(events: &mut crate::ImportEvents, import_ids: &[&str]) {
    let mut pending: std::collections::HashSet<String> =
        import_ids.iter().map(|id| id.to_string()).collect();
    while !pending.is_empty() {
        let event = tokio::time::timeout(HANG_GUARD, events.recv())
            .await
            .expect("the import reports its end")
            .expect("the import event stream remains open");
        match event {
            ImportEvent::ImportProgress {
                progress: crate::ImportProgress::Cancelled { import_id },
                ..
            } => {
                pending.remove(&import_id);
            }
            ImportEvent::ImportProgress {
                progress:
                    crate::ImportProgress::Complete { import_id, .. }
                    | crate::ImportProgress::Failed { import_id, .. },
                ..
            } if pending.contains(&import_id) => {
                panic!("the cancelled import {import_id} ended some other way")
            }
            _ => {}
        }
    }
}

/// What a cancelled import leaves: nothing running for the candidate and no
/// failure on its row.
async fn assert_left_as_it_stood(handle: &ImportServiceHandle, key: &crate::CandidateKey) {
    assert!(handle.candidate_runtime(key).await.unwrap().is_none(), "nothing is running for {key}");
    let pane = handle.load_candidate(key).await.unwrap().expect("the candidate reads back");
    assert!(pane.import_status.is_none(), "{key} records no failure");
}

/// The candidate imports once more, now that nothing holds it.
async fn assert_imports_again(handle: &ImportServiceHandle, key: &crate::CandidateKey) {
    let mut events = handle.every_event();
    let import_id = handle
        .start_import(key)
        .await
        .expect("a cancelled candidate imports again")
        .expect_queued();
    await_import_outcome(&mut events, &import_id)
        .await
        .unwrap_or_else(|error| panic!("the import after the cancel failed: {error}"));
}

async fn album_count(handle: &ImportServiceHandle) -> usize {
    bae_db::test_fixtures::row_count(&handle.database, "albums").await
}

#[tokio::test(flavor = "multi_thread")]
async fn a_running_import_cancelled_writes_nothing() {
    let (handle, _tmp, key, _) = two_importable().await;
    handle.import_cancels.hold_runs();
    let mut events = handle.every_event();
    let import_id = handle.start_import(&key).await.unwrap().expect_queued();

    handle.cancel_import(&key).await.unwrap();
    await_cancelled(&mut events, &[&import_id]).await;
    handle.import_cancels.release_runs();

    assert_eq!(album_count(&handle).await, 0, "no release was written");
    assert_left_as_it_stood(&handle, &key).await;
    assert_imports_again(&handle, &key).await;
    shut_down(handle).await;
}

/// The waiting import ends the moment it is cancelled, not when the worker
/// reaches it, and the worker then skips it.
#[tokio::test(flavor = "multi_thread")]
async fn a_waiting_import_cancelled_ends_at_once_and_never_runs() {
    let (handle, _tmp, running, waiting) = two_importable().await;
    handle.import_cancels.hold_runs();
    let mut events = handle.every_event();
    let running_id = handle.start_import(&running).await.unwrap().expect_queued();
    let waiting_id = handle.start_import(&waiting).await.unwrap().expect_queued();

    handle.cancel_import(&waiting).await.unwrap();
    await_cancelled(&mut events, &[&waiting_id]).await;
    assert_left_as_it_stood(&handle, &waiting).await;
    handle.cancel_import(&running).await.unwrap();
    await_cancelled(&mut events, &[&running_id]).await;
    handle.import_cancels.release_runs();

    assert_eq!(album_count(&handle).await, 0, "neither import wrote a release");
    assert_imports_again(&handle, &waiting).await;
    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn cancelling_every_import_ends_the_running_and_the_waiting() {
    let (handle, _tmp, running, waiting) = two_importable().await;
    handle.import_cancels.hold_runs();
    let mut events = handle.every_event();
    let running_id = handle.start_import(&running).await.unwrap().expect_queued();
    let waiting_id = handle.start_import(&waiting).await.unwrap().expect_queued();

    handle.cancel_all_imports().await.unwrap();
    await_cancelled(&mut events, &[&running_id, &waiting_id]).await;
    handle.import_cancels.release_runs();

    assert_eq!(album_count(&handle).await, 0, "no release was written");
    for key in [&running, &waiting] {
        assert_left_as_it_stood(&handle, key).await;
    }
    assert_imports_again(&handle, &running).await;
    shut_down(handle).await;
}

/// Cancelling what is not importing changes nothing and is not an error.
#[tokio::test(flavor = "multi_thread")]
async fn nothing_importing_is_nothing_to_cancel() {
    let (handle, _tmp, key, _) = two_importable().await;
    handle.cancel_import(&key).await.unwrap();
    handle.cancel_all_imports().await.unwrap();
    assert!(handle.candidate_runtime(&key).await.unwrap().is_none());
    shut_down(handle).await;
}

/// Everything an import could leave behind: every row the library holds, and
/// every file under its directory apart from the database's own. The counts
/// of how often each list's rows moved (`list_order_versions`) are left out:
/// an import moves its release through the import list's states and back,
/// and those moves count however the import ends.
#[derive(Debug, PartialEq)]
struct StoreState {
    rows: std::collections::BTreeMap<String, Vec<String>>,
    files: Vec<PathBuf>,
}

async fn store_state(handle: &ImportServiceHandle, library_dir: &Path) -> StoreState {
    let mut files = Vec::new();
    let mut dirs = vec![library_dir.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                dirs.push(path);
            } else if !path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("test.db"))
            {
                files.push(path);
            }
        }
    }
    files.sort();
    let mut rows = handle.database.every_row_for_test().await.unwrap();
    rows.remove("list_order_versions").expect("the store holds the lists' move counts");
    StoreState { rows, files }
}

/// The next `phase` percent `import_id` reports on the stream.
async fn await_progress_of(
    events: &mut crate::ImportEvents,
    import_id: &str,
    phase: crate::ImportPhase,
) {
    loop {
        let event = tokio::time::timeout(HANG_GUARD, events.recv())
            .await
            .expect("the progress arrives")
            .expect("the import event stream remains open");
        if let ImportEvent::ImportProgress {
            progress:
                crate::ImportProgress::Progress { import_id: reported, phase: reported_phase, .. },
            ..
        } = event
        {
            if reported == import_id && reported_phase == phase {
                return;
            }
        }
    }
}

/// A measuring thread held by [`ImportEventBus::hold_progress_at`], let go on
/// [`Self::release`] or on drop, so a test that fails first does not leave it
/// waiting.
struct HeldProgress<'a>(&'a ImportEventBus);

impl<'a> HeldProgress<'a> {
    fn at(bus: &'a ImportEventBus, phase: crate::ImportPhase) -> Self {
        bus.hold_progress_at(phase);
        Self(bus)
    }

    fn release(self) {}
}

impl Drop for HeldProgress<'_> {
    fn drop(&mut self) {
        self.0.release_send();
    }
}

/// Which step an import is at, in the order an import takes them.
fn import_position(import: &crate::ImportInFlight) -> u8 {
    use crate::{ImportPhase, ImportStep, PrepareStep};
    match import.step {
        ImportStep::Preparing(PrepareStep::Queued) => 0,
        ImportStep::Preparing(PrepareStep::ValidatingSourceFiles) => 1,
        ImportStep::Preparing(PrepareStep::FetchingArtistPictures) => 2,
        ImportStep::Running(ImportPhase::ReadingFiles) => 3,
        ImportStep::Running(ImportPhase::MeasuringLoudness) => 4,
        ImportStep::Running(ImportPhase::Finalizing) => 5,
    }
}

/// Import `key` once more and follow what the candidate shows while it does:
/// it starts from the queue, its steps only ever move forward, and
/// the release lands.
async fn assert_imports_again_from_the_start(
    handle: &ImportServiceHandle,
    key: &crate::CandidateKey,
) {
    let mut events = handle.every_event();
    let mut changes = handle.runtime.every_change();
    let import_id =
        handle.start_import(key).await.expect("the candidate imports again").expect_queued();
    await_import_outcome(&mut events, &import_id)
        .await
        .unwrap_or_else(|error| panic!("the import after the first attempt failed: {error}"));

    let mut shown = Vec::new();
    while let Ok(change) = changes.try_recv() {
        match change {
            crate::candidate_runtime::CandidateRuntimeChange::Updated { key: changed, runtime }
                if changed == key.clone() =>
            {
                shown.extend(runtime.import)
            }
            _ => {}
        }
    }
    assert_eq!(
        shown.first(),
        Some(&crate::ImportInFlight {
            step: crate::ImportStep::Preparing(crate::PrepareStep::Queued),
        }),
        "the import starts from the queue, not where the last attempt stopped"
    );
    let positions: Vec<u8> = shown.iter().map(import_position).collect();
    assert!(
        positions.windows(2).all(|pair| pair[0] <= pair[1]),
        "the candidate's progress only moves forward: {shown:?}"
    );
}

/// An import cancelled part way through its work — every file read and
/// hashed, its tracks being measured — leaves the store exactly as it was.
/// What its measuring threads report after the cancel is not the
/// candidate's, and the next import starts from the beginning.
#[tokio::test(flavor = "multi_thread")]
async fn cancelled_mid_run_leaves_the_store_as_it_was_and_starts_over() {
    let (handle, tmp, key, _) = two_importable().await;
    let library_dir = tmp[0].path().to_path_buf();
    let before = store_state(&handle, &library_dir).await;
    let held = HeldProgress::at(&handle.event_tx, crate::ImportPhase::MeasuringLoudness);
    let mut events = handle.every_event();
    let import_id = handle.start_import(&key).await.unwrap().expect_queued();
    tokio::time::timeout(HANG_GUARD, handle.event_tx.send_held())
        .await
        .expect("the import reaches its measurement");

    handle.cancel_import(&key).await.unwrap();
    await_cancelled(&mut events, &[&import_id]).await;
    // The measuring thread says where it had got after the import ended.
    held.release();
    await_progress_of(&mut events, &import_id, crate::ImportPhase::MeasuringLoudness).await;

    assert_left_as_it_stood(&handle, &key).await;
    assert_eq!(
        store_state(&handle, &library_dir).await,
        before,
        "the cancelled import left nothing behind"
    );
    assert_imports_again_from_the_start(&handle, &key).await;
    shut_down(handle).await;
}

/// A failed import leaves its failure and nothing else, and the next import
/// starts from the beginning.
#[tokio::test(flavor = "multi_thread")]
async fn a_failed_import_leaves_only_its_failure_and_starts_over() {
    let (handle, tmp, key, _) = two_importable().await;
    let library_dir = tmp[0].path().to_path_buf();
    let mut before = store_state(&handle, &library_dir).await;
    let blocked =
        crate::tests::files::UnopenableFile::block(&key.test_folder().join("02 Track.flac"));
    let mut events = handle.every_event();
    let import_id = handle.start_import(&key).await.unwrap().expect_queued();
    await_import_outcome(&mut events, &import_id)
        .await
        .expect_err("a source that will not open fails the import");
    drop(blocked);

    let mut after = store_state(&handle, &library_dir).await;
    assert_eq!(before.rows.remove("import_candidate_failure"), Some(Vec::new()));
    assert_eq!(
        after.rows.remove("import_candidate_failure").map(|rows| rows.len()),
        Some(1),
        "the failure is recorded"
    );
    // The list places the release by its failure — Found's import error,
    // under Needs You, with its last import failed — and no other row moves.
    let placement = |state: &mut StoreState| state.rows.remove("import_list_placement").unwrap();
    let failed_row =
        format!("| candidate | {:?} |", rusqlite::types::ValueRef::Blob(key.as_bytes()));
    assert_eq!(
        placement(&mut after),
        placement(&mut before)
            .into_iter()
            .map(|row| match row.contains(&failed_row) {
                true => row
                    .replace("| not_looked_up | not_looked_up |", "| import_error | needs_you |")
                    .replace(
                        "| Integer(1) | Integer(0) | Integer(1) | Null | Integer(0)",
                        "| Integer(1) | Integer(1) | Integer(1) | Null | Integer(0)",
                    ),
                false => row,
            })
            .collect::<Vec<_>>(),
        "the failed release is Found's import error"
    );
    // Found's filter entries count it there with it.
    let entries = |state: &mut StoreState| state.rows.remove("import_list_entry_count").unwrap();
    assert_eq!(entries(&mut before), ["not_looked_up | Integer(2)"]);
    assert_eq!(entries(&mut after), ["needs_you | Integer(1)", "not_looked_up | Integer(1)"]);
    assert_eq!(after, before, "the failed import left nothing else behind");
    assert!(handle.candidate_runtime(&key).await.unwrap().is_none(), "nothing is running");

    assert_imports_again_from_the_start(&handle, &key).await;
    let pane = handle.load_candidate(&key).await.unwrap().unwrap();
    assert!(
        matches!(pane.import_status, Some(crate::CandidateImportStatus::Complete { .. })),
        "the import that landed answers the one that failed"
    );
    shut_down(handle).await;
}

/// A retry of a failed import that is cancelled leaves the failure it was
/// retrying: the candidate is as it stood before the retry was asked for.
#[tokio::test(flavor = "multi_thread")]
async fn a_cancelled_retry_leaves_the_failure_it_retried() {
    let (handle, tmp, key, _) = two_importable().await;
    let library_dir = tmp[0].path().to_path_buf();
    let blocked =
        crate::tests::files::UnopenableFile::block(&key.test_folder().join("02 Track.flac"));
    let mut events = handle.every_event();
    let failed = handle.start_import(&key).await.unwrap().expect_queued();
    let error = await_import_outcome(&mut events, &failed)
        .await
        .expect_err("a source that will not open fails the import");
    drop(blocked);
    let before = store_state(&handle, &library_dir).await;

    handle.import_cancels.hold_runs();
    let retry = handle.start_import(&key).await.unwrap().expect_queued();
    handle.cancel_import(&key).await.unwrap();
    await_cancelled(&mut events, &[&retry]).await;
    handle.import_cancels.release_runs();

    assert_eq!(
        store_state(&handle, &library_dir).await,
        before,
        "the cancelled retry left the store as it was"
    );
    let pane = handle.load_candidate(&key).await.unwrap().unwrap();
    assert_eq!(
        pane.draft().unwrap().failure.clone().map(|failure| failure.reason),
        Some(crate::ImportFailureReason::error(error)),
        "the failure the retry was answering still stands"
    );
    shut_down(handle).await;
}

/// A waiting import cancelled and asked for again before the worker reaches
/// it: the command the cancel left in the queue is skipped, and the import
/// asked for since is the one that runs and lands.
#[tokio::test(flavor = "multi_thread")]
async fn an_import_asked_for_again_after_a_waiting_cancel_is_the_one_that_runs() {
    let (handle, _tmp, running, waiting) = two_importable().await;
    handle.import_cancels.hold_runs();
    let mut events = handle.every_event();
    let running_id = handle.start_import(&running).await.unwrap().expect_queued();
    let cancelled_id = handle.start_import(&waiting).await.unwrap().expect_queued();
    handle.cancel_import(&waiting).await.unwrap();
    await_cancelled(&mut events, &[&cancelled_id]).await;

    let again_id = handle
        .start_import(&waiting)
        .await
        .expect("the cancelled candidate is asked for again")
        .expect_queued();
    handle.import_cancels.release_runs();

    let mut outcomes = std::collections::HashMap::new();
    while !(outcomes.contains_key(&running_id) && outcomes.contains_key(&again_id)) {
        let event = tokio::time::timeout(HANG_GUARD, events.recv())
            .await
            .expect("both imports end")
            .expect("the import event stream remains open");
        if let ImportEvent::ImportProgress { progress, .. } = event {
            match progress {
                crate::ImportProgress::Complete { .. }
                | crate::ImportProgress::Failed { .. }
                | crate::ImportProgress::Cancelled { .. } => {
                    let import_id = progress.import_id().to_string();
                    assert_ne!(import_id, cancelled_id, "the cancelled command ran");
                    outcomes.insert(import_id, progress_kind(&progress));
                }
                crate::ImportProgress::Preparing { .. }
                | crate::ImportProgress::Progress { .. }
                | crate::ImportProgress::RemoteUploadQueued { .. } => {}
            }
        }
    }
    assert_eq!(outcomes[&running_id], "complete");
    assert_eq!(outcomes[&again_id], "complete");
    assert_eq!(album_count(&handle).await, 2);
    assert!(handle.candidate_runtime(&waiting).await.unwrap().is_none(), "nothing is left running");
    shut_down(handle).await;
}

fn progress_kind(progress: &crate::ImportProgress) -> &'static str {
    match progress {
        crate::ImportProgress::Complete { .. } => "complete",
        crate::ImportProgress::Failed { .. } => "failed",
        crate::ImportProgress::Cancelled { .. } => "cancelled",
        crate::ImportProgress::Preparing { .. }
        | crate::ImportProgress::Progress { .. }
        | crate::ImportProgress::RemoteUploadQueued { .. } => "running",
    }
}
