use super::PreferencesError;
use std::num::NonZeroU32;

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

/// The filename pattern the schema starts a library with: the zero-padded
/// track number, then the title.
#[cfg(any(test, feature = "test-utils"))]
pub(super) fn default_save_filename_tokens() -> Vec<SaveFilenameToken> {
    vec![SaveFilenameToken::TrackNumber, SaveFilenameToken::Title]
}

/// The bit depth a lossless save format writes: the depth of the track being
/// saved, or 16 bits for players that can't take more.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SaveBitDepth {
    SameAsSource,
    Bits16,
}

impl SaveBitDepth {
    /// The bits per sample a file is written at, for a source track stored at
    /// `source`. A source with no depth of its own is lossy audio, which a
    /// decoder hands out at 16 bits.
    pub fn bits(self, source: Option<NonZeroU32>) -> u32 {
        match self {
            Self::SameAsSource => source.map_or(16, NonZeroU32::get),
            Self::Bits16 => 16,
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
    /// Every format, in the order a format picker offers them: the lossless
    /// formats, then the lossy ones.
    pub const ALL: [SaveFormat; 6] =
        [Self::Flac, Self::Wav, Self::Aiff, Self::Mp3, Self::Aac, Self::OpusOgg];

    /// [`Self::ALL`] in its runs of lossless and of lossy formats, which a
    /// picker sets apart.
    pub fn groups() -> Vec<Vec<SaveFormat>> {
        Self::ALL
            .chunk_by(|a, b| a.bitrates_kbps().is_some() == b.bitrates_kbps().is_some())
            .map(<[SaveFormat]>::to_vec)
            .collect()
    }

    /// The codec a preset of this format starts at.
    pub fn default_codec(self) -> SaveCodec {
        match self {
            Self::Flac => SaveCodec::Flac { bit_depth: SaveBitDepth::SameAsSource },
            Self::Mp3 => SaveCodec::Mp3 { bitrate_kbps: 320 },
            Self::Aac => SaveCodec::Aac { bitrate_kbps: 256 },
            Self::OpusOgg => SaveCodec::OpusOgg { bitrate_kbps: 192 },
            Self::Wav => SaveCodec::Wav { bit_depth: SaveBitDepth::SameAsSource },
            Self::Aiff => SaveCodec::Aiff { bit_depth: SaveBitDepth::SameAsSource },
        }
    }

    /// The bitrates a lossy format saves at, in kbps, lowest first: the ones
    /// its encoder writes. `None` for a lossless format, which has a bit depth
    /// instead.
    pub fn bitrates_kbps(self) -> Option<&'static [u32]> {
        match self {
            // libmp3lame's constant bitrates; anything between is rounded.
            Self::Mp3 => Some(&[32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320]),
            Self::Aac => Some(&[96, 128, 160, 192, 256, 320]),
            Self::OpusOgg => Some(&[64, 96, 128, 160, 192, 256]),
            Self::Flac | Self::Wav | Self::Aiff => None,
        }
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

    /// This codec's setting under `format`: the bit depth carries across
    /// lossless formats, the bitrate across lossy ones (to the nearest the new
    /// format writes, the lower of two equally near); a switch between
    /// lossless and lossy starts the new format's own codec.
    pub fn switched_to(&self, format: SaveFormat) -> SaveCodec {
        let start = format.default_codec();
        match (self, &start) {
            (
                Self::Flac { bit_depth } | Self::Wav { bit_depth } | Self::Aiff { bit_depth },
                SaveCodec::Flac { .. } | SaveCodec::Wav { .. } | SaveCodec::Aiff { .. },
            ) => start.with_bit_depth(*bit_depth),
            (
                Self::Mp3 { bitrate_kbps }
                | Self::Aac { bitrate_kbps }
                | Self::OpusOgg { bitrate_kbps },
                SaveCodec::Mp3 { .. } | SaveCodec::Aac { .. } | SaveCodec::OpusOgg { .. },
            ) => {
                let accepted = format.bitrates_kbps().expect("a lossy format lists bitrates");
                let nearest = *accepted
                    .iter()
                    .min_by_key(|accepted| accepted.abs_diff(*bitrate_kbps))
                    .expect("a lossy format lists at least one bitrate");
                start.with_bitrate_kbps(nearest)
            }
            _ => start,
        }
    }

    /// This lossless codec at `bit_depth`; a lossy codec has none to set.
    fn with_bit_depth(self, bit_depth: SaveBitDepth) -> SaveCodec {
        match self {
            Self::Flac { .. } => Self::Flac { bit_depth },
            Self::Wav { .. } => Self::Wav { bit_depth },
            Self::Aiff { .. } => Self::Aiff { bit_depth },
            Self::Mp3 { .. } | Self::Aac { .. } | Self::OpusOgg { .. } => self,
        }
    }

    /// This lossy codec at `bitrate_kbps`; a lossless codec has none to set.
    fn with_bitrate_kbps(self, bitrate_kbps: u32) -> SaveCodec {
        match self {
            Self::Mp3 { .. } => Self::Mp3 { bitrate_kbps },
            Self::Aac { .. } => Self::Aac { bitrate_kbps },
            Self::OpusOgg { .. } => Self::OpusOgg { bitrate_kbps },
            Self::Flac { .. } | Self::Wav { .. } | Self::Aiff { .. } => self,
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
    pub pregap_placement: SavePregapPlacement,
    /// Whether saved files embed the release's cover art. When false, the cover
    /// file is never read — no wasted download/decrypt for a preset that won't
    /// embed it.
    pub embed_cover: bool,
}

impl SavePreset {
    /// The preset a person adds to their list: FLAC at its starting codec,
    /// named for the format, pregaps appended
    /// to the track before except the hidden first-track audio, embedding the
    /// cover, under an id from `ids`.
    pub fn new_flac(ids: &dyn coven::IdSource) -> Self {
        let format = SaveFormat::Flac;
        SavePreset {
            id: ids.new_id().simple().to_string(),
            name: format.name().to_string(),
            codec: format.default_codec(),
            pregap_placement: default_export_pregap_placement(),
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
            let accepted = format.bitrates_kbps().expect("a lossy format lists its bitrates");
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
            pregap_placement: default_export_pregap_placement(),
            embed_cover: true,
        },
        SavePreset {
            id: "mp3".to_string(),
            name: "MP3".to_string(),
            codec: SaveFormat::Mp3.default_codec(),
            pregap_placement: default_export_pregap_placement(),
            embed_cover: true,
        },
    ]
}

#[cfg(test)]
#[path = "save_tests.rs"]
mod tests;
