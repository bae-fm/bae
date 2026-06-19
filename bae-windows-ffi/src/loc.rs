//! Locale-free wire mirrors and catalog-key selection for the Windows FFI.
//!
//! The locale never crosses the bridge: bae-core emits raw numbers, typed
//! enums, and stable `core.*` catalog keys; the C# UI renders for its locale
//! (numbers/bytes/durations via `Windows.Globalization`/`TimeSpan`, keys via
//! `ResourceLoader.GetString` + the `MessageFormat` NuGet on the MF1 value).
//!
//! macOS binds the same core through uniffi, which generates `bridge_*_key()`
//! functions from `bae-bridge/src/types.rs`. Windows has no uniffi, so the
//! enum→key mapping is hand-mirrored here — this module is the single source of
//! the `core.*` key strings on Windows (mirroring the strings in
//! `bae-bridge/src/types.rs`). Each `*_key` returns exactly the same key macOS
//! resolves, so a renamed catalog key is a one-line change in lockstep with the
//! bridge, and the cross-check tests below fail the build if a key drops out of
//! the catalog.
//!
//! These keys reach C# two ways, both single-sourced here:
//!  - structured wire enums (below) serialize a `kind` tag the C# DTO switches
//!    on, and the DTO calls `bae_*_key` (exported in `lib.rs`) for its key;
//!  - the exported `bae_*_key` C-ABI functions return the key string directly
//!    for the cases the C# can't reconstruct (cloud provider, audio channels).

use bae_core::album_detail::TrackPosition;
use serde::Serialize;

// ── Track position ──────────────────────────────────────────────────────
//
// Structured position mirroring `BridgeTrackPosition`. The C# composes the
// position string ("A1"/"2-3"/"5") mechanically from the fields and resolves
// the side/disc header word from the `Side` group's catalog key. No prose
// crosses the bridge — only the side letter, disc/track numbers, and the case.

/// Wire mirror of `bae_core::album_detail::TrackPosition` (and the bridge's
/// `BridgeTrackPosition`). `kind` tags the case; only that case's fields are
/// set. The C# renders "A1" from `Sided`, "2-3" from `Disc`, "5" from `Flat`.
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FfiTrackPosition {
    /// Vinyl/cassette: position "{side_letter}{number}" (e.g. "A1").
    Sided { side_letter: String, number: i32 },
    /// Multi-disc digital: position "{disc}-{number}" (e.g. "2-3").
    Disc { disc: i32, number: i32 },
    /// Single-disc digital: position "{number}" (e.g. "5").
    Flat { number: i32 },
}

impl FfiTrackPosition {
    pub fn from_core(p: &TrackPosition) -> Self {
        match p {
            TrackPosition::Sided {
                side_letter,
                number,
            } => Self::Sided {
                side_letter: side_letter.clone(),
                number: *number,
            },
            TrackPosition::Disc { disc, number } => Self::Disc {
                disc: *disc,
                number: *number,
            },
            TrackPosition::Flat { number } => Self::Flat { number: *number },
        }
    }
}

// ── Audio format ──────────────────────────────────────────────────────────
//
// Structured audio format mirroring `BridgeAudioFormat`. The C# composes the
// one-line descriptor ("FLAC · 44.1 kHz · 16-bit · stereo") from the parts: the
// codec is a proper noun, numbers format per locale, and the channel count maps
// to a localized word via `bae_audio_channels_key`.

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

/// Catalog key for a channel count's word ("mono"/"stereo"), or `None` for
/// counts the C# renders as "{n}ch". Mirrors `bridge_audio_channels_key`.
pub fn audio_channels_key(channels: i64) -> Option<&'static str> {
    match channels {
        1 => Some("core.audio.channels.mono"),
        2 => Some("core.audio.channels.stereo"),
        _ => None,
    }
}

// ── Cloud provider ──────────────────────────────────────────────────────────

