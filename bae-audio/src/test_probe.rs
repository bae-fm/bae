//! Probes and landing searches of a file a test names by path: the test opens
//! the file in a place of a test budget, as the scan, the import or the
//! preview opens a file, and hands the open file to the reader.

use crate::{probe_audio, ProbeError, ProbeResult};
use bae_util::test_files::{lease, open_in};
use std::path::Path;

/// [`probe_audio`] over the file at `path`; a file that won't open is the
/// probe's `Io` error.
pub fn probe_file(path: &Path) -> Result<ProbeResult, ProbeError> {
    let file = open_in(lease().place_blocking(), path).map_err(ProbeError::Io)?;
    probe_audio(file, path)
}

/// [`crate::seek_landing_bytes`] over the file at `path`, with the places it
/// takes from a test budget.
#[cfg(desktop)]
pub fn landing_bytes(path: &Path, samples: &[u64]) -> Result<Vec<u64>, String> {
    let lease = lease();
    let open = || {
        open_in(lease.place_blocking(), path)
            .map_err(|error| format!("cannot open {}: {error}", path.display()))
    };
    crate::seek_landing_bytes([open()?, open()?], samples)
}
