//! Locale-free wire mirrors for the Windows FFI.
//!
//! The locale never crosses the bridge: bae-core emits raw numbers, typed
//! enums, and stable `core.*` catalog keys; the C# UI renders for its locale
//! (numbers/bytes/durations via `Windows.Globalization`/`TimeSpan`, keys via
//! `ResourceLoader.GetString` + the `MessageFormat` NuGet on the MF1 value).
//!
//! The generated bridge exposes the catalog-key functions. The C ABI still
//! serializes older JSON wire mirrors; C# maps those tags to generated bridge
//! enums at its adapter boundary, then resolves the returned keys locally.

use serde::Serialize;

// ── Audio format ──────────────────────────────────────────────────────────
//
// Structured audio format mirroring `BridgeAudioFormat`. The C# composes the
// one-line descriptor from the parts: the codec is a proper noun, numbers
// format per locale, and the channel count maps to a localized word through the
// generated bridge key function.

/// Wire mirror of `bae_core::album_detail::AudioFormat` (and the bridge's
/// `BridgeAudioFormat`). `bits_per_sample` present means lossless (show the bit
/// depth); absent means lossy (show `bitrate_kbps`).
#[derive(Serialize)]
pub struct FfiAudioFormat {
    pub codec: String,
    pub sample_rate_hz: i64,
    pub bits_per_sample: Option<i64>,
    pub bitrate_kbps: Option<i64>,
    pub channels: i64,
}

impl FfiAudioFormat {
    pub fn from_core(f: &bae_core::album_detail::AudioFormat) -> Self {
        Self {
            codec: f.codec.clone(),
            sample_rate_hz: f.sample_rate_hz,
            bits_per_sample: f.bits_per_sample,
            bitrate_kbps: f.bitrate_kbps,
            channels: f.channels,
        }
    }
}

// ── Lookup failure ──────────────────────────────────────────────────────────
//
// Structured metadata-lookup failure mirroring `BridgeLookupFailure`. The C#
// resolves a localized line per variant with the generated bridge key function
// and renders `Provider`'s status as the message argument. `Diagnostic` carries
// opaque, log-only detail — never translated, never shown as primary copy.

/// Wire mirror of `bae_core::signals::LookupFailure` (and the bridge's
/// `BridgeLookupFailure`). `kind` tags the variant.
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FfiLookupFailure {
    /// Transport/connection failure — no HTTP response.
    Network,
    /// An HTTP error response, with its status code when one was observed.
    Provider { status: Option<u16> },
    /// The request timed out before a response arrived.
    Timeout,
    /// Artwork analysis failed before barcode/text extraction finished.
    ArtworkAnalysis,
    /// A local error. `detail` is the opaque error chain — log-only.
    Diagnostic { detail: String },
}

impl FfiLookupFailure {
    pub fn from_core(f: &bae_core::signals::LookupFailure) -> Self {
        use bae_core::signals::LookupFailure;
        match f {
            LookupFailure::Network => Self::Network,
            LookupFailure::Provider { status } => Self::Provider { status: *status },
            LookupFailure::Timeout => Self::Timeout,
            LookupFailure::ArtworkAnalysis => Self::ArtworkAnalysis,
            LookupFailure::Diagnostic { detail } => Self::Diagnostic {
                detail: detail.clone(),
            },
        }
    }
}

// ── Diagnostic error ──────────────────────────────────────────────────────────
//
// Structured user-facing error mirroring `BridgeError` (`bae_core::ui::UiError`).
// The C# shows one generic localized line per category / per missing entity;
// `detail` is the opaque Rust error chain — logged and offered in a copyable
// disclosure, never translated.

/// Wire mirror of `bae_core::ui::UiError` (and the bridge's `BridgeError`).
/// `kind` tags the variant. `not_found` carries the entity wire tag; `diagnostic`
/// carries the category wire tag plus the opaque detail.
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FfiError {
    /// A specific entity was missing. `entity` is one of
    /// "library"/"album"/"release"/"track"/"file".
    NotFound { entity: &'static str, id: String },
    /// A diagnostic failure. `category` is one of
    /// "database"/"config"/"internal"/"import"/"export".
    Diagnostic {
        category: &'static str,
        detail: String,
    },
}

impl FfiError {
    pub fn from_core(error: &bae_core::ui::UiError) -> Self {
        use bae_core::ui::UiError;
        match error {
            UiError::NotFound { entity, id } => Self::NotFound {
                entity: entity_kind_tag(*entity),
                id: id.clone(),
            },
            UiError::Diagnostic { category, detail } => Self::Diagnostic {
                category: error_category_tag(*category),
                detail: detail.clone(),
            },
        }
    }
}

/// Wire tag for a diagnostic error category.
fn error_category_tag(category: bae_core::ui::UiErrorCategory) -> &'static str {
    use bae_core::ui::UiErrorCategory;
    match category {
        UiErrorCategory::Database => "database",
        UiErrorCategory::Config => "config",
        UiErrorCategory::Internal => "internal",
        UiErrorCategory::Import => "import",
        UiErrorCategory::Export => "export",
    }
}