/// Catalog key for a cloud provider's display name, or `None` for the
/// brand-name providers the UI passes through verbatim (iCloud, Google Drive,
/// Dropbox, OneDrive). An empty/`None` wire tag means local-only. Mirrors
/// `bridge_cloud_provider_label_key`. `provider` is the wire tag from
/// `cloud_provider_name` ("s3"/"google_drive"/"dropbox"/"onedrive"/"cloudkit")
/// or `None`/`""` for local-only.
pub fn cloud_provider_label_key(provider: Option<&str>) -> Option<&'static str> {
    match provider {
        None | Some("") => Some("core.cloud.local_only"),
        Some("s3") => Some("core.cloud.s3_compatible"),
        // Brand names pass through (the UI hardcodes their display names).
        Some(_) => None,
    }
}

// ── Lookup failure ──────────────────────────────────────────────────────────
//
// Structured metadata-lookup failure mirroring `BridgeLookupFailure`. The C#
// resolves a localized line per variant (`failure_key`) and renders
// `Provider`'s status as the message argument. `Diagnostic` carries opaque,
// log-only detail — never translated, never shown as primary copy.

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
/// `kind` tags the variant. `not_found` carries the entity wire tag the C#
/// resolves via `bae_entity_not_found_key`; `diagnostic` carries the category
/// wire tag the C# resolves via `bae_error_category_key` plus the opaque detail.
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

/// Wire tag for a diagnostic error category. The C# maps it to its catalog key
/// via `error_category_key` (single source below).
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

/// Wire tag for a missing-entity kind. The C# maps it to its catalog key via
/// `entity_not_found_key` (single source below).
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

/// Catalog key for a diagnostic error category's generic line. Mirrors
/// `bridge_error_category_key`. `category` is the wire tag `FfiError` carries.
pub fn error_category_key(category: &str) -> Option<&'static str> {
    match category {
        "database" => Some("core.error.category.database"),
        "config" => Some("core.error.category.config"),
        "internal" => Some("core.error.category.internal"),
        "import" => Some("core.error.category.import"),
        "export" => Some("core.error.category.export"),
        _ => None,
    }
}

/// Catalog key for a missing-entity's "… not found" line. Mirrors
/// `bridge_entity_not_found_key`. `entity` is the wire tag `FfiError` carries.
pub fn entity_not_found_key(entity: &str) -> Option<&'static str> {
    match entity {
        "library" => Some("core.error.not_found.library"),
        "album" => Some("core.error.not_found.album"),
        "release" => Some("core.error.not_found.release"),
        "track" => Some("core.error.not_found.track"),
        "file" => Some("core.error.not_found.file"),
        _ => None,
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

/// Catalog key for an actionable playback reason, or `None` for `diagnostic`
/// (the C# renders that through the `FfiError` category path). Mirrors
/// `bridge_playback_error_reason_key`. `kind` is the wire tag.
pub fn playback_error_reason_key(kind: &str) -> Option<&'static str> {
    match kind {
        "sync_disconnected" => Some("core.playback.error.sync_disconnected"),
        "upload_pending" => Some("core.playback.error.upload_pending"),
        _ => None,
    }
}

// ── Import step ──────────────────────────────────────────────────────────────
//
// Structured import-progress step mirroring `BridgeImportStep`
// (`BridgePrepareStep`/`BridgeImportPhase`). The C# resolves the localized verb
// from the key; no English prose crosses the bridge.

/// Wire mirror of `bae_core::import::ImportStep`. `kind` tags whether it's a
/// prepare step or a running phase; `step`/`phase` is the inner wire tag the C#
/// resolves via `import_step_key`.
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
                    ImportPhase::Acquire => "acquire",
                    ImportPhase::Store => "store",
                },
            },
        }
    }
}

/// Catalog key for an import prepare-step wire tag. Mirrors
/// `bridge_prepare_step_key`.
pub fn prepare_step_key(step: &str) -> Option<&'static str> {
    match step {
        "parsing_metadata" => Some("core.import.prepare.parsing_metadata"),
        "writing_cover_art" => Some("core.import.prepare.writing_cover_art"),
        "discovering_files" => Some("core.import.prepare.discovering_files"),
        "validating_tracks" => Some("core.import.prepare.validating_tracks"),
        "saving_to_database" => Some("core.import.prepare.saving_to_database"),
        _ => None,
    }
}

/// Catalog key for an import-phase wire tag. Mirrors `bridge_import_phase_key`.
pub fn import_phase_key(phase: &str) -> Option<&'static str> {
    match phase {
        "acquire" => Some("core.import.phase.acquire"),
        "store" => Some("core.import.phase.store"),
        _ => None,
    }
}

