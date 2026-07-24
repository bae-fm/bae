//! MusicBrainz DiscID: the SHA-1-over-TOC identifier MusicBrainz uses to key a
//! CD release by its table of contents.
//!
//! Ported from libdiscid's `discid_put` + `create_disc_id` (LGPL-2.1). bae never
//! reads a physical drive — it only hashes sector offsets it already parsed from
//! CUE/LOG artifacts — so the whole of libdiscid we needed is this pure function.
//! Computing it in Rust drops the C library and every prebuilt-fetch of it.

use sha1::{Digest, Sha1};

/// A CD holds at most 99 tracks; the TOC hash walks 100 offset slots (lead-out
/// followed by tracks 1..=99), zero-filling the slots past the last track.
const MAX_TRACKS: usize = 99;

/// libdiscid's `MAX_DISC_LENGTH`: 90 minutes at 75 sectors per second.
const MAX_DISC_LENGTH: i32 = 90 * 60 * 75;

/// libdiscid's URL-safe base64 alphabet: standard base64 with `+`→`.` and `/`→`_`.
/// The `=` padding is swapped to `-` after encoding.
const MUSICBRAINZ_BASE64_ALPHABET: &str =
    "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789._";

/// Why a set of offsets can't form a MusicBrainz DiscID. Mirrors the rejections
/// libdiscid's `discid_put` makes, so bae skips a malformed TOC rather than
/// hashing nonsense into a plausible-looking but meaningless ID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum DiscIdError {
    /// Track count (offsets minus the lead-out) is outside 1..=99.
    IllegalTrackCount(usize),
    /// The lead-out exceeds a 90-minute disc.
    DiscTooLong(i32),
    /// A track (or the lead-out slot) sits past the lead-out offset.
    OffsetPastLeadOut {
        index: usize,
        offset: i32,
        lead_out: i32,
    },
    /// Track offsets are not non-decreasing.
    OffsetsOutOfOrder { index: usize },
}

impl std::fmt::Display for DiscIdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiscIdError::IllegalTrackCount(n) => {
                write!(f, "illegal track count {n} (must be 1..=99)")
            }
            DiscIdError::DiscTooLong(lead_out) => {
                write!(
                    f,
                    "disc too long: lead-out {lead_out} exceeds {MAX_DISC_LENGTH}"
                )
            }
            DiscIdError::OffsetPastLeadOut {
                index,
                offset,
                lead_out,
            } => write!(
                f,
                "offset {offset} at slot {index} is past the lead-out {lead_out}"
            ),
            DiscIdError::OffsetsOutOfOrder { index } => {
                write!(f, "track offset at slot {index} is out of order")
            }
        }
    }
}

/// Compute the 28-character MusicBrainz DiscID from disc offsets.
///
/// `offsets[0]` is the lead-out (the total number of sectors on the disc);
/// `offsets[1..]` are the start sectors of tracks 1..=N, each already carrying
/// the 150-sector pregap. The ID is SHA-1 over the uppercase-hex TOC — the
/// first and last track numbers (`%02X`) followed by 100 offset slots (`%08X`,
/// lead-out first, then tracks, zero-filled) — base64-encoded with libdiscid's
/// URL-safe alphabet.
pub(super) fn musicbrainz_discid(offsets: &[i32]) -> Result<String, DiscIdError> {
    let track_count = offsets.len().saturating_sub(1);
    if !(1..=MAX_TRACKS).contains(&track_count) {
        return Err(DiscIdError::IllegalTrackCount(track_count));
    }
    let lead_out = offsets[0];
    if lead_out > MAX_DISC_LENGTH {
        return Err(DiscIdError::DiscTooLong(lead_out));
    }
    for (index, &offset) in offsets.iter().enumerate() {
        if offset > lead_out {
            return Err(DiscIdError::OffsetPastLeadOut {
                index,
                offset,
                lead_out,
            });
        }
        // libdiscid orders track offsets only (index > 1): the lead-out at slot 0
        // is the largest value and precedes track 1 in the array.
        if index > 1 && offsets[index - 1] > offset {
            return Err(DiscIdError::OffsetsOutOfOrder { index });
        }
    }

    let first_track: u8 = 1;
    let last_track = track_count as u8;

    let mut hasher = Sha1::new();
    hasher.update(format!("{first_track:02X}").as_bytes());
    hasher.update(format!("{last_track:02X}").as_bytes());
    for slot in 0..=MAX_TRACKS {
        // Slots past the last track hash as zero. `%08X` reads the int unsigned,
        // matching libdiscid's cast.
        let value = offsets.get(slot).copied().unwrap_or(0) as u32;
        hasher.update(format!("{value:08X}").as_bytes());
    }
    let digest = hasher.finalize();

    Ok(base64_musicbrainz(&digest))
}

