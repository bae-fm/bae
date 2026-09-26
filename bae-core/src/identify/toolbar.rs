//! The signals-toolbar badge types. Each identifying signal — the disc ID, the
//! barcode, the catalog number — becomes one [`ToolbarSignal`] carrying its
//! value, where it came from, its lookup state, and whether the user checked it.
//! The automation surface reports them as they are; it derives nothing.
//!
//! The derivation lives in [`super::state`], in
//! [`crate::identify::IdentifyState::toolbar`], and rides each state transition.

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
    /// A lookup is in flight — the badge spins.
    LookingUp,
    /// A lookup settled with `count` releases.
    Found { count: u32 },
    /// A lookup settled with zero releases.
    NoMatch,
    /// The signal had nothing to run: no disc layout, no codes found, or — for
    /// the catalog — no number chosen out of the ones extracted.
    Skipped,
    /// The signal's lookup is switched off in the identification settings, so
    /// nothing was asked about the value it holds.
    Off,
    /// A lookup failed. The UI resolves a localized line per variant, and shows
    /// the opaque detail for `Diagnostic`.
    Failed { failure: LookupFailure },
}

/// One of the values a signal could take, for the signals that offer several.
/// A candidate can carry thirty extracted catalog numbers, or two barcodes;
/// each signal is one badge with its list behind it, not one badge per value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignalOption {
    pub value: String,
    /// Where the value was first seen; a value seen in several places names
    /// the first.
    pub origin: SignalOrigin,
    /// Whether the identify run asks about this one. Several options of a
    /// signal can be chosen at once.
    pub chosen: bool,
}

/// Where the value a badge shows was read: the disc's table of contents for
/// the disc ID, or wherever a barcode or catalog number was read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolbarOrigin {
    /// The disc's table of contents (LOG/CUE).
    DiscToc,
    Value(SignalOrigin),
}

/// The value a badge shows, and where it was read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolbarValue {
    pub value: String,
    pub origin: ToolbarOrigin,
}

/// One badge in the signals toolbar. An unchecked badge still appears (struck
/// through, dimmed), so the row's layout holds steady as the user toggles
/// signals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolbarSignal {
    pub kind: SignalKind,
    /// The disc-ID hash, the barcode digits, the first chosen catalog number
    /// (every chosen one is marked in `options`), each with where it was
    /// read. `None` when the signal has nothing to show — no disc layout, no
    /// codes found, no catalog number chosen — and so no place it was read.
    pub shown: Option<ToolbarValue>,
    pub state: SignalState,
    /// Whether the run asks about none of this signal's values: the disc ID
    /// taken out, or every one of the candidate's barcodes. The catalog is
    /// never "excluded" — choosing no number is how it stays out — so it is
    /// always `false` there.
    pub excluded: bool,
    /// The values this signal offers, each marked when the run asks about it.
    /// Empty for the disc ID, which has one value the badge itself stands for,
    /// and for a signal the candidate carries no value of.
    pub options: Vec<SignalOption>,
}