// ── Storage state + transfer action ─────────────────────────────────────────

/// Catalog key for a transfer action's present-continuous progress verb. The
/// `action` is the wire tag `FfiStorageRow.actions` carries. Mirrors
/// `bridge_transfer_action_key`.
pub fn transfer_action_key(action: &str) -> Option<&'static str> {
    match action {
        "pin" => Some("core.transfer.action.pin"),
        "unpin" => Some("core.transfer.action.unpin"),
        "manage" => Some("core.transfer.action.manage"),
        "unmanage" => Some("core.transfer.action.unmanage"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn audio_channels_keys_exist() {
        let cat = catalog();
        assert_key(&cat, audio_channels_key(1).expect("mono"));
        assert_key(&cat, audio_channels_key(2).expect("stereo"));
        assert!(
            audio_channels_key(6).is_none(),
            "5.1 has no word, renders Nch"
        );
    }

    #[test]
    fn cloud_provider_keys_exist() {
        let cat = catalog();
        assert_key(&cat, cloud_provider_label_key(None).expect("local only"));
        assert_key(
            &cat,
            cloud_provider_label_key(Some("")).expect("local only"),
        );
        assert_key(&cat, cloud_provider_label_key(Some("s3")).expect("s3"));
        for brand in ["google_drive", "dropbox", "onedrive", "cloudkit"] {
            assert!(
                cloud_provider_label_key(Some(brand)).is_none(),
                "brand names pass through: {brand}"
            );
        }
    }

    #[test]
    fn error_keys_exist() {
        let cat = catalog();
        for category in ["database", "config", "internal", "import", "export"] {
            assert_key(&cat, error_category_key(category).expect("category keyed"));
        }
        for entity in ["library", "album", "release", "track", "file"] {
            assert_key(&cat, entity_not_found_key(entity).expect("entity keyed"));
        }
        assert!(error_category_key("nope").is_none());
        assert!(entity_not_found_key("nope").is_none());
    }

    #[test]
    fn playback_reason_keys_exist() {
        let cat = catalog();
        assert_key(
            &cat,
            playback_error_reason_key("sync_disconnected").expect("actionable"),
        );
        assert_key(
            &cat,
            playback_error_reason_key("upload_pending").expect("actionable"),
        );
        assert!(
            playback_error_reason_key("diagnostic").is_none(),
            "diagnostic renders through the FfiError category path"
        );
    }

    #[test]
    fn import_step_keys_exist() {
        let cat = catalog();
        for step in [
            "parsing_metadata",
            "writing_cover_art",
            "discovering_files",
            "validating_tracks",
            "saving_to_database",
        ] {
            assert_key(&cat, prepare_step_key(step).expect("prepare step keyed"));
        }
        for phase in ["acquire", "store"] {
            assert_key(&cat, import_phase_key(phase).expect("phase keyed"));
        }
    }

    #[test]
    fn transfer_action_keys_exist() {
        let cat = catalog();
        for action in ["pin", "unpin", "manage", "unmanage"] {
            assert_key(&cat, transfer_action_key(action).expect("action keyed"));
        }
    }

    /// The structured-composition keys the C# resolves (side/disc headers,
    /// queue counts, outbox/transfer args, lookup failures, pressings plural)
    /// must all exist — these have no `*_key` fn (the C# hardcodes the dotted
    /// key for them), so this is their cross-check.
    #[test]
    fn composition_keys_exist() {
        let cat = catalog();
        for key in [
            "core.track.side",
            "core.track.disc",
            "core.import.pressings",
            "core.lookup.failure.network",
            "core.lookup.failure.provider",
            "core.lookup.failure.provider_unknown",
            "core.lookup.failure.timeout",
            "core.lookup.failure.diagnostic",
            "core.queue.uploading",
            "core.queue.downloading",
            "core.queue.failed",
            "core.queue.queued",
            "core.outbox.bytes_progress",
            "core.outbox.throughput",
            "core.outbox.eta",
            "core.outbox.pending_deletes",
            "core.transfer.files",
        ] {
            assert_key(&cat, key);
        }
    }
}
