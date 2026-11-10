use super::*;

/// Change this device's settings in its closed copy of `store`: more
/// uploads at once, album loudness and a renamed save preset, none of them
/// a new library's.
async fn change_settings(builder: coven::CovenBuilder, store: coven::StoreId) {
    let handle = configure_store(builder, crate::SCHEMA).open(store, |_| ()).await.unwrap();
    let database = bae_store::test_devices::store_under(
        handle.clone(),
        std::sync::Arc::new(coven::SystemClock),
    );
    bae_preferences::PreferencesRow::new(database.clone())
        .edit(|preferences| {
            preferences.max_concurrent_uploads = std::num::NonZeroU32::new(5).unwrap();
            preferences.replay_gain_mode = bae_preferences::ReplayGainMode::Album;
            preferences.save_presets[0].name = "Archive".into();
            Ok(())
        })
        .await
        .unwrap();
    handle.close().await.unwrap();
}

/// The settings `handle`'s copy holds.
async fn settings(handle: &coven::CovenHandle) -> bae_preferences::Preferences {
    let database = bae_store::test_devices::store_under(
        handle.clone(),
        std::sync::Arc::new(coven::SystemClock),
    );
    bae_preferences::preferences(&database).await.unwrap()
}

/// Restoring over this device's copy keeps this device's settings: the copy
/// put in its place holds the replaced copy's preferences and save presets.
/// It reads the library's history under the transfer limits a library starts
/// at, since the settings go into the new copy only once it has loaded.
#[tokio::test]
async fn a_restore_over_this_devices_copy_keeps_its_settings() {
    let temp = tempfile::TempDir::new().unwrap();
    let app_dir = AppDir::at(temp.path()).unwrap();
    let installation = coven::Coven::in_memory(app_dir.coven_app_dir());
    let storage = std::sync::Arc::new(coven::MemoryStorage::builder().build().unwrap());
    let replaced = keys_replaced_elsewhere(&installation, &storage).await;
    let builder = || {
        bae_store::test_devices::test_builder(&installation)
            .storage_connector(std::sync::Arc::new(storage.for_device()))
    };
    change_settings(builder(), replaced.store).await;
    let before = {
        let handle =
            configure_store(builder(), crate::SCHEMA).open(replaced.store, |_| ()).await.unwrap();
        let before = settings(&handle).await;
        handle.close().await.unwrap();
        before
    };
    assert_ne!(before, bae_preferences::Preferences::default());
    let (_sender, cancel) = watch::channel(false);

    let restored = restore_replacing_device(
        builder,
        replaced.store,
        &replaced.current_code,
        Some(owner_key()),
        "Laptop",
        |_| {},
        |_| {},
        &cancel,
        std::sync::Arc::new(coven::SystemClock),
        &bae_diagnostics::Diagnostics::noop(),
    )
    .await
    .unwrap();
    assert_ne!(restored.device_id(), replaced.device);
    assert_eq!(restored.transfer_limits(), bae_app_dir::default_transfer_limits());
    assert_eq!(settings(&restored).await, before);
    restored.close().await.unwrap();
}

/// Resetting this device's copy starts it at a new library's settings: a
/// reset is the clean slate, and a bad setting may be part of what it
/// clears.
#[tokio::test]
async fn a_reset_starts_at_a_new_librarys_settings() {
    let temp = tempfile::TempDir::new().unwrap();
    let app_dir = AppDir::at(temp.path()).unwrap();
    let installation = coven::Coven::in_memory(app_dir.coven_app_dir());
    let storage = std::sync::Arc::new(coven::MemoryStorage::builder().build().unwrap());
    let (store, _, _) = connected_library(&installation, &storage, "Household").await;
    let builder = || {
        bae_store::test_devices::test_builder(&installation)
            .storage_connector(std::sync::Arc::new(storage.for_device()))
    };
    change_settings(builder(), store).await;

    let reset = reset_device(
        builder(),
        store,
        "Replacement",
        |_| {},
        &bae_diagnostics::Diagnostics::noop(),
    )
    .await
    .unwrap();
    assert_eq!(settings(&reset).await, bae_preferences::Preferences::default());
    reset.close().await.unwrap();
}
