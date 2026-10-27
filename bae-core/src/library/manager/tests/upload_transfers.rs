// The upload rates and the bytes of the files being sent, as the transfer
// watches tell them from the live query of the files being sent.
//
// `include!`d into `manager::tests`.

/// What a reader of the transfer watch holds after applying `changes`.
#[cfg(feature = "test-utils")]
fn apply_transfer_changes(
    held: &mut Option<crate::library::OutboxTransfers>,
    changes: Vec<crate::library::OutboxTransferChange>,
) {
    use crate::library::OutboxTransferChange;
    for change in changes {
        match change {
            OutboxTransferChange::Reset(transfers) => *held = Some(transfers),
            OutboxTransferChange::Updated {
                key,
                throughput_bps,
            } => {
                held.as_mut()
                    .expect("a reset comes first")
                    .rates
                    .insert(key, throughput_bps);
            }
            OutboxTransferChange::Removed { key } => {
                held.as_mut()
                    .expect("a reset comes first")
                    .rates
                    .remove(&key);
            }
            OutboxTransferChange::TotalUpdated(total) => {
                held.as_mut().expect("a reset comes first").total = total;
            }
        }
    }
}

/// Only the files being sent have a transfer, beside their release's: files
/// that only wait have none. While a file is sent the total has a bar, and
/// once every file is sent the file and its release leave and the bar goes.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn the_transfers_name_only_the_files_being_sent() {
    use crate::library::UploadTransferKey;
    let (manager, temp_dir) = setup_test_manager().await;
    let home = connect_test_cloud(&manager).await;
    let release = insert_release_with_queued_uploads(
        &manager,
        &temp_dir.path().join("sending"),
        "Album Title",
        &[("a.flac", b"bytes-a"), ("b.flac", b"bytes-b")],
    )
    .await;
    manager.database.set_transfer_limits(coven::TransferLimits {
        uploads: std::num::NonZeroUsize::new(1).unwrap(),
        ..manager.database.transfer_limits()
    });
    let mut transfers = manager.watch_outbox_transfers();
    let mut held = None;
    apply_transfer_changes(&mut held, transfers.next().await.unwrap().unwrap());
    assert_eq!(
        held,
        Some(crate::library::OutboxTransfers {
            rates: Default::default(),
            total: crate::library::UploadTransfer {
                bar: None,
                throughput_bps: 0,
            },
        }),
        "files that only wait have no transfer"
    );

    home.set_faults(coven::Faults {
        delay: std::time::Duration::from_secs(2),
        ..coven::Faults::none()
    })
    .await;
    let drain = tokio::spawn({
        let manager = manager.library.clone();
        async move { manager.drain_uploads_for_test().await }
    });
    let sending = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            apply_transfer_changes(&mut held, transfers.next().await.unwrap().unwrap());
            let current = held.as_ref().unwrap();
            if current.total.bar.is_some() {
                break current.clone();
            }
        }
    })
    .await
    .expect("the transfers tell the file being sent");
    let files: Vec<_> = sending
        .rates
        .keys()
        .filter(|key| matches!(key, UploadTransferKey::File(_)))
        .collect();
    assert_eq!(files.len(), 1, "one file is sent at a time: {sending:?}");
    assert!(
        sending
            .rates
            .contains_key(&UploadTransferKey::Release(release.id.clone())),
        "the release of the file being sent has its rate: {sending:?}"
    );

    drain.await.unwrap().unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let current = held.as_ref().unwrap();
            if current.rates.is_empty() && current.total.bar.is_none() {
                break;
            }
            apply_transfer_changes(&mut held, transfers.next().await.unwrap().unwrap());
        }
    })
    .await
    .expect("every file sent, nothing has a transfer");
}

/// The total's watch tells the files being sent together as they stand
/// first, and then only as they change.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn the_total_transfer_is_told_first_and_then_as_it_changes() {
    let (manager, temp_dir) = setup_test_manager().await;
    let home = connect_test_cloud(&manager).await;
    insert_release_with_queued_uploads(
        &manager,
        &temp_dir.path().join("sending"),
        "Album Title",
        &[("a.flac", b"bytes-a")],
    )
    .await;
    let mut total = manager.watch_outbox_total_transfer();
    let idle = crate::library::UploadTransfer {
        bar: None,
        throughput_bps: 0,
    };
    assert_eq!(total.next().await.unwrap().unwrap(), idle);

    home.set_faults(coven::Faults {
        delay: std::time::Duration::from_secs(2),
        ..coven::Faults::none()
    })
    .await;
    let drain = tokio::spawn({
        let manager = manager.library.clone();
        async move { manager.drain_uploads_for_test().await }
    });
    let sending = tokio::time::timeout(std::time::Duration::from_secs(10), total.next())
        .await
        .expect("the file being sent moves the total")
        .unwrap()
        .unwrap();
    assert!(sending.bar.is_some(), "{sending:?}");
    drain.await.unwrap().unwrap();
    let sent = tokio::time::timeout(std::time::Duration::from_secs(10), total.next())
        .await
        .expect("the file sent moves the total")
        .unwrap()
        .unwrap();
    assert_eq!(sent, idle);
}
