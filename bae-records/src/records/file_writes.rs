//! The rows of a release's files and of the audio tracks play from them.

use super::*;
use bae_store::WriteStatements as _;
use bae_util::timestamp::StoredTimestamp as _;

coven::write!(
    InsertReleaseFile(
        id: &'p str,
        release_id: &'p str,
        original_filename: &'p str,
        content_type: &'p str,
        file: coven::FileId,
        role: &'static str,
        document_kind: Option<&'static str>,
        source_audio_layout: Option<&'static str>,
        source_audio_content_type: Option<&'p str>,
        source_audio_container: Option<&'p str>,
        source_audio_duration_ms: Option<u64>,
        source_audio_sample_rate_hz: Option<i64>,
        source_audio_bits_per_sample: Option<i64>,
        source_audio_bitrate_kbps: Option<i64>,
        source_audio_channels: Option<i64>,
        created_at: String,
    ),
    r#"
    INSERT INTO release_files (
        id, release_id, original_filename, content_type, file, role, document_kind,
        source_audio_layout, source_audio_content_type, source_audio_container,
        source_audio_duration_ms, source_audio_sample_rate_hz, source_audio_bits_per_sample,
        source_audio_bitrate_kbps, source_audio_channels,
        created_at
    ) VALUES (
        :id, :release_id, :original_filename, :content_type, :file, :role, :document_kind,
        :source_audio_layout, :source_audio_content_type, :source_audio_container,
        :source_audio_duration_ms, :source_audio_sample_rate_hz, :source_audio_bits_per_sample,
        :source_audio_bitrate_kbps, :source_audio_channels,
        :created_at
    )
"#
);

/// Insert a release file row referencing `coven_file`, the coven file the
/// write staged for it.
pub fn insert_release_file_row(
    conn: &WriteContext<'_>,
    file: &NewReleaseFile,
    coven_file: coven::FileId,
) -> Result<(), IdentityWriteError> {
    let audio = file.source_audio.as_ref();
    let source_audio_layout = audio.and_then(|audio| audio.layout).map(|layout| match layout {
        crate::source_audio::SourceAudioLayout::File => "file",
        crate::source_audio::SourceAudioLayout::Cue => "cue",
    });
    conn.run(&InsertReleaseFileParams {
        id: &file.id,
        release_id: &file.release_id,
        original_filename: &file.original_filename,
        content_type: file.content_type.as_str(),
        file: coven_file,
        role: file.role.as_str(),
        document_kind: file.role.document_kind().map(crate::DocumentKind::as_str),
        source_audio_layout,
        source_audio_content_type: audio.map(|audio| audio.content_type.as_str()),
        source_audio_container: audio.map(|audio| audio.container.as_str()),
        source_audio_duration_ms: audio.map(|audio| audio.duration_ms),
        source_audio_sample_rate_hz: audio.map(|audio| audio.format.sample_rate_hz),
        source_audio_bits_per_sample: audio.and_then(|audio| audio.format.bits_per_sample),
        source_audio_bitrate_kbps: audio.and_then(|audio| audio.format.bitrate_kbps),
        source_audio_channels: audio.map(|audio| audio.format.channels),
        created_at: file.created_at.to_rfc3339(),
    })?;
    Ok(())
}

coven::write!(
    InsertAudioFormat(
        id: &'p str,
        track_id: &'p str,
        content_type: &'p str,
        pregap_samples: Option<i64>,
        generated_pregap_samples: Option<i64>,
        sample_rate: u32,
        bits_per_sample: Option<u32>,
        channels: u32,
        track_loudness_lufs: Option<f64>,
        track_peak_linear: Option<f64>,
        created_at: String,
    ),
    r#"
    INSERT INTO audio_formats (
        id, track_id, content_type, pregap_samples, generated_pregap_samples, sample_rate,
        bits_per_sample, channels, track_loudness_lufs, track_peak_linear, created_at
    ) VALUES (
        :id, :track_id, :content_type, :pregap_samples, :generated_pregap_samples, :sample_rate,
        :bits_per_sample, :channels, :track_loudness_lufs, :track_peak_linear, :created_at
    )
"#
);

pub fn insert_audio_format_row(conn: &WriteContext<'_>, af: &DbAudioFormat) -> Result<(), DbError> {
    conn.run(&InsertAudioFormatParams {
        id: &af.id,
        track_id: &af.track_id,
        content_type: af.content_type.as_str(),
        pregap_samples: optional_sqlite_count(af.pregap_samples, "pregap_samples")?,
        generated_pregap_samples: optional_sqlite_count(
            af.generated_pregap_samples,
            "generated_pregap_samples",
        )?,
        sample_rate: af.sample_rate.get(),
        bits_per_sample: af.bits_per_sample.map(std::num::NonZeroU32::get),
        channels: af.channels.get(),
        track_loudness_lufs: af.track_loudness_lufs,
        track_peak_linear: af.track_peak_linear,
        created_at: af.created_at.to_rfc3339(),
    })?;
    Ok(())
}

