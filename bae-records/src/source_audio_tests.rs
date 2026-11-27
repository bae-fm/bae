use super::*;

/// One audio file as the summary statement selects it.
struct File {
    layout: Option<SourceAudioLayout>,
    content_type: Option<&'static str>,
    sample_rate_hz: i64,
    bits_per_sample: Option<i64>,
    bitrate_kbps: Option<i64>,
    duration_ms: i64,
}

fn flac(sample_rate_hz: i64, bits: i64) -> File {
    File {
        layout: Some(SourceAudioLayout::File),
        content_type: Some("audio/flac"),
        sample_rate_hz,
        bits_per_sample: Some(bits),
        bitrate_kbps: None,
        duration_ms: 200_000,
    }
}

fn aac(kbps: i64, duration_ms: i64) -> File {
    File {
        layout: Some(SourceAudioLayout::File),
        content_type: Some("audio/aac"),
        sample_rate_hz: 44_100,
        bits_per_sample: None,
        bitrate_kbps: Some(kbps),
        duration_ms,
    }
}

/// The summary a release's detail states for `files`, written in the order
/// given: a file with no content type is one the scan read no facts of.
async fn summary(files: &[File]) -> Option<SourceAudioSummary> {
    use crate::test_fixtures::aggregate_ordering::{insert_file, release_detail_db};
    use crate::test_fixtures::RELEASE_A;
    let (db, tmp) = release_detail_db(crate::test_fixtures::TEST_SCHEMA).await;
    for (position, file) in files.iter().enumerate() {
        let source_audio = file.content_type.map(|content_type| {
            let content_type = ContentType::from_mime(content_type);
            SourceAudioFile {
                layout: file.layout,
                format: AudioFormat {
                    codec: content_type.display_name().to_string(),
                    sample_rate_hz: file.sample_rate_hz,
                    bits_per_sample: file.bits_per_sample,
                    bitrate_kbps: file.bitrate_kbps,
                    channels: 2,
                },
                content_type,
                container: bae_util::audio_container::AudioContainer::Flac,
                duration_ms: u64::try_from(file.duration_ms).expect("a length"),
            }
        });
        let role = match source_audio {
            Some(_) => crate::ReleaseFileRole::Audio,
            None => crate::ReleaseFileRole::Other,
        };
        insert_file(
            &db,
            &tmp,
            crate::NewReleaseFile {
                id: bae_util::test_ids::test_uuid(&format!("summed-file-{position}")),
                release_id: RELEASE_A.to_string(),
                original_filename: format!("file {position}"),
                content_type: ContentType::from_mime("audio/flac"),
                role,
                source_audio,
                created_at: time::UtcDateTime::now(),
            },
            1,
        )
        .await;
    }
    db.read(
        |sql| Ok(crate::records::release_editor_rows_on(&sql, RELEASE_A)?.expect("the release")),
    )
    .await
    .unwrap()
    .decoded()
    .unwrap()
    .detail
    .files
    .source_audio
}

#[tokio::test]
async fn repeated_format_is_uniform() {
    assert_eq!(
        summary(&[flac(44_100, 16), flac(44_100, 16)]).await,
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
#[tokio::test]
async fn variable_bitrate_files_are_one_format_at_their_weighted_average() {
    let summary =
        summary(&[aac(270, 100_000), aac(275, 100_000), aac(288, 200_000), aac(287, 100_000)])
            .await;
    let Some(SourceAudioSummary::Uniform { descriptor }) = summary else {
        panic!("variable-bitrate files read as mixed: {summary:?}");
    };
    // (270 + 275 + 2 × 288 + 287) / 5 = 281.6
    assert_eq!(descriptor.format.bitrate_kbps, Some(282));
    assert_eq!(descriptor.format.codec, "AAC");
}

/// Files that state no duration count once each in the average bitrate.
#[tokio::test]
async fn files_with_no_duration_count_equally() {
    let Some(SourceAudioSummary::Uniform { descriptor }) =
        summary(&[aac(256, 0), aac(320, 0), aac(320, 0)]).await
    else {
        panic!("one AAC format read as mixed");
    };
    // (256 + 320 + 320) / 3 = 298.67
    assert_eq!(descriptor.format.bitrate_kbps, Some(299));
}

/// Two content types of one codec are one format.
#[tokio::test]
async fn the_codec_decides_the_format_not_the_content_type() {
    let mut ape = flac(44_100, 16);
    ape.content_type = Some("audio/ape");
    let mut x_ape = flac(44_100, 16);
    x_ape.content_type = Some("audio/x-ape");
    let Some(SourceAudioSummary::Uniform { descriptor }) = summary(&[ape, x_ape]).await else {
        panic!("two spellings of APE read as mixed");
    };
    assert_eq!(descriptor.format.codec, "APE");
}

/// A file the tracklist leaves out, and a file the scan read no format of,
/// are not part of the summary.
#[tokio::test]
async fn files_with_no_layout_or_no_format_are_left_out() {
    let mut left_out = aac(256, 100_000);
    left_out.layout = None;
    let mut unread = aac(256, 100_000);
    unread.content_type = None;
    assert_eq!(
        summary(&[left_out, flac(44_100, 16), unread]).await,
        summary(&[flac(44_100, 16)]).await
    );
    let mut only_left_out = flac(44_100, 16);
    only_left_out.layout = None;
    assert_eq!(summary(&[only_left_out]).await, None);
}

#[tokio::test]
async fn lossless_files_state_no_bitrate() {
    let Some(SourceAudioSummary::Uniform { descriptor }) =
        summary(&[flac(44_100, 16), flac(44_100, 16)]).await
    else {
        panic!("one FLAC format read as mixed");
    };
    assert_eq!(descriptor.format.bitrate_kbps, None);
}

/// A mixed release names only what differs. A lossy file beside lossless
/// ones differs by codec; its missing bit depth is not a second difference.
#[tokio::test]
async fn a_mixed_release_names_what_differs() {
    assert_eq!(
        summary(&[flac(44_100, 16), aac(256, 100_000), flac(44_100, 16)]).await,
        Some(SourceAudioSummary::Mixed {
            differences: vec![SourceAudioDifference::Codec {
                codecs: vec!["FLAC".to_string(), "AAC".to_string()],
            }],
        })
    );
    assert_eq!(
        summary(&[flac(44_100, 16), flac(96_000, 24)]).await,
        Some(SourceAudioSummary::Mixed {
            differences: vec![
                SourceAudioDifference::SampleRate { sample_rates_hz: vec![44_100, 96_000] },
                SourceAudioDifference::BitDepth { bits_per_sample: vec![16, 24] },
            ],
        })
    );
}

#[tokio::test]
async fn a_layout_is_part_of_the_format() {
    let mut cue = flac(44_100, 16);
    cue.layout = Some(SourceAudioLayout::Cue);
    assert_eq!(
        summary(&[cue, flac(44_100, 16)]).await,
        Some(SourceAudioSummary::Mixed {
            differences: vec![SourceAudioDifference::Layout {
                layouts: vec![SourceAudioLayout::Cue, SourceAudioLayout::File],
            }],
        })
    );
}

#[tokio::test]
async fn no_files_is_no_summary() {
    assert_eq!(summary(&[]).await, None);
}
