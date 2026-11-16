//! What a release's audio files are: each file's format as the scan read it,
//! and the one summary every surface shows for the release.
//!
//! Files share a format when their codec, sample rate, bit depth, channels and
//! layout match. Bitrate is not part of it: a variable-bitrate encode states a
//! different average for every file, and those files are one format. A shared
//! lossy format states the release's average bitrate instead.
//!
//! One SQL statement, [`source_audio_summary_sql`], computes the summary for
//! every surface: the library's album detail and release editor read it over
//! a release's stored files, and the import pane reads it over a scanned
//! folder's files.

use bae_store::SchemaFunction;
use bae_types::audio_format::AudioFormat;
use bae_util::content_type::ContentType;

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

/// `bae_audio_codec`: the name bae shows for the codec of the audio content
/// type that its one argument names, as stored.
pub(crate) static AUDIO_CODEC: SchemaFunction = SchemaFunction {
    name: "bae_audio_codec",
    arguments: 1,
    version: 1,
    compute: |arguments| {
        let content_type = arguments.get::<String>(0)?;
        Ok(rusqlite::types::Value::Text(
            ContentType::from_mime(&content_type).display_name().to_string(),
        ))
    },
};

/// An SQL expression for the summary of the audio files that the statement
/// `audio` selects: a JSON object that [`SourceAudioSummary::deserialize_stored`]
/// reads, or NULL when no file counts.
///
/// `audio` returns one row for each file, with the columns `position` (the
/// order the files are read in), `layout` (`'file'` or `'cue'`, NULL for a
/// file the tracklist leaves out), `content_type`, `sample_rate_hz`,
/// `bits_per_sample`, `bitrate_kbps`, `channels` and `duration_ms`. The
/// summary leaves out a file with no layout and a file whose content type,
/// sample rate or channels the scan did not read.
///
/// When every file has the same layout, codec, sample rate, bit depth and
/// channels, the summary is that format, with the average of the files'
/// bitrates, each file's weighted by its duration (each file counts once when
/// no file states a duration). Otherwise the summary lists each fact the files
/// disagree on, with its values in the order the files first show them.
pub fn source_audio_summary_sql(audio: &str) -> String {
    let differing = |column: &str| {
        format!(
            "json((SELECT json_group_array({column} ORDER BY first) \
                   FROM (SELECT {column}, MIN(position) AS first FROM audio \
                         WHERE {column} IS NOT NULL GROUP BY {column}) \
                   HAVING COUNT(*) >= 2))"
        )
    };
    format!(
        "json((WITH audio AS ( \
              SELECT position, layout, bae_audio_codec(content_type) AS codec, \
                     sample_rate_hz, bits_per_sample, bitrate_kbps, channels, \
                     COALESCE(duration_ms, 0) AS duration_ms \
              FROM ({audio}) \
              WHERE layout IS NOT NULL AND content_type IS NOT NULL \
                AND sample_rate_hz IS NOT NULL AND channels IS NOT NULL), \
          rated AS (SELECT bitrate_kbps, duration_ms FROM audio WHERE bitrate_kbps IS NOT NULL) \
          SELECT CASE WHEN EXISTS (SELECT 1 FROM audio) THEN json_object( \
              'uniform', (SELECT COUNT(*) FROM (SELECT DISTINCT layout, codec, sample_rate_hz, \
                                                       bits_per_sample, channels \
                                                FROM audio)) = 1, \
              'first', json((SELECT json_object('layout', layout, 'codec', codec, \
                                                'sample_rate_hz', sample_rate_hz, \
                                                'bits_per_sample', bits_per_sample, \
                                                'channels', channels) \
                             FROM audio ORDER BY position LIMIT 1)), \
              'bitrate_kbps', \
                  (SELECT CASE WHEN COUNT(*) = 0 THEN NULL \
                               WHEN SUM(duration_ms) > 0 \
                               THEN (SUM(bitrate_kbps * duration_ms) + SUM(duration_ms) / 2) \
                                    / SUM(duration_ms) \
                               ELSE (SUM(bitrate_kbps) + COUNT(*) / 2) / COUNT(*) END \
                   FROM rated), \
              'layouts', {layouts}, 'codecs', {codecs}, 'sample_rates_hz', {rates}, \
              'bits_per_sample', {bits}, 'channels', {channels}) END))",
        layouts = differing("layout"),
        codecs = differing("codec"),
        rates = differing("sample_rate_hz"),
        bits = differing("bits_per_sample"),
        channels = differing("channels"),
    )
}

/// The JSON object [`source_audio_summary_sql`] computes.
#[derive(serde::Deserialize)]
struct StoredSummary {
    uniform: i64,
    first: StoredFormat,
    bitrate_kbps: Option<i64>,
    layouts: Option<Vec<String>>,
    codecs: Option<Vec<String>>,
    sample_rates_hz: Option<Vec<i64>>,
    bits_per_sample: Option<Vec<i64>>,
    channels: Option<Vec<i64>>,
}

#[derive(serde::Deserialize)]
struct StoredFormat {
    layout: String,
    codec: String,
    sample_rate_hz: i64,
    bits_per_sample: Option<i64>,
    channels: i64,
}

fn stored_layout(stored: &str) -> Result<SourceAudioLayout, String> {
    match stored {
        "file" => Ok(SourceAudioLayout::File),
        "cue" => Ok(SourceAudioLayout::Cue),
        other => Err(format!("{other:?} is no audio layout")),
    }
}

impl StoredSummary {
    fn summary(self) -> Result<SourceAudioSummary, String> {
        if self.uniform != 0 {
            return Ok(SourceAudioSummary::Uniform {
                descriptor: SourceAudioDescriptor {
                    layout: stored_layout(&self.first.layout)?,
                    format: AudioFormat {
                        codec: self.first.codec,
                        sample_rate_hz: self.first.sample_rate_hz,
                        bits_per_sample: self.first.bits_per_sample,
                        bitrate_kbps: self.bitrate_kbps,
                        channels: self.first.channels,
                    },
                },
            });
        }
        let layouts = self
            .layouts
            .map(|layouts| layouts.iter().map(|stored| stored_layout(stored)).collect())
            .transpose()?
            .map(|layouts| SourceAudioDifference::Layout { layouts });
        let differences = [
            layouts,
            self.codecs.map(|codecs| SourceAudioDifference::Codec { codecs }),
            self.sample_rates_hz
                .map(|sample_rates_hz| SourceAudioDifference::SampleRate { sample_rates_hz }),
            self.bits_per_sample
                .map(|bits_per_sample| SourceAudioDifference::BitDepth { bits_per_sample }),
            self.channels.map(|channels| SourceAudioDifference::Channels { channels }),
        ]
        .into_iter()
        .flatten()
        .collect();
        Ok(SourceAudioSummary::Mixed { differences })
    }
}

impl SourceAudioSummary {
    /// Reads the JSON object that [`source_audio_summary_sql`] computes, or
    /// `None` from its NULL; for a `#[serde(deserialize_with)]` field.
    pub fn deserialize_stored<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<Self>, D::Error> {
        use serde::Deserialize;
        Option::<StoredSummary>::deserialize(deserializer)?
            .map(StoredSummary::summary)
            .transpose()
            .map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
#[path = "source_audio_tests.rs"]
mod tests;
