//! What the audio being identified is, read off wherever it is described: a
//! candidate's scanned files, or a library release's own files and tracks.
//! Read again for every run and every read of a stored answer, never stored
//! beside what was concluded from it.

use crate::album_detail::AudioFormat;
use crate::import::folder_scanner::CategorizedFiles;
use crate::import::probe::SourceDurations;

/// The facts of one release's audio that identification reads.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AudioFacts {
    pub track_count: u32,
    /// How long each audio unit plays, which a lead's tracklist is fitted to.
    /// Empty for a library release, which no settle fits.
    pub durations: SourceDurations,
    /// The same lengths in the order the tracks are laid out, which a
    /// release's tracklist is read against to count the tracks it holds for
    /// this audio. A library release's are its tracks' stored lengths, empty
    /// where one was never measured, and its tracklist is then read whole.
    pub track_lengths_ms: Vec<u64>,
    /// Every audio file carries one channel.
    pub mono: bool,
    /// The rate that rules a CD out, when the audio rules one out — see
    /// [`super::rip::rate_ruling_out_cd`].
    pub rate_ruling_out_cd: Option<u32>,
}

impl AudioFacts {
    /// A candidate's audio, from the facts its scan stored for each file.
    pub fn of_files(files: &CategorizedFiles) -> Result<Self, crate::import::ImportError> {
        let durations = crate::import::probe::source_durations(files)?;
        let track_lengths_ms = crate::import::audio_layout::audio_durations(files, &durations)?;
        Ok(Self::of(
            files.track_count(),
            durations,
            track_lengths_ms,
            files
                .audio()
                .filter_map(|file| file.source_audio.as_ref())
                .map(|audio| &audio.format),
        ))
    }

    /// A library release's audio: its track count, its tracks' lengths, and
    /// its files' formats.
    pub(crate) fn of_release<'a>(
        track_count: u32,
        track_lengths_ms: Vec<u64>,
        formats: impl IntoIterator<Item = &'a AudioFormat>,
    ) -> Self {
        Self::of(track_count, SourceDurations::default(), track_lengths_ms, formats)
    }

    fn of<'a>(
        track_count: u32,
        durations: SourceDurations,
        track_lengths_ms: Vec<u64>,
        formats: impl IntoIterator<Item = &'a AudioFormat>,
    ) -> Self {
        let formats: Vec<&AudioFormat> = formats.into_iter().collect();
        Self {
            track_count,
            durations,
            track_lengths_ms,
            mono: !formats.is_empty() && formats.iter().all(|format| format.channels == 1),
            rate_ruling_out_cd: super::rip::rate_ruling_out_cd(formats.iter().copied()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn format(sample_rate_hz: i64, channels: i64) -> AudioFormat {
        AudioFormat {
            codec: "FLAC".to_string(),
            sample_rate_hz,
            bits_per_sample: Some(24),
            bitrate_kbps: None,
            channels,
        }
    }

    /// Audio is mono when every file carries one channel, and its rate rules
    /// a CD out by the rip rule.
    #[test]
    fn the_facts_are_read_off_every_format() {
        let mono = format(96_000, 1);
        let stereo = format(96_000, 2);
        let facts = AudioFacts::of_release(2, Vec::new(), [&mono, &mono]);
        assert!(facts.mono);
        assert_eq!(facts.rate_ruling_out_cd, Some(96_000));
        assert_eq!(facts.track_count, 2);
        assert!(!AudioFacts::of_release(2, Vec::new(), [&mono, &stereo]).mono);
        assert!(!AudioFacts::of_release(0, Vec::new(), []).mono);
        assert_eq!(
            AudioFacts::of_release(1, Vec::new(), [&format(44_100, 1)]).rate_ruling_out_cd,
            None
        );
    }
}
