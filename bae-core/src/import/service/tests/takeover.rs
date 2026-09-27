// A folder taking over the watched folders inside it: a removal of them with
// the folder watched in their place.

/// Hears whether a takeover landed.
type TakeoverAnswer = tokio::sync::oneshot::Receiver<Result<(), String>>;

/// Ask for `parent` to take over `inner`, and hear whether it landed.
fn request_takeover(harness: &CoordinatorHarness, parent: &str, inner: &[&str]) -> TakeoverAnswer {
    let (completion, result) = tokio::sync::oneshot::channel();
    harness
        .commands
        .send(WatcherCommand::Remove {
            roots: inner.iter().map(|root| root_path(root)).collect(),
            parent: Some(root_path(parent)),
            completion,
        })
        .unwrap();
    result
}

/// A pass over an inner folder is cancelled and waited out; then the parent
/// is read.
#[tokio::test]
async fn a_takeover_stops_the_inner_read_then_reads_the_parent() {
    let harness = CoordinatorHarness::with_roots(&["/music/one", "/music/two"]).await;
    rescan_and_wait(&harness, "/music/one").await;

    let taken_over = request_takeover(&harness, "/music", &["/music/one", "/music/two"]);
    harness.scans.wait_for_cancellation(0).await;
    let mut taken_over = Box::pin(taken_over);
    assert!(
        tokio::time::timeout(Duration::from_millis(50), taken_over.as_mut())
            .await
            .is_err(),
        "the takeover waits for the pass it cancelled"
    );

    harness.scans.complete(0);
    assert_eq!(taken_over.await.unwrap(), Ok(()));
    assert_eq!(
        harness.removal_backend.calls.lock().unwrap().as_slice(),
        ["uninstall", "uninstall", "remove"]
    );
    harness.scans.wait_for_count(2).await;
    assert_eq!(harness.scans.scans.lock().unwrap()[1].path, root_path("/music"));
    harness.scans.complete(1);
    harness.shutdown().await;
}

/// A refresh of an inner folder is answered by the parent's read.
#[tokio::test]
async fn a_refresh_during_a_takeover_waits_for_the_parent_read() {
    let harness = CoordinatorHarness::with_roots(&["/music/one", "/music/two"]).await;
    rescan_and_wait(&harness, "/music/one").await;
    let taken_over = request_takeover(&harness, "/music", &["/music/one", "/music/two"]);
    harness.scans.wait_for_cancellation(0).await;
    let mut refreshed = Box::pin(request_refresh(&harness, "/music/two"));
    assert!(
        tokio::time::timeout(Duration::from_millis(50), refreshed.as_mut())
            .await
            .is_err(),
        "the refresh waits for the takeover"
    );

    harness.scans.complete(0);
    assert_eq!(taken_over.await.unwrap(), Ok(()));
    harness.scans.wait_for_count(2).await;
    assert_eq!(harness.scans.scans.lock().unwrap()[1].path, root_path("/music"));
    assert!(
        tokio::time::timeout(Duration::from_millis(50), refreshed.as_mut())
            .await
            .is_err(),
        "the refresh waits for the parent's read"
    );
    harness.scans.complete(1);
    assert_eq!(refreshed.await.unwrap(), Ok(()));
    harness.shutdown().await;
}

/// Removing a folder that is being taken over is refused rather than raced
/// against the takeover.
#[tokio::test]
async fn removing_a_folder_being_taken_over_is_refused() {
    let harness = CoordinatorHarness::with_roots(&["/music/one", "/music/two"]).await;
    rescan_and_wait(&harness, "/music/one").await;
    let taken_over = request_takeover(&harness, "/music", &["/music/one", "/music/two"]);
    harness.scans.wait_for_cancellation(0).await;

    let (completion, removed) = tokio::sync::oneshot::channel();
    harness
        .commands
        .send(WatcherCommand::Remove {
            roots: vec![root_path("/music/two")],
            parent: None,
            completion,
        })
        .unwrap();
    assert_eq!(
        removed.await.unwrap(),
        Err(format!(
            "{} is being taken over by {}",
            root_path("/music/two").display(),
            root_path("/music").display()
        ))
    );

    harness.scans.complete(0);
    assert_eq!(taken_over.await.unwrap(), Ok(()));
    harness.scans.wait_for_count(2).await;
    harness.scans.complete(1);
    harness.shutdown().await;
}

/// A store change that does not land puts the watches back and reads the
/// inner folders again.
#[tokio::test]
async fn a_failed_takeover_restores_the_inner_folders() {
    let harness = CoordinatorHarness::with_roots(&["/music/one", "/music/two"]).await;
    *harness.removal_backend.remove_error.lock().unwrap() = Some("boom".to_string());

    let taken_over = request_takeover(&harness, "/music", &["/music/one", "/music/two"]);

    let error = taken_over.await.unwrap().unwrap_err();
    assert!(error.contains("boom"), "{error}");
    assert_eq!(
        harness.removal_backend.calls.lock().unwrap().as_slice(),
        ["uninstall", "uninstall", "remove", "reinstall", "reinstall"]
    );
    harness.scans.wait_for_count(2).await;
    let mut read_again: Vec<PathBuf> = harness
        .scans
        .scans
        .lock()
        .unwrap()
        .iter()
        .map(|scan| scan.path.clone())
        .collect();
    read_again.sort();
    assert_eq!(read_again, vec![root_path("/music/one"), root_path("/music/two")]);
    for index in 0..2 {
        harness.scans.complete(index);
    }
    harness.shutdown().await;
}
