//! The text signal: catalog numbers and free text read off a candidate's
//! surfaces, and the lines they were read from.

use super::{InternalFailure, TextOrigin};

/// The catalog numbers and artist/album free text, each once in first-seen
/// order; final once `Settled`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextSignal {
    Scanning {
        catalogs: Vec<String>,
        free_text: Vec<String>,
    },
    Settled {
        catalogs: Vec<String>,
        free_text: Vec<String>,
    },
    Failed {
        failure: InternalFailure,
        catalogs: Vec<String>,
        free_text: Vec<String>,
    },
}

impl TextSignal {
    pub fn catalogs(&self) -> &[String] {
        match self {
            TextSignal::Scanning { catalogs, .. }
            | TextSignal::Settled { catalogs, .. }
            | TextSignal::Failed { catalogs, .. } => catalogs,
        }
    }

    #[cfg(test)]
    pub fn free_text(&self) -> &[String] {
        match self {
            TextSignal::Scanning { free_text, .. }
            | TextSignal::Settled { free_text, .. }
            | TextSignal::Failed { free_text, .. } => free_text,
        }
    }
}

/// One line of the candidate's own text, verbatim — what ranking looks a
/// result's fields up in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextLine {
    pub text: String,
    /// The surface it was read off, which ranking weighs it by.
    pub origin: TextOrigin,
}
