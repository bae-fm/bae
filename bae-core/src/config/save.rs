use coven::ConfigError;
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

/// The default filename pattern a new preset starts with: the zero-padded track
/// number, then the title. Only [`default_save_presets`] uses it now — the
/// global filename-token config is gone.
fn default_save_filename_tokens() -> Vec<SaveFilenameToken> {
    vec![SaveFilenameToken::TrackNumber, SaveFilenameToken::Title]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SaveBitDepth {
    Source,
    Bits16,
    Bits24,
    Bits32,
}

impl SaveBitDepth {
    pub fn resolve(self, source_bits: Option<i64>) -> u32 {
        match self {
            Self::Source => source_bits
                .and_then(|bits| u32::try_from(bits).ok())
                .filter(|bits| (1..=32).contains(bits))
                .unwrap_or(32),
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

impl SaveCodec {
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
    /// blob is never read — no wasted download/decrypt for a preset that won't
    /// embed it.
    pub embed_cover: bool,
}

impl SavePreset {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.id.trim().is_empty() {
            return Err(ConfigError::Config("export preset id is empty".to_string()));
        }
        if self.name.trim().is_empty() {
            return Err(ConfigError::Config(format!(
                "export preset {} has an empty name",
                self.id
            )));
        }
        if !self.applies_to_track && !self.applies_to_release {
            return Err(ConfigError::Config(format!(
                "export preset {} does not apply to any export level",
                self.id
            )));
        }
        if self.pregap_placement == SavePregapPlacement::SingleFileWithCue && self.applies_to_track
        {
            return Err(ConfigError::Config(format!(
                "export preset {} uses single-file CUE and cannot apply to track export",
                self.id
            )));
        }
        if self.pregap_placement == SavePregapPlacement::SingleFileWithCue
            && !self.codec.supports_single_file_cue()
        {
            return Err(ConfigError::Config(format!(
                "export preset {} uses single-file CUE with an unsupported codec",
                self.id
            )));
        }
        match self.codec {
            SaveCodec::Mp3 { bitrate_kbps } => {
                if !(32..=320).contains(&bitrate_kbps) {
                    return Err(ConfigError::Config(format!(
                        "export preset {} has unsupported MP3 bitrate {}",
                        self.id, bitrate_kbps
                    )));
                }
            }
            SaveCodec::Aac { bitrate_kbps } => {
                if !(32..=512).contains(&bitrate_kbps) {
                    return Err(ConfigError::Config(format!(
                        "export preset {} has unsupported AAC bitrate {}",
                        self.id, bitrate_kbps
                    )));
                }
            }
            SaveCodec::OpusOgg { bitrate_kbps } => {
                if !(32..=512).contains(&bitrate_kbps) {
                    return Err(ConfigError::Config(format!(
                        "export preset {} has unsupported Opus bitrate {}",
                        self.id, bitrate_kbps
                    )));
                }
            }
            SaveCodec::Flac { .. } | SaveCodec::Wav { .. } | SaveCodec::Aiff { .. } => {}
        }
        Ok(())
    }
}

pub(super) fn default_save_presets() -> Vec<SavePreset> {
    vec![
        SavePreset {
            id: "flac".to_string(),
            name: "FLAC".to_string(),
            codec: SaveCodec::Flac {
                bit_depth: SaveBitDepth::Source,
            },
            filename_tokens: default_save_filename_tokens(),
            pregap_placement: default_export_pregap_placement(),
            applies_to_track: true,
            applies_to_release: true,
            embed_cover: true,
        },
        SavePreset {
            id: "mp3".to_string(),
            name: "MP3".to_string(),
            codec: SaveCodec::Mp3 { bitrate_kbps: 320 },
            filename_tokens: default_save_filename_tokens(),
            pregap_placement: default_export_pregap_placement(),
            applies_to_track: true,
            applies_to_release: true,
            embed_cover: true,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_file_cue_preset_is_release_only() {
        let preset = SavePreset {
            id: "flac-image".to_string(),
            name: "FLAC image".to_string(),
            codec: SaveCodec::Flac {
                bit_depth: SaveBitDepth::Source,
            },
            filename_tokens: default_save_filename_tokens(),
            pregap_placement: SavePregapPlacement::SingleFileWithCue,
            applies_to_track: true,
            applies_to_release: true,
            embed_cover: true,
        };

        let err = preset
            .validate()
            .expect_err("single-file CUE cannot be a track export preset");
        assert!(err.to_string().contains("cannot apply to track export"));

        let release_only = SavePreset {
            applies_to_track: false,
            ..preset
        };
        release_only.validate().unwrap();

        let opus_image = SavePreset {
            id: "opus-image".to_string(),
            name: "Opus image".to_string(),
            codec: SaveCodec::OpusOgg { bitrate_kbps: 192 },
            filename_tokens: default_save_filename_tokens(),
            pregap_placement: SavePregapPlacement::SingleFileWithCue,
            applies_to_track: false,
            applies_to_release: true,
            embed_cover: true,
        };
        let err = opus_image
            .validate()
            .expect_err("single-file CUE requires a CUE-compatible codec");
        assert!(err.to_string().contains("unsupported codec"));
    }

    #[test]
    fn aac_bitrate_is_validated_against_its_bounds() {
        let preset = |bitrate_kbps: u32| SavePreset {
            id: "aac".to_string(),
            name: "AAC".to_string(),
            codec: SaveCodec::Aac { bitrate_kbps },
            filename_tokens: default_save_filename_tokens(),
            pregap_placement: default_export_pregap_placement(),
            applies_to_track: true,
            applies_to_release: true,
            embed_cover: true,
        };

        preset(32).validate().unwrap();
        preset(256).validate().unwrap();
        preset(512).validate().unwrap();

        let err = preset(31)
            .validate()
            .expect_err("a bitrate below the minimum must be rejected");
        assert!(err.to_string().contains("unsupported AAC bitrate"));
        let err = preset(513)
            .validate()
            .expect_err("a bitrate above the maximum must be rejected");
        assert!(err.to_string().contains("unsupported AAC bitrate"));
    }
}
