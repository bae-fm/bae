//! The stored totals against a count and sum over every release's storage
//! row after each kind of write that adds, changes or removes one, and the
//! plan of their read.

use super::*;
use crate::sync_activity::tests::{queue_upload, run_on_library_with_storage};
use crate::test_reads::{StoredTotalsAllParams, StoredTotalsLocalParams, StoredTotalsRemoteParams};
use bae_records::test_fixtures::storage_listing::{
    insert_file, write_library, RELEASE_EARLY, RELEASE_LATE,
};
use bae_store::statement_row;

/// The count and size the list shows under All, Local and Remote, in that
/// order.
type Totals = [(u64, u64); 3];

const FILTERS: [StorageFilter; 3] =
    [StorageFilter::All, StorageFilter::Local, StorageFilter::Remote];

/// Each filter's count and size as the Storage Manager reads them with no
/// search, after checking each against a count and sum over every release's
/// storage row that the filter keeps.
async fn checked_totals(store: &Store, after: &'static str) -> Totals {
    let (read, counted) = store
        .read(|sql| {
            let mut read = Vec::new();
            for filter in FILTERS {
                read.push(crate::storage::storage_counts_on(
                    &sql,
                    &crate::storage::StorageScope::new(filter, ""),
                )?);
            }
            let all = statement_row(&sql, &StoredTotalsAllParams {})?;
            let local = statement_row(&sql, &StoredTotalsLocalParams {})?;
            let remote = statement_row(&sql, &StoredTotalsRemoteParams {})?;
            let counted = vec![
                (all.releases, all.size),
                (local.releases, local.size),
                (remote.releases, remote.size),
            ];
            Ok((read, counted))
        })
        .await
        .unwrap();
    assert_eq!(read, counted, "after {after}: the stored totals of All, Local and Remote");
    read.try_into().unwrap()
}

/// Send every file the upload queue holds, so that bae stores it and coven
/// switches its release's files to the cloud.
async fn upload_queued(store: &Store) {
    store.set_uploads_paused(false).await.unwrap();
    store.retry_uploads_now().await.unwrap();
    store.set_uploads_paused(true).await.unwrap();
}

/// The stored totals match a count and sum over the storage rows after each
/// kind of write: releases added, a file added to a local release (its size
/// changes, it stays local), a release's files queued (neither changes), its
/// upload stored (it turns remote, its size stays), a file added to a remote
/// release (it turns local and its size changes), and a remote and a local
/// release removed.
#[test]
fn the_stored_totals_match_a_count_of_the_storage_rows_after_each_write() {
    run_on_library_with_storage(async |store, originals| {
        let records = bae_records::test_fixtures::test_records(store);
        write_library(&records).await;
        assert_eq!(checked_totals(store, "releases added").await, [(2, 0), (2, 0), (0, 0)]);

        insert_file(
            store,
            originals,
            ("5b0c3f0e-2d7a-4c61-9a4e-1f2b3c4d5e61", RELEASE_EARLY, "early.flac", 3),
        )
        .await;
        insert_file(
            store,
            originals,
            ("6c1d4a1f-3e8b-4d72-8b5f-2a3c4d5e6f72", RELEASE_LATE, "late.flac", 5),
        )
        .await;
        assert_eq!(checked_totals(store, "files added").await, [(2, 8), (2, 8), (0, 0)]);

        queue_upload(store, RELEASE_EARLY).await;
        assert_eq!(checked_totals(store, "a release queued").await, [(2, 8), (2, 8), (0, 0)]);

        upload_queued(store).await;
        assert_eq!(checked_totals(store, "an upload stored").await, [(2, 8), (1, 5), (1, 3)]);

        insert_file(
            store,
            originals,
            ("7d2e5b2a-4f9c-4e83-9c6a-3b4d5e6f7a83", RELEASE_EARLY, "added.flac", 4),
        )
        .await;
        assert_eq!(
            checked_totals(store, "a file added to a remote release").await,
            [(2, 12), (2, 12), (0, 0)]
        );

        queue_upload(store, RELEASE_LATE).await;
        upload_queued(store).await;
        assert_eq!(
            checked_totals(store, "a second upload stored").await,
            [(2, 12), (1, 7), (1, 5)]
        );

        bae_records::test_fixtures::delete_release(&records, RELEASE_LATE).await;
        assert_eq!(
            checked_totals(store, "a remote release removed").await,
            [(1, 7), (1, 7), (0, 0)]
        );

        bae_records::test_fixtures::delete_release(&records, RELEASE_EARLY).await;
        assert_eq!(
            checked_totals(store, "a local release removed").await,
            [(0, 0), (0, 0), (0, 0)]
        );
    });
}

/// The count and size with no search read the stored totals alone: one row
/// by its key under Local or Remote, the table's two rows under All, and no
/// release's storage row.
#[tokio::test]
async fn the_totals_read_the_stored_totals_alone() {
    let (db, _tmp) = bae_store::test_fixtures::temp_db(crate::TEST_SCHEMA).await;
    let plan = |statement: &'static str| {
        let db = db.clone();
        async move {
            db.read(move |sql| {
                let unbound: &[(&str, &dyn rusqlite::ToSql)] = &[];
                let steps = crate::tests::plan_steps(&sql, statement, unbound)?;
                Ok(steps.into_iter().map(|step| step.detail).collect::<Vec<_>>())
            })
            .await
            .unwrap()
        }
    };
    assert_eq!(plan(AllStorageTotals::SQL).await, vec!["SCAN held".to_string()]);
    assert_eq!(
        plan(KeptStorageTotals::SQL).await,
        vec!["SEARCH held USING INTEGER PRIMARY KEY (rowid=?)".to_string()]
    );
}
