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

/// One piece of the folder name pattern a release save names its folder by.
/// Rendered like a filename pattern: each token's value, the non-empty values
/// joined with single spaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SaveFolderToken {
    Artist,
    Album,
    Year,
    Label,
    CatalogNumber,
}

/// The folder name pattern the schema starts a library with: the artist, then
/// the album.
#[cfg(any(test, feature = "test-utils"))]
pub(super) fn default_save_folder_tokens() -> Vec<SaveFolderToken> {
    vec![SaveFolderToken::Artist, SaveFolderToken::Album]
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

    /// The name a new preset of this format starts with: the format's name,
    /// or with the first free number from 2 ("FLAC (2)") when `taken` has it.
    pub fn free_name(self, taken: &[&str]) -> String {
        let name = self.name();
        if !taken.contains(&name) {
            return name.to_string();
        }
        // Some candidate among the first `taken.len() + 1` numbers is free, so
        // the loop ends, and `number` never passes `taken.len() + 2`.
        let mut number = 2usize;
        loop {
            let candidate = format!("{name} ({number})");
            if !taken.contains(&candidate.as_str()) {
                return candidate;
            }
            number += 1;
        }
    }

    /// Whether `name` is what `free_name` makes for this format: its name,
    /// alone or with a number from 2.
    fn is_prefill(self, name: &str) -> bool {
        let format_name = self.name();
        name == format_name
            || name
                .strip_prefix(format_name)
                .and_then(|rest| rest.strip_prefix(" ("))
                .and_then(|rest| rest.strip_suffix(')'))
                .is_some_and(|number| {
                    number.chars().all(|digit| digit.is_ascii_digit())
                        && number.parse::<u32>().is_ok_and(|number| number >= 2)
                })
    }

    /// The name of a preset whose format changes from `from` to `to`: a name
    /// still `from`'s prefill becomes `to`'s, free among `taken`; a name the
    /// person wrote stays.
    pub fn name_after_change(name: &str, from: Self, to: Self, taken: &[&str]) -> String {
        if from.is_prefill(name) {
            to.free_name(taken)
        } else {
            name.to_string()
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavePreset {
    pub id: String,
    pub name: String,
    pub codec: SaveCodec,
}

/// The settings every save format shares, all on to start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveExtras {
    /// Whether each saved file embeds the release's cover art. When false,
    /// the cover is never read for embedding.
    pub embed_cover: bool,
    /// Whether a release save writes the release's cover as `cover.jpg` in its
    /// folder.
    pub cover_file: bool,
    /// Whether each saved file carries ReplayGain tags from the loudness
    /// measured at import.
    pub loudness_tags: bool,
}

impl Default for SaveExtras {
    fn default() -> Self {
        Self { embed_cover: true, cover_file: true, loudness_tags: true }
    }
}

impl SavePreset {
    /// The preset a person adds to their list: FLAC at its starting codec,
    /// named for the format with the next free number (`SaveFormat::free_name`),
    /// embedding the cover, under an id from `ids`.
    pub fn new_flac(ids: &dyn coven::IdSource, existing: &[SavePreset]) -> Self {
        let format = SaveFormat::Flac;
        let taken: Vec<&str> = existing.iter().map(|preset| preset.name.as_str()).collect();
        SavePreset {
            id: ids.new_id().simple().to_string(),
            name: format.free_name(&taken),
            codec: format.default_codec(),
        }
    }

    /// Whether a format other than `own_id` is already named `name`.
    pub fn name_taken(name: &str, own_id: &str, presets: &[SavePreset]) -> bool {
        presets.iter().any(|preset| preset.id != own_id && preset.name == name)
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
        },
        SavePreset {
            id: "mp3".to_string(),
            name: "MP3".to_string(),
            codec: SaveFormat::Mp3.default_codec(),
        },
    ]
}

#[cfg(test)]
#[path = "save_tests.rs"]
mod tests;
