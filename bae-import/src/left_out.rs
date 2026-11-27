//! The inputs of a folder, or of a library release, that identification
//! reads, and what happens to one it can't: it is logged, reported to the
//! crash reporter and left out, and identification goes on with the others.
//! No part of an input left out is used, since a Disc ID of part of a table
//! or the clues of part of a file can match another release.
//!
//! Every read of an input goes through [`FolderInputs::read`], or, where the
//! read is awaited, ends in [`FolderInputs::leave_out`] or, for one that
//! panicked, [`FolderInputs::leave_out_unfinished`].

use bae_diagnostics::{Diagnostics, UiErrorCategory};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// What an input was being read as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InputRead {
    RipLog,
    RipLogDiscId,
    AccurateRipReport,
    SheetDiscId,
    AudioTags,
    TextFile,
    ArtworkImage,
    LibraryFile,
    Cover,
}

impl InputRead {
    /// The step the crash reporter is told of.
    fn operation(self) -> &'static str {
        match self {
            Self::RipLog => "identify: rip log",
            Self::RipLogDiscId => "identify: rip log Disc ID",
            Self::AccurateRipReport => "identify: AccurateRip report",
            Self::SheetDiscId => "identify: CUE sheet Disc ID",
            Self::AudioTags => "identify: audio tags",
            Self::TextFile => "identify: text file",
            Self::ArtworkImage => "identify: artwork image",
            Self::LibraryFile => "identify: library release file",
            Self::Cover => "identify: library release cover",
        }
    }
}

/// Why an input was left out.
#[derive(Debug)]
pub(crate) struct LeftOut {
    /// The whole error, which can name the person's files and quote them, for
    /// the log on this device.
    logged: String,
    /// The reason with no path, file name or text of the file, which leaves
    /// the device. Only the constructors below make one.
    reported: String,
}

impl LeftOut {
    /// Opening or reading the file failed. The error's text can name the
    /// file, so only its kind and the system's code are reported.
    pub(crate) fn io(error: &std::io::Error) -> Self {
        Self::of_io(error.to_string(), error.kind(), error.raw_os_error())
    }

    /// [`Self::io`] for a failure told as `logged`.
    pub(crate) fn of_io(logged: String, kind: std::io::ErrorKind, code: Option<i32>) -> Self {
        let reported = match code {
            Some(code) => format!("reading it failed: {kind:?}, system error {code}"),
            None => format!("reading it failed: {kind:?}"),
        };
        Self { logged, reported }
    }

    /// The file is no longer the one the scan read.
    pub(crate) fn changed(logged: String) -> Self {
        Self { logged, reported: "it changed after it was scanned".to_string() }
    }

    /// The disc it describes can't be hashed to a Disc ID.
    pub(crate) fn disc_id(error: &crate::discid::DiscIdError) -> Self {
        Self { logged: error.to_string(), reported: error.reported() }
    }

    /// The library could not give the file's bytes, for a failure of
    /// `category`.
    pub(crate) fn library(logged: String, category: UiErrorCategory) -> Self {
        Self { logged, reported: format!("the library could not give it: {category:?}") }
    }

    /// The platform's analyzer could not read the image. It is handed the
    /// image's bytes alone, so what it says names no file.
    pub(crate) fn unreadable_image(detail: String) -> Self {
        let reported = format!("the image could not be read: {detail}");
        Self { logged: detail, reported }
    }
}

/// An image read on the blocking pool hands its reason back as its error: the
/// image's fault, not a bug of the work.
impl coven::task::CarriesBug for LeftOut {
    fn bug(&self) -> Option<&coven::task::Bug> {
        None
    }
}

/// The reads of one pass's inputs: what each one left out is told to, and
/// which files they were.
pub(crate) struct FolderInputs {
    diagnostics: Diagnostics,
    left_out: std::sync::Mutex<HashSet<PathBuf>>,
}

impl FolderInputs {
    pub(crate) fn new(diagnostics: Diagnostics) -> Self {
        Self { diagnostics, left_out: Default::default() }
    }

    /// What `read` gives of the input at `path`, or `None` where it fails or
    /// panics, which leaves the input out.
    pub(crate) fn read<T>(
        &self,
        input: InputRead,
        path: &Path,
        read: impl FnOnce() -> Result<T, LeftOut>,
    ) -> Option<T> {
        match bae_util::task_failure::caught_sync(input.operation(), read) {
            Ok(Ok(value)) => Some(value),
            Ok(Err(why)) => {
                self.leave_out(input, path, why);
                None
            }
            Err(bug) => {
                let bug = self.diagnostics.panicked(bug);
                self.leave_out_unfinished(input, path, &bug);
                None
            }
        }
    }

    /// Leave out the input at `path`, whose read as `input` failed for `why`.
    pub(crate) fn leave_out(&self, input: InputRead, path: &Path, why: LeftOut) {
        self.diagnostics.report_bug_logged_as(
            input.operation(),
            &format_args!("left out {}: {}", path.display(), why.logged),
            &why.reported,
        );
        self.remember(path);
    }

    /// Leave out the input at `path`, whose read as `input` ended without a
    /// result for `why`. Its panic has been reported already, by the panic
    /// hook, so this only logs.
    pub(crate) fn leave_out_unfinished(
        &self,
        input: InputRead,
        path: &Path,
        why: &dyn std::fmt::Display,
    ) {
        tracing::error!(operation = input.operation(), "left out {}: {why}", path.display());
        self.remember(path);
    }

    fn remember(&self, path: &Path) {
        self.left_out.lock().expect("left-out inputs lock poisoned").insert(path.to_path_buf());
    }

    /// Whether a read of the file at `path` left it out.
    pub(crate) fn left_out(&self, path: &Path) -> bool {
        self.left_out.lock().expect("left-out inputs lock poisoned").contains(path)
    }

    /// Whether every input read so far was read.
    pub(crate) fn all_read(&self) -> bool {
        self.left_out.lock().expect("left-out inputs lock poisoned").is_empty()
    }
}

/// Inputs whose reports a test reads back.
#[cfg(test)]
pub(crate) fn reporting_inputs(
) -> (FolderInputs, std::sync::Arc<bae_diagnostics::RecordingBugReporter>) {
    let bugs = std::sync::Arc::new(bae_diagnostics::RecordingBugReporter::default());
    (FolderInputs::new(Diagnostics::noop().reporting_bugs_to(bugs.clone())), bugs)
}

#[cfg(test)]
#[path = "left_out_tests.rs"]
mod tests;
