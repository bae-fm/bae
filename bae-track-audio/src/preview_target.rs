//! A stretch of a file to audition.

use std::path::{Path, PathBuf};
use std::time::Duration;

/// One local source window that can be auditioned. The sample span is relative
/// to `path`; byte seek landings are absent because an import candidate has not
/// recorded them yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewTarget {
    path: PathBuf,
    span: bae_records::SegmentSpan,
}

impl PreviewTarget {
    pub fn whole_file(path: PathBuf) -> Self {
        Self { path, span: bae_records::SegmentSpan::whole_file() }
    }

    pub fn sample_range(path: PathBuf, start_sample: u64, end_sample: Option<u64>) -> Self {
        Self {
            path,
            span: bae_records::SegmentSpan {
                start_sample,
                end_sample,
                start_byte: None,
                end_byte: None,
            },
        }
    }

    pub fn into_sample_range(self) -> (PathBuf, u64, Option<u64>) {
        (self.path, self.span.start_sample, self.span.end_sample)
    }

    /// The file the stretch is of.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The file's name, as a preview shows what it plays.
    pub fn name(&self) -> String {
        bae_util::path_text::name_text(&self.path)
    }

    /// The stretch of the file, in samples.
    pub fn span(&self) -> bae_records::SegmentSpan {
        self.span
    }

    pub fn duration(&self, file_duration: Duration, sample_rate: u32) -> Option<Duration> {
        let start = bae_util::sample_time::frame_start(self.span.start_sample, sample_rate);
        let end = self
            .span
            .end_sample
            .map(|sample| bae_util::sample_time::frame_start(sample, sample_rate))
            .unwrap_or(file_duration);
        end.checked_sub(start).filter(|duration| !duration.is_zero())
    }
}
