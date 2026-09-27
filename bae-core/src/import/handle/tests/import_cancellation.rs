//! Cancelling an import: one waiting for the worker never starts, one running
//! is dropped before it writes, and either leaves the candidate as it stood
//! before the import was asked for — no release, no recorded failure, nothing
//! running — ready to be imported again.

use super::*;

/// Two picked candidates with their artists named, on one handle: enough to
/// have one import running while another waits behind it.
async fn two_importable() -> (ImportServiceHandle, [TempDir; 2], String, String) {
    let (manager, tmp) = setup_test_manager().await;
    let other = TempDir::new().unwrap();
    let (_, first, _) = picked_candidate(&manager, &tmp, "First Album").await;
    let (_, second, _) = picked_candidate(&manager, &other, "Second Album").await;
    let handle = manager
        .start_import_service(tokio::runtime::Handle::current())
        .await
        .unwrap();
    for key in [&first, &second] {
        handle
            .select_candidate_metadata_provenance(
                key.clone(),
                crate::import::MetadataProvenance::FileMetadata,
            )
            .await
            .unwrap();
        handle
            .set_candidate_album_artists(
                key,
                vec![crate::import::ArtistAssignment::named("Artist")],
            )
            .await
            .unwrap();
    }
    (handle, [tmp, other], first, second)
}

