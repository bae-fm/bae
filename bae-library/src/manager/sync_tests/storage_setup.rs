use super::*;

/// The settings that set S3 storage up on the manager's memory storage.
fn memory_s3_config(manager: &TestManager) -> crate::sync::S3ConfigData {
    let coven::StorageConfig::S3 { bucket, region, endpoint, profile } = manager.storage.config()
    else {
        panic!("the memory provider uses S3 configuration");
    };
    crate::sync::S3ConfigData {
        bucket,
        region,
        endpoint: endpoint.map(|url| url.to_string()),
        profile,
        access_key: "test-key".into(),
        secret_key: "test-secret".into(),
    }
}

/// A setup call that fails says why itself, so the setup it undid leaves
/// nothing for the sync status to show.
#[tokio::test]
async fn a_failed_setup_call_leaves_nothing_for_the_status() {
    let (test, _temp) = setup_test_manager().await;
    let manager = test.library.manager();
    test.storage.set_online(false);

    manager
        .save_s3_config(memory_s3_config(&test), "Test Device", &|_| ())
        .await
        .expect_err("offline storage cannot be set up");

    assert_eq!(manager.sync_status().await.unwrap().storage_setup, None);
    let listed = manager
        .store
        .read(|sql| {
            Ok(sql.query_row(
                "SELECT count(*) FROM coven_operations WHERE kind = 'StorageSetup'",
                [],
                |row| row.get::<_, i64>(0),
            )?)
        })
        .await
        .unwrap();
    assert_eq!(listed, 0);
}

/// Setting S3 up as the library's storage tells the caller the steps coven
/// reports as it runs them, in order, the last its creating the store.
#[tokio::test]
async fn setting_up_s3_storage_tells_each_step_coven_reports() {
    use coven::StorageSetupStep::*;
    let (test, _temp) = setup_test_manager().await;
    let manager = test.library.manager();
    let told = std::sync::Mutex::new(Vec::new());

    manager
        .save_s3_config(memory_s3_config(&test), "Test Device", &|progress| {
            told.lock().unwrap().push(progress)
        })
        .await
        .unwrap();

    let told = told.into_inner().unwrap();
    let order = [CheckingLocation, DetectingSettings, Connecting, CreatingStore];
    let positions: Vec<usize> = told
        .iter()
        .map(|progress| {
            order
                .iter()
                .position(|step| *step == progress.step)
                .unwrap_or_else(|| panic!("a step S3 setup doesn't run: {progress:?}"))
        })
        .collect();
    assert!(positions.windows(2).all(|pair| pair[0] <= pair[1]), "{told:?}");
    assert_eq!(
        told.last(),
        Some(&coven::Progress { step: CreatingStore, done: 1, total: 1 }),
        "{told:?}"
    );
}

/// The steps `storage_setup_finishing` names are past setup's point of no
/// return: a cancel there answers `Finishing` and the setup lands, so the
/// screen that hides Cancel at them hides it where it could no longer undo.
/// Every step before is not finishing.
#[tokio::test]
async fn a_finishing_step_is_past_the_point_of_no_return() {
    let (test, _temp) = setup_test_manager().await;
    let manager = test.library.manager();
    let told = std::sync::Mutex::new(Vec::new());

    manager
        .save_s3_config(memory_s3_config(&test), "Test Device", &|progress| {
            let finishing = crate::storage_setup_finishing(&progress.step);
            let answer = finishing.then(|| manager.store.cancel_storage_setup());
            told.lock().unwrap().push((progress.step, answer));
        })
        .await
        .expect("a setup cancelled past its point of no return finishes");

    let told = told.into_inner().unwrap();
    let first_finishing =
        told.iter().position(|(_, answer)| answer.is_some()).expect("setup reached creating");
    let (before, finishing) = told.split_at(first_finishing);
    assert!(before.iter().all(|(_, answer)| answer.is_none()), "{told:?}");
    assert!(
        finishing.iter().all(|step| step
            == &(
                coven::StorageSetupStep::CreatingStore,
                Some(coven::SetupCancellation::Finishing)
            )),
        "{told:?}"
    );
    assert!(manager.is_sync_configured().await.unwrap());
}

/// Leaving a setup's screen drops its call, which cancels the setup: coven
/// alone would run it to its end with nobody waiting. The cancelled setup
/// shows nothing, and the library stays unconfigured.
#[tokio::test]
async fn a_dropped_setup_call_cancels_its_setup() {
    let (test, _temp) = setup_test_manager().await;
    let manager = test.library.manager();
    // Every write to storage waits, so setup can't pass its point of no
    // return while the call runs.
    let writes = Arc::new(tokio::sync::Semaphore::new(0));
    test.storage.gate_creations(writes.clone()).await;
    let mut setup =
        Box::pin(manager.save_s3_config(memory_s3_config(&test), "Test Device", &|_| ()));
    let running = |status: &bae_sync_status::SyncStatusSnapshot| {
        status.storage_setup == Some(bae_sync_status::StorageSetupState::Running)
    };
    tokio::select! {
        returned = setup.as_mut() => panic!("a setup whose writes wait returned: {returned:?}"),
        _ = status_until(&test, running) => {}
    }

    drop(setup);
    writes.add_permits(usize::MAX >> 4);
    status_until(&test, |status| status.storage_setup.is_none()).await;
    let outcomes = manager
        .store
        .read(|sql| {
            Ok(sql.query(
                "SELECT outcome FROM coven_operations WHERE kind = 'StorageSetup'",
                [],
                |row| row.get::<_, Option<coven::OperationOutcome>>(0),
            )?)
        })
        .await
        .unwrap();
    assert_eq!(outcomes, vec![Some(coven::OperationOutcome::Cancelled)]);
    assert!(!manager.is_sync_configured().await.unwrap());
}
