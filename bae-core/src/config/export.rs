use coven::ConfigError;
use serde::{Deserialize, Serialize};

/// One piece of an export filename pattern. A pattern is an ordered token list;
/// rendering substitutes each token's value from the track's metadata and joins
/// the non-empty values with single spaces (see `render_export_filename`). The
/// extension is added by the exporter from the chosen format, not the pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExportFilenameToken {
    Title,
    Artist,
    Album,
    Year,
    TrackNumber,
    DiscNumber,
    TrackTotal,
}

/// The default filename pattern a new preset starts with: the zero-padded track
/// number, then the title. Only [`default_export_presets`] uses it now — the
/// global filename-token config is gone.
fn default_export_filename_tokens() -> Vec<ExportFilenameToken> {
    vec![ExportFilenameToken::TrackNumber, ExportFilenameToken::Title]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExportBitDepth {
    Source,
    Bits16,
    Bits24,
    Bits32,
}

impl ExportBitDepth {
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
pub enum ExportPresetCodec {
    Flac { bit_depth: ExportBitDepth },
    Mp3 { bitrate_kbps: u32 },
    OpusOgg { bitrate_kbps: u32 },
    Wav { bit_depth: ExportBitDepth },
    Aiff { bit_depth: ExportBitDepth },
}

impl ExportPresetCodec {
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Flac { .. } => "flac",
            Self::Mp3 { .. } => "mp3",
            Self::OpusOgg { .. } => "ogg",
            Self::Wav { .. } => "wav",
            Self::Aiff { .. } => "aiff",
        }
    }

    pub fn supports_single_file_cue(&self) -> bool {
        match self {
            Self::OpusOgg { .. } => false,
            Self::Flac { .. } | Self::Mp3 { .. } | Self::Wav { .. } | Self::Aiff { .. } => true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExportPregapPlacement {
    AppendToPreviousExceptHtoa,
    AppendToPreviousIncludingHtoa,
    Exclude,
    SingleFileWithCue,
}

fn default_export_pregap_placement() -> ExportPregapPlacement {
    ExportPregapPlacement::AppendToPreviousExceptHtoa
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportPreset {
    pub id: String,
    pub name: String,
    pub codec: ExportPresetCodec,
    pub filename_tokens: Vec<ExportFilenameToken>,
    pub pregap_placement: ExportPregapPlacement,
    pub applies_to_track: bool,
    pub applies_to_release: bool,
}

impl ExportPreset {
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
        if self.pregap_placement == ExportPregapPlacement::SingleFileWithCue
            && self.applies_to_track
        {
            return Err(ConfigError::Config(format!(
                "export preset {} uses single-file CUE and cannot apply to track export",
                self.id
            )));
        }
        if self.pregap_placement == ExportPregapPlacement::SingleFileWithCue
            && !self.codec.supports_single_file_cue()
        {
            return Err(ConfigError::Config(format!(
                "export preset {} uses single-file CUE with an unsupported codec",
                self.id
            )));
        }
        match self.codec {
            ExportPresetCodec::Mp3 { bitrate_kbps } => {
                if !(32..=320).contains(&bitrate_kbps) {
                    return Err(ConfigError::Config(format!(
                        "export preset {} has unsupported MP3 bitrate {}",
                        self.id, bitrate_kbps
                    )));
                }
            }
            ExportPresetCodec::OpusOgg { bitrate_kbps } => {
                if !(32..=512).contains(&bitrate_kbps) {
                    return Err(ConfigError::Config(format!(
                        "export preset {} has unsupported Opus bitrate {}",
                        self.id, bitrate_kbps
                    )));
                }
            }
            ExportPresetCodec::Flac { .. }
            | ExportPresetCodec::Wav { .. }
            | ExportPresetCodec::Aiff { .. } => {}
        }
        Ok(())
    }
}

pub(super) fn default_export_presets() -> Vec<ExportPreset> {
    vec![
        ExportPreset {
            id: "flac".to_string(),
            name: "FLAC".to_string(),
            codec: ExportPresetCodec::Flac {
                bit_depth: ExportBitDepth::Source,
            },
            filename_tokens: default_export_filename_tokens(),
            pregap_placement: default_export_pregap_placement(),
            applies_to_track: true,
            applies_to_release: true,
        },
        ExportPreset {
            id: "mp3".to_string(),
            name: "MP3".to_string(),
            codec: ExportPresetCodec::Mp3 { bitrate_kbps: 320 },
            filename_tokens: default_export_filename_tokens(),
            pregap_placement: default_export_pregap_placement(),
            applies_to_track: true,
            applies_to_release: true,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_file_cue_preset_is_release_only() {
        let preset = ExportPreset {
            id: "flac-image".to_string(),
            name: "FLAC image".to_string(),
            codec: ExportPresetCodec::Flac {
                bit_depth: ExportBitDepth::Source,
            },
            filename_tokens: default_export_filename_tokens(),
            pregap_placement: ExportPregapPlacement::SingleFileWithCue,
            applies_to_track: true,
            applies_to_release: true,
        };

        let err = preset
            .validate()
            .expect_err("single-file CUE cannot be a track export preset");
        assert!(err.to_string().contains("cannot apply to track export"));

        let release_only = ExportPreset {
            applies_to_track: false,
            ..preset
        };
        release_only.validate().unwrap();

        let opus_image = ExportPreset {
            id: "opus-image".to_string(),
            name: "Opus image".to_string(),
            codec: ExportPresetCodec::OpusOgg { bitrate_kbps: 192 },
            filename_tokens: default_export_filename_tokens(),
            pregap_placement: ExportPregapPlacement::SingleFileWithCue,
            applies_to_track: false,
            applies_to_release: true,
        };
        let err = opus_image
            .validate()
            .expect_err("single-file CUE requires a CUE-compatible codec");
        assert!(err.to_string().contains("unsupported codec"));
    }
}
