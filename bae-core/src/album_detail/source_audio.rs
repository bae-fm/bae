//! What a release's audio files are: each file's format as the scan read it,
//! and the one summary every surface shows for the release.
//!
//! Files share a format when their codec, sample rate, bit depth, channels and
//! layout match. Bitrate is not part of it: a variable-bitrate encode states a
//! different average for every file, and those files are one format. A shared
//! lossy format states the release's average bitrate instead.

/// The parts the UI composes into a one-line label ("FLAC · 44.1 kHz · 16-bit ·
/// stereo"): the codec is a proper noun, the channel count maps to a localized word,
/// and the numbers format per locale. A present `bits_per_sample` means lossless —
/// show the bit depth; absent means lossy — show `bitrate_kbps`, an average,
/// instead.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AudioFormat {
    pub codec: String,
    pub sample_rate_hz: i64,
    pub bits_per_sample: Option<i64>,
    pub bitrate_kbps: Option<i64>,
    pub channels: i64,
}

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
    pub content_type: crate::util::content_type::ContentType,
    pub duration_ms: i64,
}

impl SourceAudioFile {
    pub fn descriptor(&self) -> Option<SourceAudioDescriptor> {
        Some(SourceAudioDescriptor {
            layout: self.layout?,
            format: self.format.clone(),
        })
    }

    /// This file as one the release's summary is read from, when it
    /// contributes tracks.
    pub fn summarized(&self) -> Option<SummarizedAudio> {
        Some(SummarizedAudio {
            descriptor: self.descriptor()?,
            duration_ms: self.duration_ms,
        })
    }
}

/// One file a release's summary is read from: its format and how long it
/// plays, which weighs its bitrate in the release's average.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummarizedAudio {
    pub descriptor: SourceAudioDescriptor,
    pub duration_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SourceAudioSummary {
    /// Every file is one format. A lossy one's bitrate is the release's
    /// average: each file's own, weighted by how long it plays.
    Uniform { descriptor: SourceAudioDescriptor },
    /// The files are more than one format: each fact they disagree on, in the
    /// order a format's facts read. A fact they share is not named.
    Mixed {
        differences: Vec<SourceAudioDifference>,
    },
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
        if files
            .iter()
            .all(|file| file.descriptor.shares_format(&first.descriptor))
        {
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
    let rated: Vec<(i64, i64)> = files
        .iter()
        .filter_map(|file| Some((file.descriptor.format.bitrate_kbps?, file.duration_ms)))
        .collect();
    if rated.is_empty() {
        return None;
    }
    let played: i64 = rated.iter().map(|(_, duration_ms)| duration_ms).sum();
    let (weighted, weight) = if played > 0 {
        (
            rated
                .iter()
                .map(|(kbps, duration_ms)| i128::from(*kbps) * i128::from(*duration_ms))
                .sum::<i128>(),
            i128::from(played),
        )
    } else {
        (
            rated.iter().map(|(kbps, _)| i128::from(*kbps)).sum(),
            rated.len() as i128,
        )
    };
    Some(((weighted + weight / 2) / weight) as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(
        codec: &str,
        sample_rate_hz: i64,
        bits_per_sample: Option<i64>,
        bitrate_kbps: Option<i64>,
        duration_ms: i64,
    ) -> SummarizedAudio {
        SummarizedAudio {
            descriptor: SourceAudioDescriptor {
                layout: SourceAudioLayout::File,
                format: AudioFormat {
                    codec: codec.to_string(),
                    sample_rate_hz,
                    bits_per_sample,
                    bitrate_kbps,
                    channels: 2,
                },
            },
            duration_ms,
        }
    }

    fn flac(sample_rate_hz: i64, bits: i64) -> SummarizedAudio {
        file("FLAC", sample_rate_hz, Some(bits), None, 200_000)
    }

    fn aac(kbps: i64, duration_ms: i64) -> SummarizedAudio {
        file("AAC", 44_100, None, Some(kbps), duration_ms)
    }

    #[test]
    fn repeated_format_is_uniform() {
        let one = flac(44_100, 16);
        assert_eq!(
            SourceAudioSummary::from_files([one.clone(), one.clone()]),
            Some(SourceAudioSummary::Uniform {
                descriptor: one.descriptor
            })
        );
    }

    /// A variable-bitrate encode states a different average for each file;
    /// they are one format, and the release states their average weighted by
    /// how long each plays.
    #[test]
    fn variable_bitrate_files_are_one_format_at_their_weighted_average() {
        let summary = SourceAudioSummary::from_files([
            aac(270, 100_000),
            aac(275, 100_000),
            aac(288, 200_000),
            aac(287, 100_000),
        ]);
        let Some(SourceAudioSummary::Uniform { descriptor }) = summary else {
            panic!("variable-bitrate files read as mixed: {summary:?}");
        };
        // (270 + 275 + 2 × 288 + 287) / 5 = 281.6
        assert_eq!(descriptor.format.bitrate_kbps, Some(282));
        assert_eq!(descriptor.format.codec, "AAC");
    }

    #[test]
    fn lossless_files_state_no_bitrate() {
        let Some(SourceAudioSummary::Uniform { descriptor }) =
            SourceAudioSummary::from_files([flac(44_100, 16), flac(44_100, 16)])
        else {
            panic!("one FLAC format read as mixed");
        };
        assert_eq!(descriptor.format.bitrate_kbps, None);
    }

    /// A mixed release names only what differs. A lossy file beside lossless
    /// ones differs by codec; its missing bit depth is not a second difference.
    #[test]
    fn a_mixed_release_names_what_differs() {
        assert_eq!(
            SourceAudioSummary::from_files([flac(44_100, 16), aac(256, 100_000), flac(44_100, 16)]),
            Some(SourceAudioSummary::Mixed {
                differences: vec![SourceAudioDifference::Codec {
                    codecs: vec!["FLAC".to_string(), "AAC".to_string()],
                }],
            })
        );
        assert_eq!(
            SourceAudioSummary::from_files([flac(44_100, 16), flac(96_000, 24)]),
            Some(SourceAudioSummary::Mixed {
                differences: vec![
                    SourceAudioDifference::SampleRate {
                        sample_rates_hz: vec![44_100, 96_000],
                    },
                    SourceAudioDifference::BitDepth {
                        bits_per_sample: vec![16, 24],
                    },
                ],
            })
        );
    }

    #[test]
    fn a_layout_is_part_of_the_format() {
        let mut cue = flac(44_100, 16);
        cue.descriptor.layout = SourceAudioLayout::Cue;
        assert_eq!(
            SourceAudioSummary::from_files([cue, flac(44_100, 16)]),
            Some(SourceAudioSummary::Mixed {
                differences: vec![SourceAudioDifference::Layout {
                    layouts: vec![SourceAudioLayout::Cue, SourceAudioLayout::File],
                }],
            })
        );
    }

    #[test]
    fn no_files_is_no_summary() {
        assert_eq!(SourceAudioSummary::from_files([]), None);
    }
}
