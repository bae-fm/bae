//! How many times this test binary has read each file, by what read it, so a
//! test can assert that asking about a folder again reads nothing off disk.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// What read a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum FileRead {
    /// FFmpeg opened it to find its stream.
    Probe,
    /// Its header was checked for the image its name says.
    ImageCheck,
    /// It was parsed as a CUE sheet.
    CueParse,
    /// Its tags were read for the disc and track numbers they state.
    TagNumbers,
}

fn counts() -> &'static Mutex<HashMap<(FileRead, PathBuf), u64>> {
    static COUNTS: OnceLock<Mutex<HashMap<(FileRead, PathBuf), u64>>> = OnceLock::new();
    COUNTS.get_or_init(Default::default)
}

/// Count one `read` of `path`.
pub(crate) fn record(read: FileRead, path: &Path) {
    *counts()
        .lock()
        .expect("file read counts mutex poisoned")
        .entry((read, path.to_path_buf()))
        .or_default() += 1;
}

/// How many times `read` has read `path`.
pub(crate) fn count(read: FileRead, path: &Path) -> u64 {
    counts()
        .lock()
        .expect("file read counts mutex poisoned")
        .get(&(read, path.to_path_buf()))
        .copied()
        .unwrap_or(0)
}