/// libdiscid's `rfc822_binary`: standard base64 over the alphabet with `+`→`.`
/// and `/`→`_`, then `=` padding rewritten to `-`. A 20-byte SHA-1 digest always
/// yields exactly 28 characters ending in a single `-`, well under the 60-char
/// line-wrap libdiscid never reaches here.
fn base64_musicbrainz(digest: &[u8]) -> String {
    use base64::engine::general_purpose::{GeneralPurpose, PAD};
    use base64::{alphabet::Alphabet, Engine};

    let alphabet =
        Alphabet::new(MUSICBRAINZ_BASE64_ALPHABET).expect("static alphabet is valid base64");
    let engine = GeneralPurpose::new(&alphabet, PAD);
    engine.encode(digest).replace('=', "-")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fixed TOCs with their MusicBrainz DiscIDs. The expected strings were
    /// produced by the reference C library (`discid` crate) and baked in as
    /// literals so these tests outlive the dependency. `offsets[0]` is the
    /// lead-out; the rest are track start sectors including the 150-sector pregap.
    fn fixed_vectors() -> Vec<(Vec<i32>, &'static str)> {
        vec![
            // 11-track disc — the `discid` crate's own documented example.
            (
                vec![
                    242457, 150, 44942, 61305, 72755, 96360, 130485, 147315, 164275, 190702,
                    205412, 220437,
                ],
                "lSOVc5h6IXSuzcamJS1Gp4_tRuA-",
            ),
            // Single track, lead-out after the 150-sector pregap.
            (vec![180_150, 150], "NyB4TAo0fl6yU3tBAcL.jKphZs0-"),
            // Typical 3-track disc.
            (
                vec![270_150, 150, 90_150, 180_150],
                "OW7Dlo_1oOnvIk1X5vG229avEII-",
            ),
            // 99 tracks — the maximum a CD (and this hash) allows.
            (max_track_toc(), "qOt.RNFgr441aSUXpEjVegEMurg-"),
        ]
    }

    /// A 99-track TOC: track k starts at `150 + k*1000`, lead-out after track 99.
    fn max_track_toc() -> Vec<i32> {
        let mut offsets = vec![150 + 100 * 1000];
        for k in 0..99 {
            offsets.push(150 + k * 1000);
        }
        offsets
    }

    #[test]
    fn known_musicbrainz_discids() {
        for (offsets, expected) in fixed_vectors() {
            assert_eq!(
                musicbrainz_discid(&offsets).expect("valid TOC"),
                expected,
                "DiscID mismatch for offsets {offsets:?}"
            );
        }
    }

    #[test]
    fn discid_is_28_url_safe_chars() {
        let id = musicbrainz_discid(&fixed_vectors()[0].0).unwrap();
        assert_eq!(id.len(), 28);
        assert!(id.ends_with('-'));
        assert!(id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')));
    }

    #[test]
    fn rejects_no_tracks() {
        assert_eq!(
            musicbrainz_discid(&[150]),
            Err(DiscIdError::IllegalTrackCount(0))
        );
    }

    #[test]
    fn rejects_too_many_tracks() {
        let offsets = vec![0; 102]; // lead-out + 101 tracks
        assert_eq!(
            musicbrainz_discid(&offsets),
            Err(DiscIdError::IllegalTrackCount(101))
        );
    }

    #[test]
    fn rejects_disc_too_long() {
        let lead_out = MAX_DISC_LENGTH + 1;
        assert_eq!(
            musicbrainz_discid(&[lead_out, 150]),
            Err(DiscIdError::DiscTooLong(lead_out))
        );
    }

    #[test]
    fn rejects_track_past_lead_out() {
        assert_eq!(
            musicbrainz_discid(&[1000, 2000]),
            Err(DiscIdError::OffsetPastLeadOut {
                index: 1,
                offset: 2000,
                lead_out: 1000,
            })
        );
    }

    #[test]
    fn rejects_out_of_order_tracks() {
        assert_eq!(
            musicbrainz_discid(&[10_000, 150, 5000, 4000]),
            Err(DiscIdError::OffsetsOutOfOrder { index: 3 })
        );
    }
}
