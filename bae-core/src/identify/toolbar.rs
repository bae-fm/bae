//! The signals-toolbar badge types. Each identifying signal — the disc ID, the
//! barcode, the catalog number — becomes one [`ToolbarSignal`] carrying its
//! value, where it came from, its lookup state, and whether the user checked it.
//! The UI iterates and renders; it derives nothing.
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
    /// A lookup failed. The UI resolves a localized line per variant, and shows
    /// the opaque detail for `Diagnostic`.
    Failed { failure: LookupFailure },
}

/// One of the values a signal could take, for the signals that offer a choice.
/// A candidate can carry thirty extracted catalog numbers; they are one badge
/// with a list behind it, not thirty badges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignalOption {
    pub value: String,
    pub origin: SignalOrigin,
    /// Whether this is the one the identify run is using. At most one option of
    /// a signal is chosen.
    pub chosen: bool,
}

/// One badge in the signals toolbar. An unchecked badge still appears (struck
/// through, dimmed), so the row's layout holds steady as the user toggles
/// signals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolbarSignal {
    pub kind: SignalKind,
    /// The disc-ID hash, the barcode digits, the chosen catalog number. `None`
    /// when the signal has nothing to show — no disc layout, no codes found, no
    /// catalog number chosen.
    pub value: Option<String>,
    pub origin: SignalOrigin,
    pub state: SignalState,
    /// Whether the user has taken this signal out of the run. The catalog is
    /// never "excluded" — choosing no option is how it stays out — so it is
    /// always `false` there.
    pub excluded: bool,
    /// The values this signal could take, when it is one of the signals that
    /// offers a choice. Empty for the disc ID and the barcode, which have one
    /// value each.
    pub options: Vec<SignalOption>,
}
