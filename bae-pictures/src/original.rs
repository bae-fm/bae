//! The original bytes of a picture, for a viewer that decodes it at full size.

use std::io::Read;

use crate::{IsCancelled, PicturesError};

/// The bytes read from an origin by one read.
const CHUNK: usize = 64 * 1024;

/// Everything `origin` holds, read in chunks with `cancelled` asked before
/// each: the original a viewer decodes at full size.
pub fn read_original(
    mut origin: impl Read,
    cancelled: &IsCancelled,
) -> Result<Vec<u8>, PicturesError> {
    let mut bytes = Vec::new();
    let mut chunk = vec![0u8; CHUNK];
    loop {
        if cancelled() {
            return Err(PicturesError::Cancelled);
        }
        let count = origin.read(&mut chunk).map_err(PicturesError::Read)?;
        if count == 0 {
            return Ok(bytes);
        }
        bytes.extend_from_slice(&chunk[..count]);
    }
}