/// Wire tag for a missing-entity kind.
fn entity_kind_tag(entity: bae_core::ui::UiEntityKind) -> &'static str {
    use bae_core::ui::UiEntityKind;
    match entity {
        UiEntityKind::Library => "library",
        UiEntityKind::Album => "album",
        UiEntityKind::Release => "release",
        UiEntityKind::Track => "track",
        UiEntityKind::File => "file",
    }
}

// ── Playback error ──────────────────────────────────────────────────────────
//
// Structured playback failure mirroring `BridgePlaybackErrorReason`. The two
// actionable cloud-only cases are keyed; every in-core failure rides in
// `Diagnostic` and renders through the same `FfiError` category path.

/// Wire mirror of `bae_core::ui::PlaybackErrorReason` (and the bridge's
/// `BridgePlaybackErrorReason`). `kind` tags the variant; `Diagnostic` carries
/// the structured `FfiError`.
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FfiPlaybackErrorReason {
    SyncDisconnected,
    UploadPending,
    Diagnostic { error: FfiError },
}

impl FfiPlaybackErrorReason {
    pub fn from_core(reason: &bae_core::ui::PlaybackErrorReason) -> Self {
        use bae_core::ui::PlaybackErrorReason;
        match reason {
            PlaybackErrorReason::SyncDisconnected => Self::SyncDisconnected,
            PlaybackErrorReason::UploadPending => Self::UploadPending,
            PlaybackErrorReason::Diagnostic { error } => Self::Diagnostic {
                error: FfiError::from_core(error),
            },
        }
    }
}

// ── Import step ──────────────────────────────────────────────────────────────
//
// Structured import-progress step mirroring `BridgeImportStep`
// (`BridgePrepareStep`/`BridgeImportPhase`). The C# resolves the localized verb
// through generated bridge key functions; no English prose crosses the bridge.

/// Wire mirror of `bae_core::import::ImportStep`. `kind` tags whether it's a
/// prepare step or a running phase; `step`/`phase` is the inner wire tag.
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FfiImportStep {
    Preparing { step: &'static str },
    Running { phase: &'static str },
}

impl FfiImportStep {
    pub fn from_core(step: &bae_core::import::ImportStep) -> Self {
        use bae_core::import::{ImportPhase, ImportStep, PrepareStep};
        match step {
            ImportStep::Preparing(p) => Self::Preparing {
                step: match p {
                    PrepareStep::ParsingMetadata => "parsing_metadata",
                    PrepareStep::WritingCoverArt => "writing_cover_art",
                    PrepareStep::DiscoveringFiles => "discovering_files",
                    PrepareStep::ValidatingTracks => "validating_tracks",
                    PrepareStep::SavingToDatabase => "saving_to_database",
                },
            },
            ImportStep::Running(phase) => Self::Running {
                phase: match phase {
                    ImportPhase::ReferencingFiles => "referencing_files",
                    ImportPhase::MeasuringLoudness => "measuring_loudness",
                    ImportPhase::Finalizing => "finalizing",
                },
            },
        }
    }
}

#[cfg(test)]
mod tests {
    /// Load the master catalog the macOS app and the Windows `Core.resw` are
    /// generated from, so a renamed/dropped key fails this build instead of
    /// rendering a raw key in the WinUI app.
    fn catalog() -> bae_loc::Catalog {
        bae_loc::Catalog::from_toml(include_str!("../../bae-bridge/loc/catalog.toml"))
            .expect("catalog parses")
    }

    fn assert_key(cat: &bae_loc::Catalog, key: &str) {
        assert!(cat.messages.contains_key(key), "catalog missing `{key}`");
    }

    /// The structured-composition keys the C# resolves directly (side/disc
    /// headers, queue counts, outbox args, pressings plural) must all exist.
    #[test]
    fn composition_keys_exist() {
        let cat = catalog();
        for key in [
            "core.track.side",
            "core.track.disc",
            "core.import.pressings",
            "core.queue.uploading",
            "core.queue.downloading",
            "core.queue.failed",
            "core.queue.queued",
            "core.download.bytes_progress",
            "core.outbox.bytes_progress",
            "core.outbox.throughput",
            "core.outbox.eta",
            "core.outbox.pending_deletes",
        ] {
            assert_key(&cat, key);
        }
    }
}
