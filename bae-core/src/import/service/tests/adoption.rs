// A folder taking over the watched folders inside it, as the coordinator runs
// it: what reads them is stopped and their watches taken down before the rows
// change hands, and the folder that takes over is read straight after.

/// Hears whether an adoption landed, or whether the read after it is over.
type AdoptionAnswer = tokio::sync::oneshot::Receiver<Result<(), String>>;

/// Ask for `parent` to take over `inner`: whether it landed, then whether the
/// read of `parent` after it is over.
fn request_adoption(
    harness: &CoordinatorHarness,
    parent: &str,
    inner: &[&str],
) -> (AdoptionAnswer, AdoptionAnswer) {
    let (adopted, adopted_result) = tokio::sync::oneshot::channel();
    let (read, read_result) = tokio::sync::oneshot::channel();
    harness
        .commands
        .send(WatcherCommand::Adopt {
            parent: root_path(parent),
            inner: inner.iter().map(|root| root_path(root)).collect(),
            adopted,
            read: Some(read),
        })
        .unwrap();
    (adopted_result, read_result)
}

/// A pass over a folder being taken over is cancelled and waited for before
/// anything changes hands; then the adopting folder is read, and whoever asked
/// hears once that read is over.
#[tokio::test]
async fn adoption_stops_the_inner_read_then_reads_the_parent() {
    let harness = CoordinatorHarness::with_roots(&["/music/one", "/music/two"]).await;
    rescan_and_wait(&harness, "/music/one").await;

    let (adopted, read) = request_adoption(&harness, "/music", &["/music/one", "/music/two"]);
    harness.scans.wait_for_cancellation(0).await;
    let mut adopted = Box::pin(adopted);
    assert!(
        tokio::time::timeout(Duration::from_millis(50), adopted.as_mut())
            .await
            .is_err(),
        "the adoption waits for the pass it cancelled"
    );

    harness.scans.complete(0);
    assert_eq!(adopted.await.unwrap(), Ok(()));
    assert_eq!(
        harness.removal_backend.calls.lock().unwrap().as_slice(),
        ["uninstall", "uninstall", "adopt"]
    );
    harness.scans.wait_for_count(2).await;
    assert_eq!(harness.scans.scans.lock().unwrap()[1].path, root_path("/music"));
    harness.scans.complete(1);
    assert_eq!(read.await.unwrap(), Ok(()));
    harness.shutdown().await;
}

/// A durable change that does not land puts the watches back and reads the
/// folders again, and both callers hear why.
#[tokio::test]
async fn a_failed_adoption_restores_the_inner_folders() {
    let harness = CoordinatorHarness::with_roots(&["/music/one", "/music/two"]).await;
    *harness.removal_backend.remove_error.lock().unwrap() = Some("boom".to_string());

    let (adopted, read) = request_adoption(&harness, "/music", &["/music/one", "/music/two"]);

    let error = adopted.await.unwrap().unwrap_err();
    assert!(error.contains("boom"), "{error}");
    assert!(read.await.unwrap().is_err());
    assert_eq!(
        harness.removal_backend.calls.lock().unwrap().as_slice(),
        ["uninstall", "uninstall", "adopt", "reinstall", "reinstall"]
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
