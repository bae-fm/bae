use super::PreferencesError;
use std::ops::RangeInclusive;

use serde::{Deserialize, Serialize};

/// One piece of an export filename pattern. A pattern is an ordered token list;
/// rendering substitutes each token's value from the track's metadata and joins
/// the non-empty values with single spaces (see `render_save_filename`). The
/// extension is added by the exporter from the chosen format, not the pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SaveFilenameToken {
    Title,
    Artist,
    Album,
    Year,
    TrackNumber,
    DiscNumber,
    TrackTotal,
}

/// The filename pattern the schema's default presets and a new preset start
/// with: the zero-padded track number, then the title.
fn default_save_filename_tokens() -> Vec<SaveFilenameToken> {
    vec![SaveFilenameToken::TrackNumber, SaveFilenameToken::Title]
}

/// The bit depth a lossless save format writes. Always one of these three, so
/// a saved file's depth never depends on what the source happens to state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SaveBitDepth {
    Bits16,
    Bits24,
    Bits32,
}

impl SaveBitDepth {
    pub fn bits(self) -> u32 {
        match self {
            Self::Bits16 => 16,
            Self::Bits24 => 24,
            Self::Bits32 => 32,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SaveCodec {
    Flac { bit_depth: SaveBitDepth },
    Mp3 { bitrate_kbps: u32 },
    Aac { bitrate_kbps: u32 },
    OpusOgg { bitrate_kbps: u32 },
    Wav { bit_depth: SaveBitDepth },
    Aiff { bit_depth: SaveBitDepth },
}

/// A format a preset saves as. Each codec is one format at its own settings:
/// a lossless format's bit depth or a lossy one's bitrate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveFormat {
    Flac,
    Mp3,
    Aac,
    OpusOgg,
    Wav,
    Aiff,
}

impl SaveFormat {
    /// Every format, in the order a format picker offers them.
    pub const ALL: [SaveFormat; 6] =
        [Self::Flac, Self::Mp3, Self::Aac, Self::OpusOgg, Self::Wav, Self::Aiff];

    /// The codec a preset of this format starts at.
    pub fn default_codec(self) -> SaveCodec {
        match self {
            Self::Flac => SaveCodec::Flac { bit_depth: SaveBitDepth::Bits24 },
            Self::Mp3 => SaveCodec::Mp3 { bitrate_kbps: 320 },
            Self::Aac => SaveCodec::Aac { bitrate_kbps: 256 },
            Self::OpusOgg => SaveCodec::OpusOgg { bitrate_kbps: 192 },
            Self::Wav => SaveCodec::Wav { bit_depth: SaveBitDepth::Bits24 },
            Self::Aiff => SaveCodec::Aiff { bit_depth: SaveBitDepth::Bits24 },
        }
    }

    /// The bitrates a lossy format saves at, in kbps; `None` for a lossless
    /// one, which has a bit depth instead.
    pub fn bitrates_kbps(self) -> Option<RangeInclusive<u32>> {
        match self {
            Self::Mp3 => Some(Self::MP3_BITRATES_KBPS),
            Self::Aac | Self::OpusOgg => Some(Self::AAC_OPUS_BITRATES_KBPS),
            Self::Flac | Self::Wav | Self::Aiff => None,
        }
    }

    /// The bitrates MP3 saves and streams at, in kbps.
    pub const MP3_BITRATES_KBPS: RangeInclusive<u32> = 32..=320;
    /// The bitrates AAC and Opus save and stream at, in kbps.
    pub const AAC_OPUS_BITRATES_KBPS: RangeInclusive<u32> = 32..=512;

    /// `kbps` brought into the MP3 range.
    pub fn clamp_mp3_kbps(kbps: u32) -> u32 {
        kbps.clamp(*Self::MP3_BITRATES_KBPS.start(), *Self::MP3_BITRATES_KBPS.end())
    }

    /// `kbps` brought into the AAC and Opus range.
    pub fn clamp_aac_opus_kbps(kbps: u32) -> u32 {
        kbps.clamp(*Self::AAC_OPUS_BITRATES_KBPS.start(), *Self::AAC_OPUS_BITRATES_KBPS.end())
    }

    /// The format's name, a proper noun shown as-is in every locale; a new
    /// preset of the format is named it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Flac => "FLAC",
            Self::Mp3 => "MP3",
            Self::Aac => "AAC",
            Self::OpusOgg => "Opus",
            Self::Wav => "WAV",
            Self::Aiff => "AIFF",
        }
    }
}

impl SaveCodec {
    pub fn format(&self) -> SaveFormat {
        match self {
            Self::Flac { .. } => SaveFormat::Flac,
            Self::Mp3 { .. } => SaveFormat::Mp3,
            Self::Aac { .. } => SaveFormat::Aac,
            Self::OpusOgg { .. } => SaveFormat::OpusOgg,
            Self::Wav { .. } => SaveFormat::Wav,
            Self::Aiff { .. } => SaveFormat::Aiff,
        }
    }

