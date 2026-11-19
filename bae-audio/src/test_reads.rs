//! What this process's file reads have cost, for tests: how many times each
//! file was probed, so a bae-library test can assert that asking about a folder
//! again opens nothing with FFmpeg; and how many bytes a landing search read,
//! so this crate's tests can assert a search doesn't read the whole file.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

#[cfg(feature = "test-instrumentation")]
fn probe_counts() -> &'static Mutex<HashMap<PathBuf, u64>> {
    static COUNTS: OnceLock<Mutex<HashMap<PathBuf, u64>>> = OnceLock::new();
    COUNTS.get_or_init(Default::default)
}

/// Count one probe of `path`.
#[cfg(feature = "test-instrumentation")]
pub(crate) fn record_probe(path: &Path) {
    *probe_counts()
        .lock()
        .expect("probe counts mutex poisoned")
        .entry(path.to_path_buf())
        .or_default() += 1;
}

/// How many times `path` has been probed.
#[cfg(feature = "test-instrumentation")]
pub fn probes(path: &Path) -> u64 {
    probe_counts().lock().expect("probe counts mutex poisoned").get(path).copied().unwrap_or(0)
}

#[cfg(test)]
thread_local! {
    /// The bytes the landing searches this thread ran have read.
    static LANDING_SEARCH_BYTES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Count `bytes` a landing search read. A search runs on the thread that
/// calls it, so a test counts its own searches.
#[cfg(test)]
pub(crate) fn record_landing_search(bytes: u64) {
    LANDING_SEARCH_BYTES.with(|total| total.set(total.get() + bytes));
}

/// How many bytes the landing searches of this thread have read.
#[cfg(test)]
pub(crate) fn landing_search() -> u64 {
    LANDING_SEARCH_BYTES.with(std::cell::Cell::get)
}
