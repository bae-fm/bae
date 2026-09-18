//! AccurateRip: the three disc ids that name a disc's record, and the two
//! checksum schemes that say whether our bits match everybody else's.
//!
//! A drive reads a CD a few samples off from where the disc says the audio
//! starts, and the offset differs per drive and per pressing. AccurateRip
//! absorbs that two ways: the first and last audio tracks drop the samples an
//! offset could shift off the disc entirely, and every record carries a
//! checksum over one sector deep inside each track, which a rip can slide
//! against to discover the pressing's offset.

use super::toc::DiscToc;
use super::{TrackEdges, SAMPLES_PER_SECTOR};
use std::ops::Range;

/// Samples AccurateRip leaves out at the start of the first audio track.
const LEAD_IN_SAMPLES: usize = 2939;

/// Samples AccurateRip leaves out at the end of the last audio track.
const LEAD_OUT_SAMPLES: usize = 2940;

/// The widest pressing offset AccurateRip's records describe, in samples. The
/// lead-in and lead-out skips above are sized to cover exactly this much slide.
pub const MAX_PRESSING_OFFSET: i32 = 2939;

/// The sector every record's offset-finding checksum is taken over.
const OFFSET_FINDING_SECTOR: usize = 450;

/// The three ids that name a disc's AccurateRip record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccurateRipIds {
    /// The audio tracks' start sectors plus the disc's lead-out.
    pub id1: u32,
    /// The same sectors weighted by their position among the audio tracks.
    pub id2: u32,
    /// The CDDB disc id, over every track including data.
    pub id3: u32,
}

impl AccurateRipIds {
    /// The ids for a disc. Data tracks are left out of `id1` and `id2` but the
    /// lead-out still sits past them, and `id3` counts them — the combination
    /// that makes an enhanced CD's record findable (ARver, `doc/data_track.md`).
    pub fn of(toc: &DiscToc) -> Self {
        let mut id1 = 0u32;
        let mut id2 = 0u32;
        let mut index = 0u32;
        for track in toc.audio_tracks() {
            index += 1;
            id1 = id1.wrapping_add(track.start);
            id2 = id2.wrapping_add(track.start.max(1).wrapping_mul(index));
        }
        let leadout = toc.leadout();
        Self {
            id1: id1.wrapping_add(leadout),
            id2: id2.wrapping_add(leadout.max(1).wrapping_mul(index + 1)),
            id3: super::cddb::cddb_disc_id(toc),
        }
    }

    /// Where the disc's record lives under AccurateRip's host: three directory
    /// levels taken from the low hex digits of `id1`, then the record file.
    pub fn dbar_path(&self, audio_tracks: u32) -> String {
        format!(
            "accuraterip/{:x}/{:x}/{:x}/dBAR-{:03}-{:08x}-{:08x}-{:08x}.bin",
            self.id1 & 0xF,
            (self.id1 >> 4) & 0xF,
            (self.id1 >> 8) & 0xF,
            audio_tracks,
            self.id1,
            self.id2,
            self.id3,
        )
    }
}

/// One track's checksum under each of AccurateRip's two schemes. A record may
/// hold either; a rip matches when either agrees.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrackChecksums {
    pub v1: u32,
    pub v2: u32,
}

/// The window `track` describes once slid by `offset`, or `None` when the slide
/// would leave the disc.
fn slid(track: Range<usize>, offset: i32, disc_length: usize) -> Option<Range<usize>> {
    let offset = offset as isize;
    let start = track.start.checked_add_signed(offset)?;
    let end = track.end.checked_add_signed(offset)?;
    (end <= disc_length).then_some(start..end)
}

/// One track's AccurateRip checksums, taken over the whole disc's samples so a
/// slid window can reach into the neighbouring track.
///
/// `track` is the track's own frame range within `disc`; `offset` slides that
/// window by a pressing offset. `None` when the track is shorter than the skips
/// its edges call for, or when the slid window leaves the disc.
pub fn track_checksums(
    disc: &[u32],
    track: Range<usize>,
    edges: TrackEdges,
    offset: i32,
) -> Option<TrackChecksums> {
    let length = track.end.checked_sub(track.start)?;
    let skip_head = if edges.first { LEAD_IN_SAMPLES } else { 0 };
    let keep_to = if edges.last {
        length.checked_sub(LEAD_OUT_SAMPLES)?
    } else {
        length
    };
    if skip_head >= keep_to {
        return None;
    }
    let window = slid(
        track.start + skip_head..track.start + keep_to,
        offset,
        disc.len(),
    )?;
    let mut v1 = 0u32;
    let mut high = 0u32;
    for (index, sample) in disc[window].iter().enumerate() {
        // The 1-based sample index within the track, which the skipped head
        // still counts toward.
        let position = (skip_head + index + 1) as u64;
        let product = u64::from(*sample) * position;
        v1 = v1.wrapping_add(product as u32);
        high = high.wrapping_add((product >> 32) as u32);
    }
    Some(TrackChecksums {
        v1,
        v2: v1.wrapping_add(high),
    })
}

/// The offset-finding checksum: one sector deep enough into the track that
/// every pressing has audio there, weighted by position within that sector.
/// `None` when the track (slid) does not reach that far.
pub fn crc450(disc: &[u32], track_start: usize, offset: i32) -> Option<u32> {
    let head = track_start.checked_add(OFFSET_FINDING_SECTOR * SAMPLES_PER_SECTOR)?;
    let window = slid(
        head..head.checked_add(SAMPLES_PER_SECTOR)?,
        offset,
        disc.len(),
    )?;
    let mut crc = 0u32;
    for (index, sample) in disc[window].iter().enumerate() {
        crc = crc.wrapping_add(sample.wrapping_mul(index as u32 + 1));
    }
    Some(crc)
}

/// Offsets to try, nearest first, so an unshifted pressing wins over a distant
/// coincidence.
fn candidate_offsets() -> impl Iterator<Item = i32> {
    std::iter::once(0).chain((1..=MAX_PRESSING_OFFSET).flat_map(|delta| [delta, -delta]))
}

/// The pressing offset at which this track's sector 450 checksums to what a
/// record says it should. `None` when no offset in AccurateRip's range does.
pub fn find_pressing_offset(disc: &[u32], track_start: usize, expected: u32) -> Option<i32> {
    candidate_offsets().find(|offset| crc450(disc, track_start, *offset) == Some(expected))
}

#[cfg(test)]
#[path = "accuraterip_tests.rs"]
mod tests;
