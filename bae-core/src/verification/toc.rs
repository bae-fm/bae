//! The table of contents every disc id and checksum on this side is computed
//! from.
//!
//! Sectors are 0-based (LSN) — the numbering a rip log's TOC table prints.
//! CDDB works in LBA, which is the same sector plus the 150-sector lead-in;
//! the conversion happens where CDDB needs it, not here.

use super::VerificationError;

/// A CD holds at most 99 tracks.
const MAX_TRACKS: usize = 99;

/// One track's extent on the disc, in sectors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TocTrack {
    /// The number the disc's TOC gives the track. On a mixed-mode disc track 1
    /// is the data track, so the audio numbering starts at 2 — which is why
    /// AccurateRip's track index is counted separately, among audio tracks.
    pub number: u8,
    /// First sector of the track.
    pub start: u32,
    /// Last sector of the track, inclusive. Not derivable from the next track's
    /// start: an enhanced CD puts its data track in a second session, 11400
    /// sectors past the end of the audio, and CTDB's TOCID is computed from
    /// where the audio actually ends.
    pub end: u32,
    /// False for a data track. AccurateRip's disc ids and checksums leave data
    /// tracks out; CDDB counts them.
    pub audio: bool,
}

/// A disc's tracks and the sector its content ends at.
///
/// Built through [`DiscToc::new`], which rejects a layout no disc could have,
/// so every kernel downstream can read the tracks without re-checking them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscToc {
    tracks: Vec<TocTrack>,
    leadout: u32,
}

impl DiscToc {
    /// Tracks in disc order, and the first sector after the last track of the
    /// whole disc — data tracks included, so an enhanced CD's lead-out sits
    /// past its data session.
    pub fn new(tracks: Vec<TocTrack>, leadout: u32) -> Result<Self, VerificationError> {
        let invalid = |detail: &str| VerificationError::InvalidToc {
            detail: detail.to_string(),
        };
        if tracks.is_empty() || tracks.len() > MAX_TRACKS {
            return Err(invalid(&format!(
                "a disc holds 1..={MAX_TRACKS} tracks, not {}",
                tracks.len()
            )));
        }
        if !tracks.iter().any(|track| track.audio) {
            return Err(invalid("a disc with no audio track has nothing to verify"));
        }
        for (index, track) in tracks.iter().enumerate() {
            if track.start > track.end {
                return Err(invalid(&format!(
                    "track {} ends at sector {} before its start {}",
                    track.number, track.end, track.start
                )));
            }
            if let Some(previous) = index.checked_sub(1).map(|i| &tracks[i]) {
                if previous.end >= track.start {
                    return Err(invalid(&format!(
                        "track {} starts at sector {} inside track {}",
                        track.number, track.start, previous.number
                    )));
                }
                if previous.number >= track.number {
                    return Err(invalid(&format!(
                        "track {} follows track {} out of order",
                        track.number, previous.number
                    )));
                }
            }
        }
        let last_end = tracks
            .last()
            .expect("a non-empty track list has a last track")
            .end;
        if leadout <= last_end {
            return Err(invalid(&format!(
                "lead-out {leadout} is not past the last track's end {last_end}"
            )));
        }
        Ok(Self { tracks, leadout })
    }

    /// Every track on the disc, data tracks included.
    pub fn tracks(&self) -> &[TocTrack] {
        &self.tracks
    }

    /// The first sector after the last track of the whole disc.
    pub fn leadout(&self) -> u32 {
        self.leadout
    }

    /// The tracks AccurateRip and CTDB checksum, in disc order.
    pub fn audio_tracks(&self) -> impl DoubleEndedIterator<Item = &TocTrack> {
        self.tracks.iter().filter(|track| track.audio)
    }

    /// How many tracks a rip of this disc produces — the count AccurateRip's
    /// record path carries.
    pub fn audio_track_count(&self) -> u32 {
        self.audio_tracks().count() as u32
    }

    /// The first audio track, which AccurateRip and CTDB both skip samples at
    /// the start of.
    pub fn first_audio_track(&self) -> &TocTrack {
        self.audio_tracks()
            .next()
            .expect("a validated TOC holds at least one audio track")
    }

