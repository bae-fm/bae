//! Storage setups as coven lists them: running, undone until the person
//! dismisses it, and acknowledged once a setup call returns or, finished or
//! cancelled, once the library opens.

use super::*;
use coven::Storage as _;
use std::sync::Arc;

/// An undone setup's line, whatever its detail.
fn undone() -> SetupRow {
    SetupRow::Undone(UiError::diagnostic(UiErrorCategory::StorageSetupUndone, "offline"))
}

#[test]
fn a_setup_with_no_outcome_runs_and_names_where_setup_stands() {
    use coven::OperationId as Id;
    assert_eq!(SetupRow::of(None).unwrap(), SetupRow::Running);
    let rows = vec![(Id(1), undone()), (Id(2), SetupRow::Running), (Id(3), undone())];
    assert_eq!(standing(rows), Some(StorageSetupState::Running));
}

#[test]
fn the_newest_undone_setup_shows_and_settled_ones_show_nothing() {
    use coven::OperationId as Id;
    let rows = vec![(Id(1), undone()), (Id(2), undone()), (Id(3), SetupRow::Settled)];
    assert!(matches!(standing(rows), Some(StorageSetupState::Undone { operation: Id(2), .. })));
    assert_eq!(standing(vec![(Id(1), SetupRow::Settled)]), None);
    assert_eq!(standing(Vec::new()), None);
}

#[test]
fn how_a_setup_ended_reads_as_settled_or_undone() {
    use coven::OperationOutcome as O;
    let category = |outcome| match SetupRow::of(Some(outcome)).unwrap() {
        SetupRow::Undone(UiError::Diagnostic { category, .. }) => Some(category),
        SetupRow::Running | SetupRow::Settled => None,
    };
    assert_eq!(SetupRow::of(Some(O::Satisfied)).unwrap(), SetupRow::Settled);
    assert_eq!(SetupRow::of(Some(O::Cancelled)).unwrap(), SetupRow::Settled);
    // A refused sharing is no way for a storage setup to end.
    let refused = O::SharingRefused {
        account: "ana@example.com".into(),
        refusal: coven::SharingRefusal::InvalidAddress,
    };
    assert!(SetupRow::of(Some(refused)).is_err());
    assert_eq!(
        category(O::Undone(coven::UndoCause::Interrupted)),
        Some(UiErrorCategory::StorageSetupUndone)
    );
    assert_eq!(
        category(O::Rejected(coven::OperationRejection::StoreOwner)),
        Some(UiErrorCategory::StoreOwnerAccount)
    );
}

/// A library on its own installation, unconnected, whose device reaches
/// its S3 storage through `device`.
async fn library(
    app_dir: &tempfile::TempDir,
    device: Arc<coven::MemoryStorage>,
) -> (Store, SyncStatus) {
    let installation = coven::Coven::in_memory(app_dir.path().to_path_buf());
    let handle = bae_store::configure_store(
        bae_store::test_devices::test_builder(&installation),
        crate::test_fixtures::TEST_SCHEMA,
    )
    .storage_connector(device)
    .create_store("Household", "Test Device", &[], |_, _| Ok(()))
    .await
    .unwrap();
    let store = bae_store::test_devices::store_under(handle, Arc::new(coven::SystemClock));
    let sync_status = crate::test_fixtures::test_sync_status(&store);
    (store, sync_status)
}

/// Set the library's storage up on `device`.
async fn set_up(
    store: &Store,
    device: &coven::MemoryStorage,
) -> Result<coven::StorageSetup, coven::StorageSetupError> {
    store
        .setup_s3_storage(
            device.config(),
            "owner-key".into(),
            coven::SecretText::new("owner-secret".into()),
            &|_| (),
        )
        .await
}

async fn state(store: &Store) -> Option<StorageSetupState> {
    store.read(|sql| storage_setup_on(&sql)).await.unwrap()
}

/// The storage setups `coven_operations` lists, oldest first.
async fn listed(store: &Store) -> Vec<coven::OperationId> {
    store
        .read(|sql| {
            Ok(sql.query(
                "SELECT operation FROM coven_operations \
                 WHERE kind = 'StorageSetup' ORDER BY operation",
                [],
                |row| Ok(coven::OperationId(row.get(0)?)),
            )?)
        })
        .await
        .unwrap()
}

/// A setup that fails before its point of no return is undone, and shows
/// why until its call's return, which heard how it ended, acknowledges it.
#[tokio::test]
async fn an_undone_setup_shows_why_until_its_call_acknowledges_it() {
    let app_dir = tempfile::TempDir::new().unwrap();
    let storage = coven::MemoryStorage::builder().build().unwrap();
    let device = Arc::new(storage.for_device());
    let (store, sync_status) = library(&app_dir, device.clone()).await;
    device.set_online(false);

    assert!(set_up(&store, &device).await.is_err());
    let Some(StorageSetupState::Undone { why, .. }) = state(&store).await else {
        panic!("the setup is undone");
    };
    assert!(matches!(
        why,
        UiError::Diagnostic { category: UiErrorCategory::StorageSetupUndone, .. }
    ));
    assert!(!sync_status.storage_setup_running().await.unwrap());

    sync_status.acknowledge_storage_setups().await.unwrap();
    assert_eq!(state(&store).await, None);
    assert_eq!(listed(&store).await, Vec::new());
}

/// The person dismisses an undone setup, and only an undone one: a finished
/// setup is refused, and stays until the library opens.
#[tokio::test]
async fn only_an_undone_setup_is_dismissed() {
    let app_dir = tempfile::TempDir::new().unwrap();
    let storage = coven::MemoryStorage::builder().build().unwrap();
    let device = Arc::new(storage.for_device());
    let (store, sync_status) = library(&app_dir, device.clone()).await;
    device.set_online(false);
    assert!(set_up(&store, &device).await.is_err());
    device.set_online(true);
    set_up(&store, &device).await.unwrap();
    let [undone, finished] = listed(&store).await[..] else {
        panic!("two setups are listed");
    };
    assert!(matches!(
        state(&store).await,
        Some(StorageSetupState::Undone { operation, .. }) if operation == undone
    ));

    let refused = sync_status.acknowledge_undone_storage_setup(finished).await;
    assert!(
        matches!(refused, Err(SetupDismissError::NotUndone(operation)) if operation == finished),
        "{refused:?}"
    );
    sync_status.acknowledge_undone_storage_setup(undone).await.unwrap();
    assert_eq!(state(&store).await, None);
    assert_eq!(listed(&store).await, vec![finished]);
    sync_status.acknowledge_settled_storage_setups().await.unwrap();
    assert_eq!(listed(&store).await, Vec::new());
}

/// Opening the library acknowledges the setups that finished and leaves an
/// undone one for the person.
#[tokio::test]
async fn opening_acknowledges_finished_setups_and_keeps_an_undone_one() {
    let app_dir = tempfile::TempDir::new().unwrap();
    let storage = coven::MemoryStorage::builder().build().unwrap();
    let device = Arc::new(storage.for_device());
    let (store, sync_status) = library(&app_dir, device.clone()).await;
    set_up(&store, &device).await.unwrap();
    device.set_online(false);
    assert!(set_up(&store, &device).await.is_err());
    let [_, undone] = listed(&store).await[..] else {
        panic!("two setups are listed");
    };

    sync_status.acknowledge_settled_storage_setups().await.unwrap();
    assert_eq!(listed(&store).await, vec![undone]);
}
