//! What creating a library costs locally — coven creating the store and
//! running bae's schema — under the log subscriber the apps install, with one
//! sink at debug as the macOS app's unified log takes. Run explicitly, in the
//! release profile:
//!
//!   scripts/cargo.sh test --release -p bae-library --features test-utils \
//!     --test integration -- test_library_create_cost:: --ignored --nocapture
//!
//! Creates the library several times, each in a fresh directory, and prints
//! each creation's time and the median.

use bae_util::logging::LogSink;
use std::time::{Duration, Instant};
use tracing_subscriber::filter::LevelFilter;

const CREATIONS: usize = 5;

#[tokio::test(flavor = "multi_thread")]
#[ignore = "one timing measurement, run explicitly in the release profile"]
async fn creating_a_library_costs() {
    // The process's subscriber is set once, and an install after another
    // test's keeps that one: the measurement would then time its logging.
    assert!(
        !tracing::dispatcher::has_been_set(),
        "another test in this process installed the log subscriber; \
         run this measurement alone, filtered to test_library_create_cost::"
    );
    // The sink discards its lines, so the time is the logging setup's and
    // the library's, not a terminal's.
    bae_util::logging::install(vec![LogSink::new(
        tracing_subscriber::fmt::layer().with_writer(std::io::sink),
        LevelFilter::DEBUG,
    )]);
    let mut times: Vec<Duration> = Vec::with_capacity(CREATIONS);
    for _ in 0..CREATIONS {
        let directory = tempfile::TempDir::new().unwrap();
        let app_dir = bae_app_dir::AppDir::under_home(directory.path()).unwrap();
        let installation = coven::Coven::in_memory(app_dir.coven_app_dir());
        let clock: coven::ClockRef = std::sync::Arc::new(coven::SystemClock);
        let builder = bae_store::TestSources::on(&clock).builder(&installation);
        let name = bae_store::EnteredName::parse("Bench").unwrap();
        let started = Instant::now();
        let handle = bae_library::create_library(
            builder,
            name,
            bae_store::EnteredName::parse("Creator").unwrap(),
            clock.clone(),
            &bae_diagnostics::Diagnostics::noop(),
        )
        .await
        .unwrap();
        let created_in = started.elapsed();
        handle.close().await.unwrap();
        eprintln!("created a library in {created_in:?}");
        times.push(created_in);
    }
    times.sort();
    eprintln!("median of {CREATIONS} library creations: {:?}", times[CREATIONS / 2]);
    bae_store::test_fixtures::window_cost::report_cost(
        "create_library",
        times[CREATIONS / 2].as_secs_f64() * 1000.0,
        "ms",
    );
}
