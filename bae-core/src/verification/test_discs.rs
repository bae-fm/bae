//! Discs the kernel tests compute against: the two checked-in rip logs, and the
//! two layouts ARver publishes worked disc ids for (`doc/data_track.md`).

use super::{DiscToc, TocTrack};
use std::path::Path;

/// The TOC a checked-in rip log describes, read the way the import pass reads
/// it — off disk through the encoding detection, since EAC writes UTF-16LE.
pub(super) fn from_fixture(name: &str) -> DiscToc {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/logs")
        .join(name);
    let text = crate::text_encoding::read_text_file(&path)
        .expect("log fixture should be readable")
        .text;
    DiscToc::from_log_text(&text).expect("log fixture should carry a TOC")
}

/// An all-audio disc from contiguous start sectors: each track runs to the next
/// one's start, and the lead-out follows the last.
pub(super) fn audio_disc(starts: &[u32], leadout: u32) -> DiscToc {
    let tracks = starts
        .iter()
        .enumerate()
        .map(|(index, start)| TocTrack {
            number: index as u8 + 1,
            start: *start,
            end: starts.get(index + 1).copied().unwrap_or(leadout) - 1,
            audio: true,
        })
        .collect();
    DiscToc::new(tracks, leadout).expect("a contiguous audio disc is a valid TOC")
}

/// The enhanced CD ARver works through: ten audio tracks, then a data track in
/// a second session 11400 sectors past the end of the audio.
///
/// Track starts are the LSNs `pycdio` reports in that document. Its other two
/// listings — `discid`'s `sectors`/`offset-list` and the start/length/end table
/// — are in LBA, 150 sectors higher, so the audio session's lead-out reads
/// 270335 there and 270185 here. Track 10's documented length says the same:
/// 247305 + 22880 sectors ends it at 270184, and the data track at 281585 then
/// sits the document's usual 11400 sectors past that.
pub(super) fn enhanced_cd() -> DiscToc {
    const AUDIO_LEADOUT: u32 = 270_185;
    let audio = [
        0, 12617, 40337, 63075, 93260, 117965, 151715, 184190, 215110, 247305,
    ];
    let mut tracks: Vec<TocTrack> = audio
        .iter()
        .enumerate()
        .map(|(index, start)| TocTrack {
            number: index as u8 + 1,
            start: *start,
            end: audio.get(index + 1).copied().unwrap_or(AUDIO_LEADOUT) - 1,
            audio: true,
        })
        .collect();
    tracks.push(TocTrack {
        number: 11,
        start: 281585,
        end: 333650,
        audio: false,
    });
    DiscToc::new(tracks, 333651).expect("an enhanced CD is a valid TOC")
}

/// The mixed mode CD ARver works through: a data track first, then 28 audio
/// tracks, all in one session. Sectors are its LBA offsets less the 150-sector
/// lead-in.
pub(super) fn mixed_mode_cd() -> DiscToc {
    let starts = [
        0, 66578, 76352, 85813, 93610, 103898, 115916, 124593, 134056, 142766, 151702, 160570,
        169275, 178581, 188309, 196561, 206204, 214695, 223536, 225596, 226234, 232518, 239297,
        239781, 244839, 254114, 259916, 261072, 261524,
    ];
    const LEADOUT: u32 = 261826;
    let tracks = starts
        .iter()
        .enumerate()
        .map(|(index, start)| TocTrack {
            number: index as u8 + 1,
            start: *start,
            end: starts.get(index + 1).copied().unwrap_or(LEADOUT) - 1,
            audio: index > 0,
        })
        .collect();
    DiscToc::new(tracks, LEADOUT).expect("a mixed mode CD is a valid TOC")
}

/// A deterministic stand-in for ripped audio: a linear congruential sequence,
/// so every window's checksum depends on exactly where that window starts.
pub(super) fn pattern_frames(count: usize) -> Vec<u32> {
    let mut state = 0x1234_5678u32;
    (0..count)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            state
        })
        .collect()
}
