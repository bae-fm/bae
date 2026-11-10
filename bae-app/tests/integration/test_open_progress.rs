//! Opening a library tells the app each step the open runs, with how much
//! of it is done, so the app draws a bar for a long open instead of a
//! spinner.

use std::sync::{Arc, Mutex};

use bae_app::{bootstrap, BootstrapError};
use bae_app_dir::AppDir;
use tempfile::TempDir;

/// A library this device holds that hasn't run bae's migration yet, as
/// after an update that adds one, opens through the app's own path and
/// reports its steps in order, ending with the migration done.
#[test]
fn opening_a_library_with_a_migration_to_run_reports_migrating_it() {
    let directory = TempDir::new().unwrap();
    let app_dir = AppDir::under_home(directory.path()).unwrap();
    let installation = coven::Coven::in_memory(app_dir.coven_app_dir());
    let store = bae_library::test_libraries::multi_thread_runtime()
        .block_on(installation.create_closed_store("Household", Arc::new(coven::UuidIds)))
        .unwrap()
        .id();
    let reported = Arc::new(Mutex::new(Vec::new()));

    let opened = bootstrap(
        app_dir.clone(),
        store.to_string(),
        200,
        false,
        bae_app::PlaybackOutput::Device(Box::new(bae_playback::audio_output::FailingAudioDevice)),
        bae_diagnostics::Diagnostics::noop(),
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::SequentialIds::new()),
        installation.builder().id_source(bae_store::test_devices::ordered_ids()),
        {
            let reported = reported.clone();
            move |progress| reported.lock().unwrap().push(progress)
        },
        |services, runtime| -> Result<(), BootstrapError> {
            runtime.block_on(services.close()).unwrap();
            Ok(())
        },
    );

    opened.unwrap();
    let reported = reported.lock().unwrap().clone();
    assert_eq!(
        reported.last(),
        Some(&coven::Progress { step: coven::OpenStep::Migrating, done: 1, total: 1 }),
        "{reported:?}"
    );
    // The store never ordered text by bae's collations, so the open rebuilds
    // their indexes first.
    let migrating =
        reported.iter().position(|progress| progress.step == coven::OpenStep::Migrating).unwrap();
    assert!(
        reported[..migrating].iter().all(|progress| progress.step == coven::OpenStep::Rebuilding)
            && reported[migrating..]
                .iter()
                .all(|progress| progress.step == coven::OpenStep::Migrating),
        "rebuilding comes before migrating: {reported:?}"
    );
}
