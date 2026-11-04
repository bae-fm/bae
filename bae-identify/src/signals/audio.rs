//! What the audio being identified is, read off wherever it is described: a
//! candidate's scanned files, or a library release's own files and tracks.
//! Read again for every run and every read of a stored answer, never stored
//! beside what was concluded from it.

use bae_types::audio_format::AudioFormat;

/// The facts of one release's audio that identification reads.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AudioFacts {
    pub track_count: u32,
    /// How long each track plays, in the order the tracks are laid out,
    /// which a release's tracklist is read against. A library release's are
    /// its tracks' stored lengths.
    pub track_lengths_ms: Vec<u64>,
    /// Every audio file carries one channel.
    pub mono: bool,
}

impl AudioFacts {
    /// The audio of `track_count` tracks playing for `track_lengths_ms`, in
    /// files of `formats`.
    pub fn of<'a>(
        track_count: u32,
        track_lengths_ms: Vec<u64>,
        formats: impl IntoIterator<Item = &'a AudioFormat>,
    ) -> Self {
        let formats: Vec<&AudioFormat> = formats.into_iter().collect();
        Self {
            track_count,
            track_lengths_ms,
            mono: !formats.is_empty() && formats.iter().all(|format| format.channels == 1),
        }
    }
}

#[cfg(test)]
#[path = "audio_tests.rs"]
mod tests;
