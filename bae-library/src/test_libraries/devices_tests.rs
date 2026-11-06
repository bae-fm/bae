use super::*;

/// A two-device test closes both devices before its runtime ends, as closing a
/// library in the bridge does before it drops the library's runtime: nothing of
/// either device is left running for the runtime's shutdown to cancel under it.
#[test]
fn a_two_device_test_closes_both_devices_before_its_runtime_ends() {
    let databases = Arc::new(std::sync::Mutex::new(Vec::new()));
    let kept = databases.clone();
    run_two_device_test(async move |devices| {
        kept.lock()
            .expect("kept databases")
            .extend([devices.a().store().clone(), devices.b().store().clone()]);
    });
    let databases = std::mem::take(&mut *databases.lock().expect("kept databases"));
    assert_eq!(databases.len(), 2);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build a runtime to read the devices from");
    for db in databases {
        let read = runtime.block_on(db.unsent_writes_for_test());
        assert!(
            matches!(
                &read,
                Err(bae_store::DbError::Coven(error))
                    if matches!(**error, coven::CovenError::Database(coven::DbError::StoreClosed))
            ),
            "the device's store is closed: {read:?}"
        );
    }
}