    pub fn extension(&self) -> &'static str {
        match self {
            Self::Flac { .. } => "flac",
            Self::Mp3 { .. } => "mp3",
            Self::Aac { .. } => "m4a",
            Self::OpusOgg { .. } => "ogg",
            Self::Wav { .. } => "wav",
            Self::Aiff { .. } => "aiff",
        }
    }

    pub fn supports_single_file_cue(&self) -> bool {
        match self {
            Self::Aac { .. } | Self::OpusOgg { .. } => false,
            Self::Flac { .. } | Self::Mp3 { .. } | Self::Wav { .. } | Self::Aiff { .. } => true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SavePregapPlacement {
    AppendToPreviousExceptHtoa,
    AppendToPreviousIncludingHtoa,
    Exclude,
    SingleFileWithCue,
}

fn default_export_pregap_placement() -> SavePregapPlacement {
    SavePregapPlacement::AppendToPreviousExceptHtoa
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavePreset {
    pub id: String,
    pub name: String,
    pub codec: SaveCodec,
    pub filename_tokens: Vec<SaveFilenameToken>,
    pub pregap_placement: SavePregapPlacement,
    pub applies_to_track: bool,
    pub applies_to_release: bool,
    /// Whether saved files embed the release's cover art. When false, the cover
    /// file is never read — no wasted download/decrypt for a preset that won't
    /// embed it.
    pub embed_cover: bool,
}

/// What a save writes: one track, or a whole release.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveLevel {
    Track,
    Release,
}

impl std::fmt::Display for SaveLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Track => "track",
            Self::Release => "release",
        })
    }
}

impl SavePreset {
    /// Whether the preset is offered for saves of `level`.
    pub fn applies_to(&self, level: SaveLevel) -> bool {
        match level {
            SaveLevel::Track => self.applies_to_track,
            SaveLevel::Release => self.applies_to_release,
        }
    }

    /// The preset a person adds to their list: FLAC at its starting codec,
    /// named for the format, the default filename pattern, pregaps appended
    /// to the track before except the hidden first-track audio, offered for
    /// tracks and releases, embedding the cover, under an id from `ids`.
    pub fn new_flac(ids: &dyn coven::IdSource) -> Self {
        let format = SaveFormat::Flac;
        SavePreset {
            id: ids.new_id().simple().to_string(),
            name: format.name().to_string(),
            codec: format.default_codec(),
            filename_tokens: default_save_filename_tokens(),
            pregap_placement: default_export_pregap_placement(),
            applies_to_track: true,
            applies_to_release: true,
            embed_cover: true,
        }
    }

    pub fn validate(&self) -> Result<(), PreferencesError> {
        if self.id.trim().is_empty() {
            return Err(PreferencesError::Invalid("export preset id is empty".to_string()));
        }
        if self.name.trim().is_empty() {
            return Err(PreferencesError::Invalid(format!(
                "export preset {} has an empty name",
                self.id
            )));
        }
        if !self.applies_to_track && !self.applies_to_release {
            return Err(PreferencesError::Invalid(format!(
                "export preset {} does not apply to any export level",
                self.id
            )));
        }
        if self.pregap_placement == SavePregapPlacement::SingleFileWithCue && self.applies_to_track
        {
            return Err(PreferencesError::Invalid(format!(
                "export preset {} uses single-file CUE and cannot apply to track export",
                self.id
            )));
        }
        if self.pregap_placement == SavePregapPlacement::SingleFileWithCue
            && !self.codec.supports_single_file_cue()
        {
            return Err(PreferencesError::Invalid(format!(
                "export preset {} uses single-file CUE with an unsupported codec",
                self.id
            )));
        }
        if let SaveCodec::Mp3 { bitrate_kbps }
        | SaveCodec::Aac { bitrate_kbps }
        | SaveCodec::OpusOgg { bitrate_kbps } = self.codec
        {
            let format = self.codec.format();
            let accepted = format.bitrates_kbps().expect("a lossy format has a bitrate range");
            if !accepted.contains(&bitrate_kbps) {
                return Err(PreferencesError::Invalid(format!(
                    "export preset {} has unsupported {} bitrate {}",
                    self.id,
                    format.name(),
                    bitrate_kbps
                )));
            }
        }
        Ok(())
    }
}

/// The presets the schema writes into a new library, for tests that build
/// preferences without one.
#[cfg(any(test, feature = "test-utils"))]
pub(super) fn default_save_presets() -> Vec<SavePreset> {
    vec![
        SavePreset {
            id: "flac".to_string(),
            name: "FLAC".to_string(),
            codec: SaveFormat::Flac.default_codec(),
            filename_tokens: default_save_filename_tokens(),
            pregap_placement: default_export_pregap_placement(),
            applies_to_track: true,
            applies_to_release: true,
            embed_cover: true,
        },
        SavePreset {
            id: "mp3".to_string(),
            name: "MP3".to_string(),
            codec: SaveFormat::Mp3.default_codec(),
            filename_tokens: default_save_filename_tokens(),
            pregap_placement: default_export_pregap_placement(),
            applies_to_track: true,
            applies_to_release: true,
            embed_cover: true,
        },
    ]
}

#[cfg(test)]
#[path = "save_tests.rs"]
mod tests;
