//! The typed reason a metadata lookup failed.
//!
//! The locale never crosses the bridge: bae-core emits this typed reason and a
//! stable catalog key per variant; the UI renders the localized line. The HTTP
//! status from the metadata provider is carried structured (never flattened
//! into prose), so the UI can show it. `Diagnostic` is the one variant that
//! carries free text — an opaque, log-only error chain that is never
//! translated and never shown as primary copy.

/// Why a metadata lookup failed, closed to exactly the cases the producers
/// distinguish.
///
/// The MusicBrainz disc-ID lookup maps to `Network` / `Timeout` /
/// `Provider`. Local failures (re-identify resolution, the in-library check,
/// a disc-ID compute task panic) map to `Diagnostic` — they carry no provider
/// verdict, only diagnostic detail.
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// A local error (DB load, "release not found", a disc-ID compute task
    /// panic). `detail` is the opaque error chain — log-only, never
    /// translated, never shown as primary user-facing copy.
    Diagnostic { detail: String },
}
