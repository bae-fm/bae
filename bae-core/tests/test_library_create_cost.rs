#![cfg(feature = "test-utils")]
//! What creating a library costs locally — coven creating the store and
//! running bae's schema — under the log subscriber the apps install, with one
//! sink at debug as the macOS app's unified log takes. Run explicitly, in the
//! release profile:
//!
//!   scripts/cargo.sh test --release -p bae-core --features test-utils \
//!     --test test_library_create_cost -- --ignored --nocapture
//!
//! Creates the library several times, each in a fresh directory, and prints
//! each creation's time and the median.

use bae_core::logging::LogSink;
use std::time::{Duration, Instant};
use tracing_subscriber::filter::LevelFilter;

const CREATIONS: usize = 5;

#[tokio::test(flavor = "multi_thread")]
#[ignore = "one timing measurement, run explicitly in the release profile"]
async fn creating_a_library_costs() {
    // The sink discards its lines, so the time is the logging setup's and
    // the library's, not a terminal's.
    bae_core::logging::install(vec![LogSink::new(
        tracing_subscriber::fmt::layer().with_writer(std::io::sink),
        LevelFilter::DEBUG,
    )]);
    let mut times: Vec<Duration> = Vec::with_capacity(CREATIONS);
    for _ in 0..CREATIONS {
        let directory = tempfile::TempDir::new().unwrap();
        let app_dir = bae_core::config::AppDir::under_home(directory.path()).unwrap();
        let installation = coven::TestCoven::new();
        let builder = installation.builder(app_dir.store_layout());
        let name = bae_core::library_name::LibraryName::parse("Bench").unwrap();
        let started = Instant::now();
        let handle = bae_core::library::create_library(builder, name)
            .await
            .unwrap();
        let created_in = started.elapsed();
        handle.close().await.unwrap();
        eprintln!("created a library in {created_in:?}");
        times.push(created_in);
    }
    times.sort();
    eprintln!(
        "median of {CREATIONS} library creations: {:?}",
        times[CREATIONS / 2]
    );
}
