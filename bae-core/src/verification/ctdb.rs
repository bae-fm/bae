//! The CUETools database: the TOC it looks a disc up by, and the CRC-32s it
//! compares a rip against.
//!
//! CTDB keys on a hash of the audio session's layout rather than on sector
//! sums, and checksums plain CRC-32 over the PCM instead of AccurateRip's
//! position-weighted sums. It leaves out a whole 5880-sample block at each end
//! of the disc's audio, which covers any pressing offset without needing to
//! search for one.

use super::toc::DiscToc;
use super::{TrackEdges, SAMPLES_PER_SECTOR};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use sha1::{Digest, Sha1};
use std::ops::Range;

/// CTDB's TOCID hashes 100 track slots however many the disc has.
const TOCID_SLOTS: usize = 100;

/// Stereo samples CTDB leaves out at each end of the disc's audio: ten sectors,
/// wide enough to swallow any pressing offset.
const EDGE_SAMPLES: usize = 10 * SAMPLES_PER_SECTOR;

/// The disc as CTDB's lookup names it: every track's start sector, data tracks
/// marked by a leading `-`, then the disc's lead-out.
pub fn toc_string(toc: &DiscToc) -> String {
    let mut parts: Vec<String> = toc
        .tracks()
        .iter()
        .map(|track| {
            if track.audio {
                track.start.to_string()
            } else {
                format!("-{}", track.start)
            }
        })
        .collect();
    parts.push(toc.leadout().to_string());
    parts.join(":")
}

/// The disc's CTDB TOCID — the identifier a CUETools log prints as
/// `[CTDB TOCID: …]`.
///
/// Hashed over the audio tracks' starts relative to the first one, then where
/// the audio ends, then empty slots out to 100 tracks; base64 with CTDB's
/// URL-safe substitutions.
pub fn tocid(toc: &DiscToc) -> String {
    let origin = toc.first_audio_track().start;
    let mut slots = String::with_capacity(TOCID_SLOTS * 8);
    for track in toc.audio_tracks().skip(1) {
        slots.push_str(&format!("{:08X}", track.start - origin));
    }
    slots.push_str(&format!("{:08X}", toc.last_audio_track().end + 1 - origin));
    let used = toc.audio_track_count() as usize;
    slots.push_str(&"0".repeat((TOCID_SLOTS - used) * 8));

    let digest = Sha1::digest(slots.as_bytes());
    BASE64
        .encode(digest)
        .replace('+', ".")
        .replace('/', "_")
        .replace('=', "-")
}

/// How many samples CTDB drops from the end of a disc `frames` long. The head
/// drop is always [`EDGE_SAMPLES`]; the tail additionally swallows the partial
/// block the disc ends on, so both ends land on a block boundary.
fn tail_drop(frames: usize) -> usize {
    EDGE_SAMPLES + frames % EDGE_SAMPLES
}

/// CRC-32 (the zlib polynomial) over the frames as they sit on disc: each one
/// two little-endian 16-bit samples, left then right.
fn crc32_of(frames: &[u32]) -> u32 {
    /// Frames converted to bytes per pass. Big enough that the per-call
    /// overhead disappears, small enough to stay in cache.
    const CHUNK_FRAMES: usize = 8192;

    let mut hasher = crc32fast::Hasher::new();
    let mut bytes = Vec::with_capacity(CHUNK_FRAMES * 4);
    for chunk in frames.chunks(CHUNK_FRAMES) {
        bytes.clear();
        for frame in chunk {
            bytes.extend_from_slice(&frame.to_le_bytes());
        }
        hasher.update(&bytes);
    }
    hasher.finalize()
}

/// The whole disc's CTDB CRC-32, from track 1's INDEX 01 with a block dropped
/// at each end. `None` when the disc holds less audio than the drops remove.
pub fn disc_crc32(disc: &[u32]) -> Option<u32> {
    let end = disc.len().checked_sub(tail_drop(disc.len()))?;
    (EDGE_SAMPLES < end).then(|| crc32_of(&disc[EDGE_SAMPLES..end]))
}

/// One track's CTDB CRC-32. The first audio track drops the disc's head block
/// and the last drops its tail; every other track is checksummed whole. `None`
/// when the track is shorter than the drops its edges call for.
pub fn track_crc32(disc: &[u32], track: Range<usize>, edges: TrackEdges) -> Option<u32> {
    let start = if edges.first {
        track.start.checked_add(EDGE_SAMPLES)?
    } else {
        track.start
    };
    let end = if edges.last {
        track.end.checked_sub(tail_drop(disc.len()))?
    } else {
        track.end
    };
    (start < end && end <= disc.len()).then(|| crc32_of(&disc[start..end]))
}

#[cfg(test)]
#[path = "ctdb_tests.rs"]
mod tests;
