//! Unified identify pipeline. Consumes the [`crate::signals::Signals`] a
//! candidate's files yield (produced by the signal-extraction service) and
//! drives a state machine that matches it against external metadata. The
//! disc-ID and barcode signals are looked up in parallel (triangulation);
//! once both settle, `combine` reconciles their results into a terminal
//! `Found`, `Conflict`, or `NotFoundAnywhere`, carrying per-result provenance.
//!
//! The state machine is a pure reducer (`state::step`); the service
//! (`service::IdentifyService`) relays `SignalsUpdated` from the extraction
//! service into the reducer and interprets its side effects (the MB/Discogs
//! disc-ID and barcode lookups), feeding results back in. Scanning, OCR, and
//! disc-ID derivation belong to `crate::signals`, not here.
//!
//! All events flow through the existing `ImportEvent` broadcast channel via
//! `ImportEvent::IdentifyStateChanged`. Consumers see one event per state
//! transition, carrying the full state payload.

pub mod analyzer;
pub mod barcode;
pub mod candidate_text;
pub mod combine;
pub mod discid;
pub mod service;
pub mod state;
pub mod toolbar;

pub use analyzer::{ArtworkAnalysis, ArtworkAnalyzer, NoopAnalyzer};
pub use combine::{GroupKey, ResultProvenance};
pub use service::{IdentifyService, IdentifyServiceHandle};
pub use state::{
    BarcodeProgress, DiscidProgress, ExcludedSignal, IdentifyEvent, IdentifySource, IdentifyState,
};
pub use toolbar::{SignalKind, SignalRole, SignalState, ToolbarSignal};
