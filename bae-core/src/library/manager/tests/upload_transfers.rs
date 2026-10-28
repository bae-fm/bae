// How far and how fast the files being sent go, as the transfer watches
// tell them from the live query of the files being sent and coven's upload
// watch.
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
                release_id,
                transfer,
            } => {
                held.as_mut()
                    .expect("a reset comes first")
                    .releases
                    .insert(release_id, transfer);
            }
            OutboxTransferChange::Removed { release_id } => {
                held.as_mut()
                    .expect("a reset comes first")
                    .releases
                    .remove(&release_id);
            }
            OutboxTransferChange::TotalUpdated(total) => {
                held.as_mut().expect("a reset comes first").total = total;
            }
        }
    }
}

/// Only the releases with files being sent have a transfer: files that only
/// wait have none. While a file is sent the total has a bar, and
/// once every file is sent the file and its release leave and the bar goes.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn the_transfers_name_only_the_files_being_sent() {
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
            releases: Default::default(),
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
    assert!(
        sending.releases.contains_key(&release.id),
        "the release of the file being sent has its transfer: {sending:?}"
    );

    drain.await.unwrap().unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let current = held.as_ref().unwrap();
            if current.releases.is_empty() && current.total.bar.is_none() {
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
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        let mut told = sending;
        while told != idle {
            let next = total.next().await.unwrap().unwrap();
            assert_ne!(next, told, "the total is told only as it changes");
            told = next;
        }
    })
    .await
    .expect("the file sent leaves the total idle");
}

/// A release being sent reads its bytes against its whole size from the
/// moment its send starts: its stored files count as done, and a file sent in
/// one request counts its share as its object goes, before the provider
/// answers. Its file's own share is told by file.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn a_file_sent_in_one_request_moves_its_release_as_it_goes() {
    let (manager, temp_dir) = setup_test_manager().await;
    let home = connect_test_cloud(&manager).await;
    let release = insert_release_with_queued_uploads(
        &manager,
        &temp_dir.path().join("sending"),
        "Album Title",
        &[("a.flac", b"bytes-a")],
    )
    .await;
    let mut transfers = manager.watch_outbox_transfers();
    let mut files = manager.watch_upload_file_transfers();
    let mut held = None;
    apply_transfer_changes(&mut held, transfers.next().await.unwrap().unwrap());

    // The memory provider takes a request's bytes as it starts, then holds
    // the answer.
    home.set_faults(coven::Faults {
        delay: std::time::Duration::from_secs(2),
        ..coven::Faults::none()
    })
    .await;
    let drain = tokio::spawn({
        let manager = manager.library.clone();
        async move { manager.drain_uploads_for_test().await }
    });
    let bar = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            apply_transfer_changes(&mut held, transfers.next().await.unwrap().unwrap());
            let bar = held
                .as_ref()
                .unwrap()
                .releases
                .get(&release.id)
                .map(|transfer| transfer.bar);
            if bar.is_some_and(|bar| bar.bytes_done > 0) {
                break bar.unwrap();
            }
        }
    })
    .await
    .expect("the release moves while its one request goes");
    assert_eq!(
        bar,
        crate::library::UploadBar {
            bytes_done: 7,
            bytes_total: 7,
        },
        "the file's whole share counts once its request has taken it"
    );
    let fraction = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        let mut held = std::collections::BTreeMap::new();
        loop {
            for change in files.next().await.unwrap() {
                match change {
                    crate::library::FileTransferChange::Reset(all) => held = all,
                    crate::library::FileTransferChange::Updated { file, fraction } => {
                        held.insert(file, fraction);
                    }
                    crate::library::FileTransferChange::Removed { file } => {
                        held.remove(&file);
                    }
                }
            }
            if let Some(fraction) = held.values().next() {
                break *fraction;
            }
        }
    })
    .await
    .expect("the file being sent is told by file");
    assert_eq!(fraction, 1.0);
    drain.await.unwrap().unwrap();
}
