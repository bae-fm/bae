//! The signals-toolbar projection: a flat, pre-shaped list of badges the UI
//! renders directly. Each identifying signal — the disc ID, the barcode, and
//! every catalog candidate — becomes one [`ToolbarSignal`] carrying its value,
//! where it came from, its lookup state, and whether the user has excluded it
//! from triangulation. The UI iterates and renders.
//!
//! This module defines the badge types. The per-signal state derivation that
//! produces them from `IdentifyState` lives in [`super::state`], built by
//! [`crate::identify::IdentifyState::toolbar`] and broadcast alongside each
//! identify-state transition.

use crate::signals::{LookupFailure, SignalOrigin};

/// Which kind of signal a badge represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalKind {
    DiscId,
    Barcode,
    Catalog,
}

/// A signal's role in triangulation. Identity signals (disc ID, barcode) are
/// looked up and intersected to *find* releases; filter signals (catalog) are
/// matched against the found set to *narrow* it to one pressing. The UI splits
/// the badge row into an identity zone and a Refine zone on this.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalRole {
    Identity,
    Filter,
}

/// The live lookup/match state of one signal badge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignalState {
    /// The signal's lookup is in flight (identity signals) — the badge spins.
    LookingUp,
    /// An identity lookup settled with `count` releases.
    Found { count: u32 },
    /// An identity lookup settled with zero releases.
    NoMatch,
    /// The signal had no source to run (no disc layout, no artwork) — it never
    /// produced a value to look up.
    Skipped,
    /// An identity lookup failed. Carries the typed reason; the UI resolves a
    /// localized line per variant (and shows the opaque detail for
    /// `Diagnostic`).
    Failed { failure: LookupFailure },
    /// A catalog filter confirms `count` of the matched releases (their catno
    /// equals this candidate). `count == 0` renders as muted noise; `> 0`
    /// pinpoints the pressing with an accent check.
    Confirms { count: u32 },
}

/// One badge in the signals toolbar — a fully pre-shaped row the UI renders
/// without deriving anything. Excluded badges still appear (struck through,
/// dimmed), so the toolbar layout is stable as the user toggles signals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolbarSignal {
    pub kind: SignalKind,
    pub role: SignalRole,
    /// The signal's value — the disc-ID hash, the barcode digits, the catalog
    /// number. `None` only when an identity signal has no value to show (no
    /// disc layout, no barcode codes found).
    pub value: Option<String>,
    pub origin: SignalOrigin,
    pub state: SignalState,
    /// Whether the user has excluded this signal from triangulation.
    pub excluded: bool,
}
