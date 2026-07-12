//! The signals-toolbar badge types. Each identifying signal — the disc ID, the
//! barcode, every catalog candidate — becomes one [`ToolbarSignal`] carrying its
//! value, where it came from, its lookup state, and whether the user excluded it.
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

/// Identity signals (disc ID, barcode) get looked up and intersected to *find*
/// releases; filter signals (catalog) get matched against what was found to
/// *narrow* it to one pressing. The UI splits the badge row on this.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalRole {
    Identity,
    Filter,
}

/// One badge's live lookup/match state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignalState {
    /// An identity lookup is in flight — the badge spins.
    LookingUp,
    /// An identity lookup settled with `count` releases.
    Found { count: u32 },
    /// An identity lookup settled with zero releases.
    NoMatch,
    /// The signal had no source to run — it never produced a value to look up.
    Skipped,
    /// An identity lookup failed. The UI resolves a localized line per variant, and
    /// shows the opaque detail for `Diagnostic`.
    Failed { failure: LookupFailure },
    /// This catalog candidate confirms `count` of the matched releases. Zero renders
    /// as muted noise; more than zero pinpoints the pressing.
    Confirms { count: u32 },
}

/// One badge in the signals toolbar. Excluded badges still appear (struck through,
/// dimmed), so the row's layout holds steady as the user toggles signals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolbarSignal {
    pub kind: SignalKind,
    pub role: SignalRole,
    /// The disc-ID hash, the barcode digits, the catalog number. `None` only when an
    /// identity signal has nothing to show — no disc layout, no codes found.
    pub value: Option<String>,
    pub origin: SignalOrigin,
    pub state: SignalState,
    pub excluded: bool,
}
