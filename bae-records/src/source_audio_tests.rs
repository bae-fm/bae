use super::*;

/// One audio file as the summary statement selects it.
struct File {
    layout: Option<&'static str>,
    content_type: Option<&'static str>,
    sample_rate_hz: i64,
    bits_per_sample: Option<i64>,
    bitrate_kbps: Option<i64>,
    duration_ms: i64,
}

fn flac(sample_rate_hz: i64, bits: i64) -> File {
    File {
        layout: Some("file"),
        content_type: Some("audio/flac"),
        sample_rate_hz,
        bits_per_sample: Some(bits),
        bitrate_kbps: None,
        duration_ms: 200_000,
    }
}

fn aac(kbps: i64, duration_ms: i64) -> File {
    File {
        layout: Some("file"),
        content_type: Some("audio/aac"),
        sample_rate_hz: 44_100,
        bits_per_sample: None,
        bitrate_kbps: Some(kbps),
        duration_ms,
    }
}

/// The summary that [`source_audio_summary_sql`] computes over `files`, in
/// the order given.
fn summary(files: &[File]) -> Option<SourceAudioSummary> {
    #[derive(serde::Deserialize)]
    struct Column {
        #[serde(deserialize_with = "SourceAudioSummary::deserialize_stored")]
        summary: Option<SourceAudioSummary>,
    }
    let conn = rusqlite::Connection::open_in_memory().expect("an in-memory database opens");
    crate::test_fixtures::TEST_SCHEMA.open_bare_library(&conn);
    let rows: Vec<serde_json::Value> = files
        .iter()
        .enumerate()
        .map(|(position, file)| {
            serde_json::json!({
                "position": position,
                "layout": file.layout,
                "content_type": file.content_type,
                "sample_rate_hz": file.sample_rate_hz,
                "bits_per_sample": file.bits_per_sample,
                "bitrate_kbps": file.bitrate_kbps,
                "channels": 2,
                "duration_ms": file.duration_ms,
            })
        })
        .collect();
    let audio = "SELECT value ->> 'position' AS position, value ->> 'layout' AS layout, \
                        value ->> 'content_type' AS content_type, \
                        value ->> 'sample_rate_hz' AS sample_rate_hz, \
                        value ->> 'bits_per_sample' AS bits_per_sample, \
                        value ->> 'bitrate_kbps' AS bitrate_kbps, \
                        value ->> 'channels' AS channels, value ->> 'duration_ms' AS duration_ms \
                 FROM json_each(:files)";
    let json: String = conn
        .query_row(
            &format!("SELECT json_object('summary', {})", source_audio_summary_sql(audio)),
            rusqlite::named_params! { ":files": serde_json::Value::Array(rows).to_string() },
            |row| row.get(0),
        )
        .expect("the summary statement runs");
    serde_json::from_str::<Column>(&json).expect("the summary reads").summary
}

#[test]
fn repeated_format_is_uniform() {
    assert_eq!(
        summary(&[flac(44_100, 16), flac(44_100, 16)]),
        Some(SourceAudioSummary::Uniform {
            descriptor: SourceAudioDescriptor {
                layout: SourceAudioLayout::File,
                format: AudioFormat {
                    codec: "FLAC".to_string(),
                    sample_rate_hz: 44_100,
                    bits_per_sample: Some(16),
                    bitrate_kbps: None,
                    channels: 2,
                },
            },
        })
    );
}

/// A variable-bitrate encode states a different average for each file;
/// they are one format, and the release states their average weighted by
/// how long each plays.
#[test]
fn variable_bitrate_files_are_one_format_at_their_weighted_average() {
    let summary =
        summary(&[aac(270, 100_000), aac(275, 100_000), aac(288, 200_000), aac(287, 100_000)]);
    let Some(SourceAudioSummary::Uniform { descriptor }) = summary else {
        panic!("variable-bitrate files read as mixed: {summary:?}");
    };
    // (270 + 275 + 2 × 288 + 287) / 5 = 281.6
    assert_eq!(descriptor.format.bitrate_kbps, Some(282));
    assert_eq!(descriptor.format.codec, "AAC");
}

/// Files that state no duration count once each in the average bitrate.
#[test]
fn files_with_no_duration_count_equally() {
    let Some(SourceAudioSummary::Uniform { descriptor }) =
        summary(&[aac(256, 0), aac(320, 0), aac(320, 0)])
    else {
        panic!("one AAC format read as mixed");
    };
    // (256 + 320 + 320) / 3 = 298.67
    assert_eq!(descriptor.format.bitrate_kbps, Some(299));
}

/// Two content types of one codec are one format.
#[test]
fn the_codec_decides_the_format_not_the_content_type() {
    let mut ape = flac(44_100, 16);
    ape.content_type = Some("audio/ape");
    let mut x_ape = flac(44_100, 16);
    x_ape.content_type = Some("audio/x-ape");
    let Some(SourceAudioSummary::Uniform { descriptor }) = summary(&[ape, x_ape]) else {
        panic!("two spellings of APE read as mixed");
    };
    assert_eq!(descriptor.format.codec, "APE");
}

/// A file the tracklist leaves out, and a file the scan read no format of,
/// are not part of the summary.
#[test]
fn files_with_no_layout_or_no_format_are_left_out() {
    let mut left_out = aac(256, 100_000);
    left_out.layout = None;
    let mut unread = aac(256, 100_000);
    unread.content_type = None;
    assert_eq!(summary(&[left_out, flac(44_100, 16), unread]), summary(&[flac(44_100, 16)]));
    let mut only_left_out = flac(44_100, 16);
    only_left_out.layout = None;
    assert_eq!(summary(&[only_left_out]), None);
}

#[test]
fn lossless_files_state_no_bitrate() {
    let Some(SourceAudioSummary::Uniform { descriptor }) =
        summary(&[flac(44_100, 16), flac(44_100, 16)])
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
        summary(&[flac(44_100, 16), aac(256, 100_000), flac(44_100, 16)]),
        Some(SourceAudioSummary::Mixed {
            differences: vec![SourceAudioDifference::Codec {
                codecs: vec!["FLAC".to_string(), "AAC".to_string()],
            }],
        })
    );
    assert_eq!(
        summary(&[flac(44_100, 16), flac(96_000, 24)]),
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
    cue.layout = Some("cue");
    assert_eq!(
        summary(&[cue, flac(44_100, 16)]),
        Some(SourceAudioSummary::Mixed {
            differences: vec![SourceAudioDifference::Layout {
                layouts: vec![SourceAudioLayout::Cue, SourceAudioLayout::File],
            }],
        })
    );
}

#[test]
fn no_files_is_no_summary() {
    assert_eq!(summary(&[]), None);
}