/// Wait for every one of `import_ids` to end cancelled, in whatever order
/// they end.
async fn await_cancelled(
    events: &mut tokio::sync::broadcast::Receiver<ImportEvent>,
    import_ids: &[&str],
) {
    let mut pending: std::collections::HashSet<String> =
        import_ids.iter().map(|id| id.to_string()).collect();
    while !pending.is_empty() {
        let event = tokio::time::timeout(std::time::Duration::from_secs(10), events.recv())
            .await
            .expect("the import reports its end")
            .expect("the import event stream remains open");
        match event {
            ImportEvent::ImportProgress {
                progress: crate::import::ImportProgress::Cancelled { import_id },
                ..
            } => {
                pending.remove(&import_id);
            }
            ImportEvent::ImportProgress {
                progress:
                    crate::import::ImportProgress::Complete { import_id, .. }
                    | crate::import::ImportProgress::Failed { import_id, .. },
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
async fn assert_left_as_it_stood(handle: &ImportServiceHandle, key: &str) {
    assert!(
        handle.runtime.get(key).is_none(),
        "nothing is running for {key}"
    );
    let pane = handle
        .candidate_pane(key)
        .await
        .unwrap()
        .expect("the candidate reads back");
    assert!(pane.import_status.is_none(), "{key} records no failure");
}

/// The candidate imports once more, now that nothing holds it.
async fn assert_imports_again(handle: &ImportServiceHandle, key: &str) {
    let mut events = handle.subscribe_events();
    let import_id = handle
        .start_import(key, crate::import::StorageMode::Local, false)
        .await
        .expect("a cancelled candidate imports again");
    await_import_outcome(&mut events, &import_id)
        .await
        .unwrap_or_else(|error| panic!("the import after the cancel failed: {error}"));
}

async fn album_count(handle: &ImportServiceHandle) -> usize {
    handle.library_manager.get_albums(&[]).await.unwrap().len()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_running_import_cancelled_writes_nothing() {
    let (handle, _tmp, key, _) = two_importable().await;
    handle.import_cancels.hold_runs();
    let mut events = handle.subscribe_events();
    let import_id = handle
        .start_import(&key, crate::import::StorageMode::Local, false)
        .await
        .unwrap();

    handle.cancel_import(&key).unwrap();
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
    let mut events = handle.subscribe_events();
    let running_id = handle
        .start_import(&running, crate::import::StorageMode::Local, false)
        .await
        .unwrap();
    let waiting_id = handle
        .start_import(&waiting, crate::import::StorageMode::Local, false)
        .await
        .unwrap();

    handle.cancel_import(&waiting).unwrap();
    await_cancelled(&mut events, &[&waiting_id]).await;
    assert_left_as_it_stood(&handle, &waiting).await;
    handle.cancel_import(&running).unwrap();
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
    let mut events = handle.subscribe_events();
    let running_id = handle
        .start_import(&running, crate::import::StorageMode::Local, false)
        .await
        .unwrap();
    let waiting_id = handle
        .start_import(&waiting, crate::import::StorageMode::Local, false)
        .await
        .unwrap();

    handle.cancel_all_imports();
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
    handle.cancel_import(&key).unwrap();
    handle.cancel_all_imports();
    assert!(handle.runtime.get(&key).is_none());
    shut_down(handle).await;
}

/// Everything an import could leave behind: every row the library holds, and
/// every file under its directory apart from the database's own.
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
    StoreState {
        rows: handle.library_manager.every_row_for_test().await,
        files,
    }
}

/// The next `phase` percent `import_id` reports on the stream.
async fn await_progress_of(
    events: &mut tokio::sync::broadcast::Receiver<ImportEvent>,
    import_id: &str,
    phase: crate::import::ImportPhase,
) {
    loop {
        let event = tokio::time::timeout(std::time::Duration::from_secs(10), events.recv())
            .await
            .expect("the progress arrives")
            .expect("the import event stream remains open");
        if let ImportEvent::ImportProgress {
            progress:
                crate::import::ImportProgress::Progress {
                    import_id: reported,
                    phase: reported_phase,
                    ..
                },
            ..
        } = event
        {
            if reported == import_id && reported_phase == phase {
                return;
            }
        }
    }
}

/// How far along an import's step is, in the order an import takes them.
fn import_position(import: &crate::import::candidates::ImportInFlight) -> (u8, u32) {
    use crate::import::{ImportPhase, ImportStep, PrepareStep};
    let step = match import.step.expect("an import in flight names its step") {
        ImportStep::Preparing(PrepareStep::Queued) => 0,
        ImportStep::Preparing(PrepareStep::ValidatingSourceFiles) => 1,
        ImportStep::Running(ImportPhase::ReadingFiles) => 2,
        ImportStep::Running(ImportPhase::MeasuringLoudness) => 3,
        ImportStep::Running(ImportPhase::Finalizing) => 4,
    };
    (step, import.progress_percent.unwrap_or(0))
}

/// Import `key` once more and follow what the candidate shows while it does:
/// it starts from the queue with no progress, only ever moves forward, and
/// the release lands.
async fn assert_imports_again_from_the_start(handle: &ImportServiceHandle, key: &str) {
    let mut events = handle.subscribe_events();
    let mut changes = handle.runtime.subscribe();
    let import_id = handle
        .start_import(key, crate::import::StorageMode::Local, false)
        .await
        .expect("the candidate imports again");
    await_import_outcome(&mut events, &import_id)
        .await
        .unwrap_or_else(|error| panic!("the import after the first attempt failed: {error}"));

    let mut shown = Vec::new();
    while let Ok(change) = changes.try_recv() {
        match change {
            crate::import::candidate_runtime::CandidateRuntimeChange::Updated {
                key: changed,
                runtime,
            } if changed == key => shown.extend(runtime.import),
            _ => {}
        }
    }
    assert_eq!(
        shown.first(),
        Some(&crate::import::candidates::ImportInFlight {
            progress_percent: None,
            step: Some(crate::import::ImportStep::Preparing(
                crate::import::PrepareStep::Queued
            )),
        }),
        "the import starts from the queue, not where the last attempt stopped"
    );
    let positions: Vec<(u8, u32)> = shown.iter().map(import_position).collect();
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
    handle
        .event_tx
        .hold_progress_at(crate::import::ImportPhase::MeasuringLoudness);
    let mut events = handle.subscribe_events();
    let import_id = handle
        .start_import(&key, crate::import::StorageMode::Local, false)
        .await
        .unwrap();
    tokio::time::timeout(
        std::time::Duration::from_secs(10),
        handle.event_tx.progress_held(),
    )
    .await
    .expect("the import reaches its measurement");

    handle.cancel_import(&key).unwrap();
    await_cancelled(&mut events, &[&import_id]).await;
    // The measuring thread says where it had got after the import ended.
    handle.event_tx.release_progress();
    await_progress_of(
        &mut events,
        &import_id,
        crate::import::ImportPhase::MeasuringLoudness,
    )
    .await;

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
        crate::test_files::UnopenableFile::block(&Path::new(&key).join("02 Track.flac"));
    let mut events = handle.subscribe_events();
    let import_id = handle
        .start_import(&key, crate::import::StorageMode::Local, false)
        .await
        .unwrap();
    await_import_outcome(&mut events, &import_id)
        .await
        .expect_err("a source that will not open fails the import");
    drop(blocked);

    let mut after = store_state(&handle, &library_dir).await;
    assert_eq!(
        before.rows.remove("import_candidate_failure"),
        Some(Vec::new())
    );
    assert_eq!(
        after
            .rows
            .remove("import_candidate_failure")
            .map(|rows| rows.len()),
        Some(1),
        "the failure is recorded"
    );
    assert_eq!(after, before, "the failed import left nothing else behind");
    assert!(handle.runtime.get(&key).is_none(), "nothing is running");

    assert_imports_again_from_the_start(&handle, &key).await;
    let pane = handle.candidate_pane(&key).await.unwrap().unwrap();
    assert!(
        pane.failure.is_none(),
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
        crate::test_files::UnopenableFile::block(&Path::new(&key).join("02 Track.flac"));
    let mut events = handle.subscribe_events();
    let failed = handle
        .start_import(&key, crate::import::StorageMode::Local, false)
        .await
        .unwrap();
    let error = await_import_outcome(&mut events, &failed)
        .await
        .expect_err("a source that will not open fails the import");
    drop(blocked);
    let before = store_state(&handle, &library_dir).await;

    handle.import_cancels.hold_runs();
    let retry = handle
        .start_import(&key, crate::import::StorageMode::Local, false)
        .await
        .unwrap();
    handle.cancel_import(&key).unwrap();
    await_cancelled(&mut events, &[&retry]).await;
    handle.import_cancels.release_runs();

    assert_eq!(
        store_state(&handle, &library_dir).await,
        before,
        "the cancelled retry left the store as it was"
    );
    let pane = handle.candidate_pane(&key).await.unwrap().unwrap();
    assert_eq!(
        pane.failure.map(|failure| failure.error),
        Some(error),
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
    let mut events = handle.subscribe_events();
    let running_id = handle
        .start_import(&running, crate::import::StorageMode::Local, false)
        .await
        .unwrap();
    let cancelled_id = handle
        .start_import(&waiting, crate::import::StorageMode::Local, false)
        .await
        .unwrap();
    handle.cancel_import(&waiting).unwrap();
    await_cancelled(&mut events, &[&cancelled_id]).await;

    let again_id = handle
        .start_import(&waiting, crate::import::StorageMode::Local, false)
        .await
        .expect("the cancelled candidate is asked for again");
    handle.import_cancels.release_runs();

    let mut outcomes = std::collections::HashMap::new();
    while !(outcomes.contains_key(&running_id) && outcomes.contains_key(&again_id)) {
        let event = tokio::time::timeout(std::time::Duration::from_secs(10), events.recv())
            .await
            .expect("both imports end")
            .expect("the import event stream remains open");
        if let ImportEvent::ImportProgress { progress, .. } = event {
            match progress {
                crate::import::ImportProgress::Complete { .. }
                | crate::import::ImportProgress::Failed { .. }
                | crate::import::ImportProgress::Cancelled { .. } => {
                    let import_id = progress.import_id().to_string();
                    assert_ne!(import_id, cancelled_id, "the cancelled command ran");
                    outcomes.insert(import_id, progress_kind(&progress));
                }
                crate::import::ImportProgress::Preparing { .. }
                | crate::import::ImportProgress::Progress { .. }
                | crate::import::ImportProgress::RemoteUploadQueued { .. } => {}
            }
        }
    }
    assert_eq!(outcomes[&running_id], "complete");
    assert_eq!(outcomes[&again_id], "complete");
    assert_eq!(album_count(&handle).await, 2);
    assert!(handle.runtime.get(&waiting).is_none(), "nothing is left running");
    shut_down(handle).await;
}

fn progress_kind(progress: &crate::import::ImportProgress) -> &'static str {
    match progress {
        crate::import::ImportProgress::Complete { .. } => "complete",
        crate::import::ImportProgress::Failed { .. } => "failed",
        crate::import::ImportProgress::Cancelled { .. } => "cancelled",
        crate::import::ImportProgress::Preparing { .. }
        | crate::import::ImportProgress::Progress { .. }
        | crate::import::ImportProgress::RemoteUploadQueued { .. } => "running",
    }
}
