//! The signal badges: one [`ToolbarSignal`] per identifying signal, built by
//! [`crate::identify::IdentifyState::toolbar`].

use crate::identify::NotAskedReason;
use crate::signals::{LookupFailure, SignalOrigin};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalKind {
    DiscId,
    Barcode,
    Catalog,
}

/// One badge's live lookup state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignalState {
    LookingUp,
    Found {
        count: u32,
    },
    NoMatch,
    /// The signal had nothing to look up: no disc layout, no codes, or no
    /// catalog number chosen.
    Skipped,
    /// The signal holds a value and nobody was asked about it.
    NotAsked {
        reason: NotAskedReason,
    },
    Failed {
        failure: LookupFailure,
    },
}

/// One of the values a signal offers; a signal is one badge with its values
/// listed behind it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignalOption {
    pub value: String,
    /// Where the value was first seen.
    pub origin: SignalOrigin,
    /// Whether the run asks about this value; several can be chosen at once.
    pub chosen: bool,
}

/// Where the value a badge shows was read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolbarOrigin {
    /// The disc's table of contents (LOG/CUE).
    DiscToc,
    Value(SignalOrigin),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolbarValue {
    pub value: String,
    pub origin: ToolbarOrigin,
}

/// One badge; a badge whose values are all left out still appears.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolbarSignal {
    pub kind: SignalKind,
    /// The value the badge shows; `None` when the signal has none.
    pub shown: Option<ToolbarValue>,
    pub state: SignalState,
    /// Whether the person left every value out; always `false` for the
    /// catalog, which stays out by having no number chosen.
    pub excluded: bool,
    /// The values the signal offers; empty for the disc ID, which has one.
    pub options: Vec<SignalOption>,
}
