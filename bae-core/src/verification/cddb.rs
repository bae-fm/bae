//! The CDDB (freedb/gnudb) disc id, which is also AccurateRip's third id.
//!
//! Eight hex digits: a checksum over each track's start second, the disc's
//! playing length in seconds, and the track count. Every track counts, data
//! tracks included — which is what makes an enhanced CD's id differ from the
//! one computed from its audio alone.

use super::toc::DiscToc;

/// Sectors per second on a CD.
const SECTORS_PER_SECOND: u32 = 75;

/// The lead-in CDDB counts from: a sector's LBA is its LSN plus 150.
const LEAD_IN_SECTORS: u32 = 150;

/// The sum of a number's decimal digits.
fn digit_sum(mut n: u32) -> u32 {
    let mut sum = 0;
    while n > 0 {
        sum += n % 10;
        n /= 10;
    }
    sum
}

/// The disc's CDDB id. Print it as eight lowercase hex digits (`{:08x}`).
pub fn cddb_disc_id(toc: &DiscToc) -> u32 {
    let second_of = |sector: u32| (sector + LEAD_IN_SECTORS) / SECTORS_PER_SECOND;
    let checksum: u32 = toc.tracks().iter().fold(0u32, |sum, track| {
        sum.wrapping_add(digit_sum(second_of(track.start)))
    });
    let first_start = toc
        .tracks()
        .first()
        .expect("a validated TOC holds at least one track")
        .start;
    let seconds = second_of(toc.leadout()) - second_of(first_start);
    ((checksum % 255) << 24) | ((seconds & 0xFFFF) << 8) | (toc.tracks().len() as u32 & 0xFF)
}

#[cfg(test)]
#[path = "cddb_tests.rs"]
mod tests;
