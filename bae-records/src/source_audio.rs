//! What a release's audio files are: each file's format as the scan read it,
//! and the one summary every surface shows for the release.
//!
//! Files share a format when their codec, sample rate, bit depth, channels and
//! layout match. Bitrate is not part of it: a variable-bitrate encode states a
//! different average for every file, and those files are one format. A shared
//! lossy format states the release's average bitrate instead.

use bae_types::audio_format::AudioFormat;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SourceAudioLayout {
    File,
    Cue,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SourceAudioDescriptor {
    pub layout: SourceAudioLayout,
    pub format: AudioFormat,
}

impl SourceAudioDescriptor {
    /// Whether `other` is the same format: everything but the bitrate.
    fn shares_format(&self, other: &Self) -> bool {
        let (a, b) = (&self.format, &other.format);
        self.layout == other.layout
            && a.codec == b.codec
            && a.sample_rate_hz == b.sample_rate_hz
            && a.bits_per_sample == b.bits_per_sample
            && a.channels == b.channels
    }
}

/// The scan facts persisted with one physical release file.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SourceAudioFile {
    /// How this file contributes tracks. `None` when the physical audio is
    /// carried with the release but excluded from its tracklist.
    pub layout: Option<SourceAudioLayout>,
    pub format: AudioFormat,
    pub content_type: bae_util::content_type::ContentType,
    /// The container the file's bytes are in, as the probe read it.
    pub container: bae_util::audio_container::AudioContainer,
    pub duration_ms: u64,
}

impl SourceAudioFile {
    pub fn descriptor(&self) -> Option<SourceAudioDescriptor> {
        Some(SourceAudioDescriptor { layout: self.layout?, format: self.format.clone() })
    }

    /// This file as one the release's summary is read from, when it
    /// contributes tracks.
    pub fn summarized(&self) -> Option<SummarizedAudio> {
        Some(SummarizedAudio { descriptor: self.descriptor()?, duration_ms: self.duration_ms })
    }
}

/// One file a release's summary is read from: its format and how long it
/// plays, which weighs its bitrate in the release's average.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummarizedAudio {
    pub descriptor: SourceAudioDescriptor,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SourceAudioSummary {
    /// Every file is one format. A lossy one's bitrate is the release's
    /// average: each file's own, weighted by how long it plays.
    Uniform { descriptor: SourceAudioDescriptor },
    /// The files are more than one format: each fact they disagree on, in the
    /// order a format's facts read. A fact they share is not named.
    Mixed { differences: Vec<SourceAudioDifference> },
}

/// One fact a release's files disagree on, and its values, each once, in the
/// order the files are read.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SourceAudioDifference {
    Layout {
        layouts: Vec<SourceAudioLayout>,
    },
    Codec {
        codecs: Vec<String>,
    },
    SampleRate {
        sample_rates_hz: Vec<i64>,
    },
    /// The lossless files' bit depths. A lossy file has none, and a codec
    /// states one for every file or for none, so lossy files beside lossless
    /// ones already differ by codec.
    BitDepth {
        bits_per_sample: Vec<i64>,
    },
    Channels {
        channels: Vec<i64>,
    },
}

impl SourceAudioSummary {
    pub fn from_files(files: impl IntoIterator<Item = SummarizedAudio>) -> Option<Self> {
        let files: Vec<SummarizedAudio> = files.into_iter().collect();
        let first = files.first()?;
        if files.iter().all(|file| file.descriptor.shares_format(&first.descriptor)) {
            let mut descriptor = first.descriptor.clone();
            descriptor.format.bitrate_kbps = average_bitrate_kbps(&files);
            return Some(Self::Uniform { descriptor });
        }
        let formats: Vec<&AudioFormat> = files.iter().map(|file| &file.descriptor.format).collect();
        let differences = [
            differing(files.iter().map(|file| Some(file.descriptor.layout)))
                .map(|layouts| SourceAudioDifference::Layout { layouts }),
            differing(formats.iter().map(|format| Some(format.codec.clone())))
                .map(|codecs| SourceAudioDifference::Codec { codecs }),
            differing(formats.iter().map(|format| Some(format.sample_rate_hz)))
                .map(|sample_rates_hz| SourceAudioDifference::SampleRate { sample_rates_hz }),
            differing(formats.iter().map(|format| format.bits_per_sample))
                .map(|bits_per_sample| SourceAudioDifference::BitDepth { bits_per_sample }),
            differing(formats.iter().map(|format| Some(format.channels)))
                .map(|channels| SourceAudioDifference::Channels { channels }),
        ]
        .into_iter()
        .flatten()
        .collect();
        Some(Self::Mixed { differences })
    }
}

/// The distinct present values, first seen first, when there are two or more.
fn differing<T: PartialEq>(values: impl Iterator<Item = Option<T>>) -> Option<Vec<T>> {
    let mut distinct = Vec::new();
    for value in values.flatten() {
        if !distinct.contains(&value) {
            distinct.push(value);
        }
    }
    (distinct.len() >= 2).then_some(distinct)
}

/// The files' bitrate, each file's weighted by how long it plays, or `None`
/// for lossless files, which state none. Files that report no length count
/// equally, since there is nothing to weigh them by.
fn average_bitrate_kbps(files: &[SummarizedAudio]) -> Option<i64> {
    let rated: Vec<(i64, u64)> = files
        .iter()
        .filter_map(|file| Some((file.descriptor.format.bitrate_kbps?, file.duration_ms)))
        .collect();
    if rated.is_empty() {
        return None;
    }
    let played: u64 = rated.iter().map(|(_, duration_ms)| duration_ms).sum();
    let (weighted, weight) = if played > 0 {
        (
            rated
                .iter()
                .map(|(kbps, duration_ms)| i128::from(*kbps) * i128::from(*duration_ms))
                .sum::<i128>(),
            i128::from(played),
        )
    } else {
        (rated.iter().map(|(kbps, _)| i128::from(*kbps)).sum(), rated.len() as i128)
    };
    Some(((weighted + weight / 2) / weight) as i64)
}

#[cfg(test)]
#[path = "source_audio_tests.rs"]
mod tests;
