use super::*;
use std::collections::HashMap;

fn transfer(bytes_done: u64, throughput_bps: u64) -> UploadTransfer {
    UploadTransfer {
        bar: Some(crate::library::UploadBar {
            bytes_done,
            bytes_total: 100,
        }),
        throughput_bps,
    }
}

fn transfers(
    keyed: Vec<(UploadTransferKey, UploadTransfer)>,
    total: UploadTransfer,
) -> OutboxTransfers {
    OutboxTransfers {
        keyed: keyed.into_iter().collect::<HashMap<_, _>>(),
        total,
    }
}

#[test]
fn a_reader_hears_each_transfer_that_moved_and_each_that_left() {
    let first = UploadTransferKey::Release("release-1".into());
    let second = UploadTransferKey::Release("release-2".into());
    let told = transfers(
        vec![
            (first.clone(), transfer(10, 5)),
            (second.clone(), transfer(20, 5)),
        ],
        transfer(30, 10),
    );
    let next = transfers(vec![(first.clone(), transfer(40, 5))], transfer(40, 5));

    let changes = changes_between(&told, &next);

    assert_eq!(changes.len(), 3, "{changes:?}");
    assert!(changes.contains(&OutboxTransferChange::Removed { key: second }));
    assert!(changes.contains(&OutboxTransferChange::Updated {
        key: first,
        transfer: transfer(40, 5),
    }));
    assert!(changes.contains(&OutboxTransferChange::TotalUpdated(transfer(40, 5))));
}

#[test]
fn transfers_that_did_not_move_say_nothing() {
    let key = UploadTransferKey::Release("release-1".into());
    let told = transfers(vec![(key, transfer(10, 5))], transfer(10, 5));

    assert!(changes_between(&told, &told.clone()).is_empty());
}

/// The upload total's watch over a queue this test publishes itself.
mod total_transfer {
    use super::*;
    use crate::library::{OutboxPauseState, OutboxQueue, UploadProgress};
    use std::sync::Arc;
    use std::time::Duration;

    /// A queue at `revision` whose keyed transfers stand at `keyed` and whose
    /// files together moved `total`.
    fn published(
        revision: u64,
        keyed: Vec<(UploadTransferKey, UploadTransfer)>,
        total: UploadTransfer,
    ) -> Option<Result<OutboxState, String>> {
        Some(Ok(OutboxState {
            revision,
            queue: OutboxQueue {
                upload_groups: Vec::new(),
                other_files: Vec::new(),
                total: UploadProgress::default(),
                pause_state: OutboxPauseState::Running,
            },
            transfers: transfers(keyed, total),
        }))
    }

    /// A watch over `values`, with a library behind it it never reads: the
    /// test publishes before the first read.
    async fn watch(
        values: watch::Receiver<Option<Result<OutboxState, String>>>,
        temp_dir: &tempfile::TempDir,
    ) -> OutboxTotalTransferWatch {
        let database = crate::db::Database::new_test(
            temp_dir.path().join("test.db").to_str().unwrap(),
            Arc::new(coven::SystemClock),
        )
        .await
        .unwrap();
        let config = crate::config::Config::with_defaults(
            database.store_id(),
            temp_dir.path().join("preferences"),
        );
        let manager = LibraryManager::new(
            database,
            crate::config::AppDir::under_home(temp_dir.path()).unwrap(),
            Arc::new(crate::config::ConfigHandle::new(config)),
            Arc::new(coven::SystemClock),
            Arc::new(coven::UuidIds),
            crate::diagnostics::Diagnostics::noop(),
            tokio::runtime::Handle::current(),
            bae_catalogs::remote_image::RemoteImageCache::for_test(bae_util::http::Http::for_test()),
            crate::providers::Providers::offline(),
        );
        OutboxTotalTransferWatch {
            manager,
            values,
            told: None,
        }
    }

    /// Whether the watch has a value to give now, without waiting for one.
    async fn gives_now(watch: &mut OutboxTotalTransferWatch) -> Option<UploadTransfer> {
        tokio::time::timeout(Duration::from_millis(100), watch.next())
            .await
            .ok()
            .flatten()
    }

    #[tokio::test]
    async fn the_total_is_told_first_and_then_only_as_it_moves() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let release = UploadTransferKey::Release("release-1".into());
        let (values, receiver) = watch::channel(published(
            1,
            vec![(release.clone(), transfer(10, 5))],
            transfer(10, 5),
        ));
        let mut watch = watch(receiver, &temp_dir).await;

        assert_eq!(gives_now(&mut watch).await, Some(transfer(10, 5)));

        // A release's transfer moving while the total holds tells nothing.
        values.send_replace(published(
            1,
            vec![(release.clone(), transfer(20, 5))],
            transfer(10, 5),
        ));
        assert_eq!(gives_now(&mut watch).await, None);

        values.send_replace(published(
            1,
            vec![(release, transfer(30, 5))],
            transfer(30, 5),
        ));
        assert_eq!(gives_now(&mut watch).await, Some(transfer(30, 5)));
        assert_eq!(gives_now(&mut watch).await, None);
    }
}
