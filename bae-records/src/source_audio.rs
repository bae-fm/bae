//! What a release's audio files are: each file's format as the scan read it,
//! and the one summary every surface shows for the release.
//!
//! Files share a format when their codec, sample rate, bit depth, channels and
//! layout match. Bitrate is not part of it: a variable-bitrate encode states a
//! different average for every file, and those files are one format. A shared
//! lossy format states the release's average bitrate instead.
//!
//! One fragment, [`AudioSummary`], computes the summary for every surface:
//! the library's album detail and release editor read it over a release's
//! stored files, and the import pane reads it over a scanned folder's files.

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
pub(crate) fn audio_codec(
    arguments: &bae_store::SqlArguments<'_>,
) -> rusqlite::Result<rusqlite::types::Value> {
    let content_type = arguments.get::<String>(0)?;
    Ok(rusqlite::types::Value::Text(
        ContentType::from_mime(&content_type).display_name().to_string(),
    ))
}

// The summary of the audio files `counted` gives, as a JSON object that
// [`SourceAudioSummary::deserialize_stored`] reads, or NULL when no file
// counts. `counted` gives one row for each file: `position` (the order the
// files are read in), `layout` (`'file'` or `'cue'`, NULL for a file the
// tracklist leaves out) and the audio facts the scan read. The summary leaves
// out a file with no layout and a file whose content type, sample rate or
// channels the scan did not read.
//
// When every file has the same layout, codec, sample rate, bit depth and
// channels, the summary is that format, with the average of the files'
// bitrates, each file's weighted by its duration (each file counts once when
// no file states a duration). Otherwise the summary lists each fact the files
// disagree on, with its values in the order the files first show them.
sqlive_compose::fragment!(
    AudioSummary
    with counted(
        position INTEGER, layout TEXT, content_type TEXT, sample_rate_hz INTEGER,
        bits_per_sample INTEGER, bitrate_kbps INTEGER, channels INTEGER,
        duration_ms INTEGER
    ) { summary: Option<String> },
    r#"
    SELECT CASE WHEN collected.files > 0 THEN (
        SELECT json_object(
            'uniform', COUNT(DISTINCT json_array(
                           audio.value ->> 0, audio.value ->> 1, audio.value ->> 2,
                           audio.value ->> 3, audio.value ->> 5)) = 1,
            'first', json((SELECT json_object(
                               'layout', first.value ->> 0, 'codec', first.value ->> 1,
                               'sample_rate_hz', first.value ->> 2,
                               'bits_per_sample', first.value ->> 3,
                               'channels', first.value ->> 5)
                           FROM json_each(collected.listed) AS first
                           ORDER BY first.key LIMIT 1)),
            'bitrate_kbps',
                CASE WHEN COUNT(audio.value ->> 4) = 0 THEN NULL
                     WHEN SUM(audio.value ->> 6) FILTER (WHERE audio.value ->> 4 IS NOT NULL) > 0
                     THEN (SUM((audio.value ->> 4) * (audio.value ->> 6))
                           + SUM(audio.value ->> 6)
                                 FILTER (WHERE audio.value ->> 4 IS NOT NULL) / 2)
                          / SUM(audio.value ->> 6) FILTER (WHERE audio.value ->> 4 IS NOT NULL)
                     ELSE (SUM(audio.value ->> 4) + COUNT(audio.value ->> 4) / 2)
                          / COUNT(audio.value ->> 4) END,
            'layouts', json((SELECT json_group_array(facts.stated ORDER BY facts.first)
                FROM (SELECT of_file.value ->> 0 AS stated, MIN(of_file.key) AS first
                      FROM json_each(collected.listed) AS of_file
                      WHERE of_file.value ->> 0 IS NOT NULL GROUP BY 1) AS facts
                HAVING COUNT(*) >= 2)),
            'codecs', json((SELECT json_group_array(facts.stated ORDER BY facts.first)
                FROM (SELECT of_file.value ->> 1 AS stated, MIN(of_file.key) AS first
                      FROM json_each(collected.listed) AS of_file
                      WHERE of_file.value ->> 1 IS NOT NULL GROUP BY 1) AS facts
                HAVING COUNT(*) >= 2)),
            'sample_rates_hz', json((SELECT json_group_array(facts.stated ORDER BY facts.first)
                FROM (SELECT of_file.value ->> 2 AS stated, MIN(of_file.key) AS first
                      FROM json_each(collected.listed) AS of_file
                      WHERE of_file.value ->> 2 IS NOT NULL GROUP BY 1) AS facts
                HAVING COUNT(*) >= 2)),
            'bits_per_sample', json((SELECT json_group_array(facts.stated ORDER BY facts.first)
                FROM (SELECT of_file.value ->> 3 AS stated, MIN(of_file.key) AS first
                      FROM json_each(collected.listed) AS of_file
                      WHERE of_file.value ->> 3 IS NOT NULL GROUP BY 1) AS facts
                HAVING COUNT(*) >= 2)),
            'channels', json((SELECT json_group_array(facts.stated ORDER BY facts.first)
                FROM (SELECT of_file.value ->> 5 AS stated, MIN(of_file.key) AS first
                      FROM json_each(collected.listed) AS of_file
                      WHERE of_file.value ->> 5 IS NOT NULL GROUP BY 1) AS facts
                HAVING COUNT(*) >= 2)))
        FROM json_each(collected.listed) AS audio) END AS summary
    FROM (
        SELECT json_group_array(json_array(
                   counted.layout, bae_audio_codec(counted.content_type),
                   counted.sample_rate_hz, counted.bits_per_sample, counted.bitrate_kbps,
                   counted.channels, COALESCE(counted.duration_ms, 0))
                   ORDER BY counted.position) AS listed,
               COUNT(*) AS files
        FROM counted
        WHERE counted.layout IS NOT NULL AND counted.content_type IS NOT NULL
          AND counted.sample_rate_hz IS NOT NULL AND counted.channels IS NOT NULL
    ) AS collected
"#
);

/// The JSON object [`AudioSummary`] computes.
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
    /// Reads the JSON object that [`AudioSummary`] computes, or `None` from
    /// its NULL; for a `#[serde(deserialize_with)]` field.
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