coven::write!(
    InsertAudioSegment(
        id: &'p str,
        audio_format_id: &'p str,
        segment_index: i64,
        role: &'static str,
        file_id: &'p str,
        start_sample: i64,
        end_sample: Option<i64>,
        start_byte: Option<i64>,
        end_byte: Option<i64>,
        created_at: String,
    ),
    r#"
    INSERT INTO audio_format_segments (
        id, audio_format_id, segment_index, role, file_id, start_sample, end_sample,
        start_byte, end_byte, created_at
    ) VALUES (
        :id, :audio_format_id, :segment_index, :role, :file_id, :start_sample, :end_sample,
        :start_byte, :end_byte, :created_at
    )
"#
);

pub fn insert_audio_segment_row(
    conn: &WriteContext<'_>,
    segment: &DbAudioSegment,
) -> Result<(), DbError> {
    conn.run(&InsertAudioSegmentParams {
        id: &segment.id,
        audio_format_id: &segment.audio_format_id,
        segment_index: segment.segment_index,
        role: segment.role.as_str(),
        file_id: &segment.file_id,
        start_sample: to_i64(segment.start_sample, "start_sample")?,
        end_sample: optional_sqlite_count(segment.end_sample, "end_sample")?,
        start_byte: optional_sqlite_count(segment.start_byte, "start_byte")?,
        end_byte: optional_sqlite_count(segment.end_byte, "end_byte")?,
        created_at: segment.created_at.to_rfc3339(),
    })?;
    Ok(())
}

coven::write!(
    InsertScrubPoints(audio_segment_id: &'p str, points: Vec<u8>),
    r#"
    INSERT INTO audio_segment_scrub_points (audio_format_segment_id, points)
    VALUES (:audio_segment_id, :points)
"#
);

pub fn insert_scrub_points_row(
    conn: &WriteContext<'_>,
    points: &DbScrubPoints,
) -> Result<(), DbError> {
    conn.run(&InsertScrubPointsParams {
        audio_segment_id: &points.audio_segment_id,
        points: super::scrub_points_column::encode(&points.points),
    })?;
    Ok(())
}

coven::write!(
    InsertCarvedTrack(track_id: &'p str, sheet_file_id: &'p str, file_reference: &'p str),
    r#"
    INSERT INTO carved_tracks (track_id, sheet_file_id, file_reference)
    VALUES (:track_id, :sheet_file_id, :file_reference)
"#
);

/// Record that the CUE sheet `sheet_file_id`, one of a release's files,
/// carves the track `track_id` out of an image, from under the sheet's
/// `FILE` line `file_reference`.
pub fn insert_carved_track_row(
    conn: &WriteContext<'_>,
    track_id: &str,
    sheet_file_id: &str,
    file_reference: &str,
) -> Result<(), DbError> {
    conn.run(&InsertCarvedTrackParams { track_id, sheet_file_id, file_reference })?;
    Ok(())
}

/// [`to_i64`] of a value that may be absent.
fn optional_sqlite_count(value: Option<u64>, what: &str) -> Result<Option<i64>, DbError> {
    value.map(|value| to_i64(value, what)).transpose()
}

// coven's record of the file `:id`, one byte long, that only device 1 holds,
// as coven writes it.
#[cfg(test)]
coven::write!(
    PlantCovenFile(id: &'p str),
    r#"
    INSERT OR IGNORE INTO coven_files (id, audience, size, coordinates, namespace)
    VALUES (:id, '', 1, 'device 1', 'release_files')
"#
);

// Every stored file's recorded length as `:size`, which may be a length no
// write stores.
#[cfg(test)]
coven::write!(DamageCovenFileSizes(size: i64), "UPDATE coven_files SET size = :size");

// A release file row with any role, kind and source-audio facts, a
// combination a CHECK refuses among them.
#[cfg(test)]
coven::write!(
    PlantReleaseFile(
        id: &'p str,
        release_id: &'p str,
        content_type: &'p str,
        file: &'p str,
        role: &'p str,
        document_kind: Option<&'p str>,
        source_audio_layout: Option<&'p str>,
        source_audio_content_type: Option<&'p str>,
        source_audio_container: Option<&'p str>,
        source_audio_duration_ms: Option<i64>,
        source_audio_sample_rate_hz: Option<i64>,
        source_audio_bits_per_sample: Option<i64>,
        source_audio_channels: Option<i64>,
        created_at: &'p str,
    ),
    r#"
    INSERT INTO release_files (
        id, release_id, original_filename, content_type, file, role, document_kind,
        source_audio_layout, source_audio_content_type, source_audio_container,
        source_audio_duration_ms, source_audio_sample_rate_hz, source_audio_bits_per_sample,
        source_audio_channels, created_at
    ) VALUES (
        :id, :release_id, :id, :content_type, :file, :role, :document_kind,
        :source_audio_layout, :source_audio_content_type, :source_audio_container,
        :source_audio_duration_ms, :source_audio_sample_rate_hz, :source_audio_bits_per_sample,
        :source_audio_channels, :created_at
    )
"#
);