    /// The last audio track, whose tail both databases leave out.
    pub fn last_audio_track(&self) -> &TocTrack {
        self.audio_tracks()
            .next_back()
            .expect("a validated TOC holds at least one audio track")
    }

    /// The TOC an EAC/XLD/CUERipper log prints, read from the log's text.
    ///
    /// A log's TOC table lists the tracks the ripper extracted, which are the
    /// audio ones; a data track it leaves out is invisible here, so an enhanced
    /// CD read this way carries the audio session's lead-out rather than the
    /// disc's. Reading the physical disc is the only way to see the difference
    /// (see ARver's `doc/data_track.md`), and bae never touches a drive.
    pub fn from_log_text(log: &str) -> Result<Self, VerificationError> {
        let rows = crate::import::discid::extract_log_toc_sectors(log).map_err(|e| {
            VerificationError::InvalidToc {
                detail: format!("no TOC in the log: {e}"),
            }
        })?;
        let tracks = rows
            .iter()
            .enumerate()
            .map(|(index, (start, end))| {
                Ok(TocTrack {
                    number: u8::try_from(index + 1).map_err(|_| VerificationError::InvalidToc {
                        detail: format!("log TOC holds {} rows", rows.len()),
                    })?,
                    start: u32::try_from(*start).map_err(|_| VerificationError::InvalidToc {
                        detail: format!("log TOC row {} starts at sector {start}", index + 1),
                    })?,
                    end: u32::try_from(*end).map_err(|_| VerificationError::InvalidToc {
                        detail: format!("log TOC row {} ends at sector {end}", index + 1),
                    })?,
                    audio: true,
                })
            })
            .collect::<Result<Vec<_>, VerificationError>>()?;
        let leadout = tracks
            .last()
            .expect("the log TOC parser returns at least one row")
            .end
            + 1;
        Self::new(tracks, leadout)
    }

    /// The TOC a cue sheet and the measured lengths of the audio it names
    /// describe — the same inputs the MusicBrainz disc ID is computed from when
    /// a folder carries no log.
    ///
    /// A sheet lays out only the audio it names, so a sheet that also declares
    /// a data track describes a disc this cannot measure: the data track's
    /// sectors are in no audio file, and leaving them out would move the
    /// lead-out every disc id here is taken against. Such a sheet is refused
    /// rather than reduced to its audio.
    pub fn from_cue(
        sheet: &crate::cue_flac::CueSheet,
        audio: &[crate::import::discid::SheetAudioDuration<'_>],
    ) -> Result<Self, VerificationError> {
        let invalid = |detail: String| VerificationError::InvalidToc { detail };
        if let Some(data) = sheet.tracks.iter().find(|track| !track.is_playable_audio()) {
            return Err(invalid(format!(
                "CUE track {} is not audio, so its sectors are in no file",
                data.number
            )));
        }
        let layout = crate::import::discid::cue_disc_layout(sheet, audio)
            .map_err(|e| invalid(format!("CUE lays out no disc: {e}")))?;
        let sector = |value: i32, what: &str| {
            u32::try_from(value).map_err(|_| invalid(format!("CUE {what} is sector {value}")))
        };
        let leadout = sector(layout.leadout, "lead-out")?;
        let tracks = layout
            .tracks
            .iter()
            .enumerate()
            .map(|(index, (number, start))| {
                let next = layout
                    .tracks
                    .get(index + 1)
                    .map(|(_, start)| *start)
                    .unwrap_or(layout.leadout);
                Ok(TocTrack {
                    number: u8::try_from(*number)
                        .map_err(|_| invalid(format!("CUE holds track {number}")))?,
                    start: sector(*start, "track start")?,
                    end: sector(next, "track end")?
                        .checked_sub(1)
                        .ok_or_else(|| invalid(format!("CUE track {number} ends at sector 0")))?,
                    audio: true,
                })
            })
            .collect::<Result<Vec<_>, VerificationError>>()?;
        Self::new(tracks, leadout)
    }
}

#[cfg(test)]
#[path = "toc_tests.rs"]
mod tests;
