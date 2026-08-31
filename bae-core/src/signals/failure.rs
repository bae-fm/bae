//! The typed reason a metadata lookup failed.
//!
//! The locale never crosses the bridge: core emits this typed reason and the UI
//! renders the localized line. A provider's HTTP status stays structured, never
//! flattened into prose, so the UI can show it. `Diagnostic` is the one variant
//! carrying free text — an opaque, log-only error chain, never translated and never
//! shown as primary copy.

/// Closed to exactly the cases the producers distinguish. A MusicBrainz lookup fails
/// as `Network` / `Timeout` / `Provider`; a local failure (re-identify resolution,
/// the in-library check, a compute task panic) as `Diagnostic`, since it carries no
/// provider verdict.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LookupFailure {
    /// A transport/connection failure that produced no HTTP response
    /// (connection refused, DNS failure, a dropped body).
    Network,
    /// An HTTP error response from the metadata provider. `status` is the
    /// HTTP status code when one was observed.
    Provider { status: Option<u16> },
    /// The request timed out before a response arrived.
    Timeout,
    /// Artwork analysis failed before it could finish extracting barcode/text
    /// signals.
    ArtworkAnalysis,
    /// A local error — a DB load, a "release not found", a compute task panic.
    /// `detail` is the opaque error chain: log-only, never translated.
    Diagnostic { detail: String },
}
