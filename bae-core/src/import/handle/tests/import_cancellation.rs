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
