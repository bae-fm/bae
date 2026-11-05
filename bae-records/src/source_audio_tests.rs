use super::*;

fn file(
    codec: &str,
    sample_rate_hz: i64,
    bits_per_sample: Option<i64>,
    bitrate_kbps: Option<i64>,
    duration_ms: u64,
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

fn aac(kbps: i64, duration_ms: u64) -> SummarizedAudio {
    file("AAC", 44_100, None, Some(kbps), duration_ms)
}

#[test]
fn repeated_format_is_uniform() {
    let one = flac(44_100, 16);
    assert_eq!(
        SourceAudioSummary::from_files([one.clone(), one.clone()]),
        Some(SourceAudioSummary::Uniform { descriptor: one.descriptor })
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
                SourceAudioDifference::SampleRate { sample_rates_hz: vec![44_100, 96_000] },
                SourceAudioDifference::BitDepth { bits_per_sample: vec![16, 24] },
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
