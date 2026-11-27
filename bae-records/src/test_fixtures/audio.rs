//! Audio a test gives a track, as an import stores it.

const FLAC: &str = "audio/flac";

/// Give `track_id` an audio format whose segments are, in order, a pregap
/// when `pregap`, then its body, each a whole new FLAC file of `release_id`.
/// Returns the files' ids and the segments' ids, in segment order.
pub async fn store_audio(
    records: &crate::Records,
    temp: &bae_util::test_files::TestFolder,
    release_id: &str,
    track_id: &str,
    pregap: bool,
) -> (Vec<String>, Vec<String>) {
    let now = time::UtcDateTime::now();
    let roles: &[crate::DbAudioSegmentRole] = if pregap {
        &[crate::DbAudioSegmentRole::AudioPregap, crate::DbAudioSegmentRole::Main]
    } else {
        &[crate::DbAudioSegmentRole::Main]
    };
    let format = crate::DbAudioFormat {
        id: bae_util::test_ids::test_uuid(&format!("format-{track_id}")),
        track_id: track_id.to_string(),
        content_type: bae_util::content_type::ContentType::from_mime(FLAC),
        pregap_samples: pregap.then_some(88_200),
        generated_pregap_samples: None,
        sample_rate: std::num::NonZeroU32::new(44_100).unwrap(),
        bits_per_sample: std::num::NonZeroU32::new(16),
        channels: std::num::NonZeroU32::new(2).unwrap(),
        track_loudness_lufs: Some(-9.5),
        track_peak_linear: Some(0.5),
        created_at: now,
    };
    let mut files = Vec::new();
    let mut segments = Vec::new();
    for (index, role) in roles.iter().enumerate() {
        let name = format!("{track_id}-{index}.flac");
        let original = temp.write(&name, vec![0u8; 100 + index]);
        let mut file = crate::NewReleaseFile::new(
            release_id,
            &name,
            bae_util::content_type::ContentType::from_mime(FLAC),
            crate::ReleaseFileRole::Audio,
            bae_util::test_ids::test_uuid(&name),
            now,
        );
        file.source_audio = Some(crate::source_audio::SourceAudioFile {
            layout: Some(crate::source_audio::SourceAudioLayout::File),
            content_type: bae_util::content_type::ContentType::from_mime(FLAC),
            container: bae_util::audio_container::AudioContainer::Flac,
            duration_ms: 1_000,
            format: bae_types::audio_format::AudioFormat {
                codec: "FLAC".to_string(),
                sample_rate_hz: 44_100,
                bits_per_sample: Some(16),
                bitrate_kbps: None,
                channels: 2,
            },
        });
        records.insert_file_for_test(&file, &original).await.unwrap();
        segments.push(crate::DbAudioSegment {
            id: bae_util::test_ids::test_uuid(&format!("segment-{name}")),
            audio_format_id: format.id.clone(),
            segment_index: i64::try_from(index).unwrap(),
            role: role.clone(),
            file_id: file.id.clone(),
            start_sample: 0,
            end_sample: None,
            start_byte: None,
            end_byte: None,
            created_at: now,
        });
        files.push(file.id);
    }
    records.insert_audio_format_with_segments_for_test(&format, &segments).await.unwrap();
    (files, segments.into_iter().map(|segment| segment.id).collect())
}
