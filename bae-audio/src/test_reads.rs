//! What this process's file reads have cost, for tests: how many times each
//! file was probed, so a bae-core test can assert that asking about a folder
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
    probe_counts()
        .lock()
        .expect("probe counts mutex poisoned")
        .get(path)
        .copied()
        .unwrap_or(0)
}

#[cfg(test)]
fn landing_search_bytes() -> &'static Mutex<HashMap<PathBuf, u64>> {
    static BYTES: OnceLock<Mutex<HashMap<PathBuf, u64>>> = OnceLock::new();
    BYTES.get_or_init(Default::default)
}

/// Count `bytes` a landing search read from `path`.
#[cfg(test)]
pub(crate) fn record_landing_search(path: &Path, bytes: u64) {
    *landing_search_bytes()
        .lock()
        .expect("landing search bytes mutex poisoned")
        .entry(path.to_path_buf())
        .or_default() += bytes;
}

/// How many bytes landing searches have read from `path`.
#[cfg(test)]
pub(crate) fn landing_search(path: &Path) -> u64 {
    landing_search_bytes()
        .lock()
        .expect("landing search bytes mutex poisoned")
        .get(path)
        .copied()
        .unwrap_or(0)
}
