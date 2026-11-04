//! The closed, typed catalog of telemetry events bae ships to Datadog.
//!
//! Everything that leaves the device is one of the `TelemetryEvent` variants
//! declared below. A variant cannot exist without a declared name, level, and
//! typed fields, and a field cannot exist unless its type implements
//! [`TelemetryValue`] — which `String`/`&str` deliberately do not. Free text,
//! names, titles, paths, and externally-resolvable ids (MusicBrainz, Discogs)
//! therefore cannot enter the payload by construction: there is no
//! stringly-typed field to put them in and no free-text path to Datadog. Local
//! `tracing` logs keep the full detail; only this catalog ships.

use std::time::Duration;

use crate::DiagnosticLevel;
use bae_types::release_storage::ReleaseStorageAction;
use coven::CloudProvider;

/// A value allowed to ship in a telemetry event. Deliberately NOT implemented
/// for `String`/`&str`: free text, names, paths, and externally-resolvable ids
/// (MusicBrainz, Discogs) must not enter the payload. Closed field-less enums
/// implement it via `telemetry_value_enum!`; DB-generated random ids ship via
/// [`LocalId`].
pub trait TelemetryValue {
    fn record(&self) -> serde_json::Value;
}

/// An id randomly generated in this library's own DB (library, release, track,
/// queue entry). It resolves to nothing outside the user's own database — the
/// one string-shaped thing telemetry may carry, and what we actually debug
/// with. Never wrap an external id (MusicBrainz, Discogs), a name, or a path:
/// those identify real-world entities and must stay in local logs only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalId(pub String);

impl TelemetryValue for LocalId {
    fn record(&self) -> serde_json::Value {
        serde_json::Value::String(self.0.clone())
    }
}

macro_rules! telemetry_value_primitive {
    ($($t:ty),* $(,)?) => {
        $(
            impl TelemetryValue for $t {
                fn record(&self) -> serde_json::Value {
                    serde_json::Value::from(*self)
                }
            }
        )*
    };
}

// Numbers ship as JSON numbers (Datadog measures), booleans as JSON booleans.
telemetry_value_primitive!(u8, u16, u32, u64, usize, bool);

impl TelemetryValue for Duration {
    fn record(&self) -> serde_json::Value {
        // Whole milliseconds, as a JSON number.
        serde_json::Value::from(self.as_millis() as u64)
    }
}

impl TelemetryValue for bae_types::track_time::TrackTime {
    fn record(&self) -> serde_json::Value {
        // Milliseconds from the track's start, as a JSON number; negative in
        // the pregap.
        serde_json::Value::from(self.as_millis())
    }
}

/// Declare a closed, field-less enum and implement [`TelemetryValue`] for it,
/// recording each variant as an explicit snake_case string. The impl-only arm
/// wires an existing type (e.g. `ReleaseStorageAction`) whose definition lives
/// elsewhere — the local trait makes that legal here.
macro_rules! telemetry_value_enum {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $( $variant:ident => $json:literal ),* $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        $vis enum $name {
            $( $variant ),*
        }

        impl TelemetryValue for $name {
            fn record(&self) -> serde_json::Value {
                serde_json::Value::String(
                    match self {
                        $( Self::$variant => $json ),*
                    }
                    .to_string(),
                )
            }
        }
    };
    (
        impl $name:ty {
            $( $variant:path => $json:literal ),* $(,)?
        }
    ) => {
        impl TelemetryValue for $name {
            fn record(&self) -> serde_json::Value {
                serde_json::Value::String(
                    match self {
                        $( $variant => $json ),*
                    }
                    .to_string(),
                )
            }
        }
    };
}

telemetry_value_enum! {
    /// How a play command established a new playing context.
    pub enum PlaybackStartSource {
        Release => "release",
        Releases => "releases",
        Tracks => "tracks",
        LibraryShuffled => "library_shuffled",
        Restored => "restored",
    }
}

telemetry_value_enum! {
    /// Why a track began playing.
    pub enum TrackTransition {
        Manual => "manual",
        AutoAdvance => "auto_advance",
        Gapless => "gapless",
        Repeat => "repeat",
    }
}

telemetry_value_enum! {
    /// The playback operation whose failure produced a `PlaybackFailed` event.
    /// Every variant has an emitting call site in the playback service.
    pub enum PlaybackOperation {
        LoadContext => "load_context",
        Preload => "preload",
        AudioOutputInit => "audio_output_init",
        QueueAdd => "queue_add",
        QueueChange => "queue_change",
        Starvation => "starvation",
        DeviceRebuild => "device_rebuild",
        StreamStart => "stream_start",
        Read => "read",
        Preview => "preview",
    }
}

telemetry_value_enum! {
    /// The sync operation whose failure produced a `SyncFailed` telemetry event.
    /// The sync-loop status listener is the only emitting site, so `Cycle` is
    /// the only variant.
    pub enum SyncOperation {
        Cycle => "cycle",
    }
}

telemetry_value_enum! {
    /// A user-intent playback command, reported once when the command loop picks
    /// it up. Only commands a person issues map to a variant; internal/system
    /// commands (auto-advance, shutdown), pure queries (get volume/queue), and
    /// continuous inputs (volume, position ticks) have none and ship nothing.
    pub enum PlaybackCommandKind {
        Play => "play",
        PlayRelease => "play_release",
        PlayReleases => "play_releases",
        PlayTracks => "play_tracks",
        PlayLibraryShuffled => "play_library_shuffled",
        Next => "next",
        Previous => "previous",
        Seek => "seek",
        Pause => "pause",
        Resume => "resume",
        CancelSidePauseCountdown => "cancel_side_pause_countdown",
        Stop => "stop",
        SetShuffle => "set_shuffle",
        SetRepeat => "set_repeat",
        AddReleaseToQueue => "add_release_to_queue",
        AddReleaseNext => "add_release_next",
        RemoveFromQueue => "remove_from_queue",
        ReorderQueue => "reorder_queue",
        SkipTo => "skip_to",
    }
}

telemetry_value_enum! {
    /// One impossible-state site class. Each variant is a distinct kind of
    /// "should never happen" the code detected; the local `warn!`/`error!` next
    /// to the emission keeps the free-text detail, this counts the occurrence per
    /// version so regressions surface in the wild.
    pub enum AnomalyKind {
        ResumePersistFailed => "resume_persist_failed",
        GaplessBoundaryDesync => "gapless_boundary_desync",
        SidePauseDesync => "side_pause_desync",
        PreloadStateMissing => "preload_state_missing",
        QueueEntryUnknown => "queue_entry_unknown",
        EncryptionKeyMissing => "encryption_key_missing",
    }
}

impl TelemetryValue for ReleaseStorageAction {
    fn record(&self) -> serde_json::Value {
        serde_json::Value::String(self.key().to_string())
    }
}

telemetry_value_enum! {
    /// A host UI screen the platform apps report opening.
    pub enum Screen {
        Library => "library",
        Settings => "settings",
    }
}

telemetry_value_enum! {
    /// Which bootstrap step failed, mirroring `BootstrapError`'s variants. The
    /// error's own message stays in local logs; only this closed kind ships.
    pub enum AppStartFailureKind {
        LibraryNotFound => "library_not_found",
        Config => "config",
        Database => "database",
        Internal => "internal",
        KeyringUnavailable => "keyring_unavailable",
        KeychainEntitlementMissing => "keychain_entitlement_missing",
        ReplacementUnfinished => "replacement_unfinished",
    }
}

impl TelemetryValue for CloudProvider {
    fn record(&self) -> serde_json::Value {
        let value = match self {
            CloudProvider::S3 => "s3",
            CloudProvider::Dropbox => "dropbox",
            CloudProvider::OneDrive => "onedrive",
            CloudProvider::CloudKit => "cloudkit",
        };
        serde_json::Value::String(value.to_string())
    }
}

/// Declare the closed telemetry catalog. Each entry names a variant, its wire
/// name, its level, and its typed fields. A field's type must implement
/// [`TelemetryValue`] or this fails to compile — the enforcement point that
/// keeps free text out of the payload.
macro_rules! telemetry_events {
    (
        $(
            $(#[$meta:meta])*
            $variant:ident, $name:literal, $level:ident { $( $field:ident : $ty:ty ),* $(,)? }
        ),* $(,)?
    ) => {
        /// The closed set of events bae ships. Construct a variant and hand it
        /// to `Diagnostics::event`; there is no other path to Datadog.
        #[derive(Clone, Debug, PartialEq)]
        pub enum TelemetryEvent {
            $(
                $(#[$meta])*
                $variant { $( $field : $ty ),* },
            )*
        }

        impl TelemetryEvent {
            /// The declared wire name — Datadog's message/display line.
            pub fn name(&self) -> &'static str {
                match self {
                    $( Self::$variant { .. } => $name, )*
                }
            }

            /// The declared severity.
            pub fn level(&self) -> DiagnosticLevel {
                match self {
                    $( Self::$variant { .. } => DiagnosticLevel::$level, )*
                }
            }

            /// Every field recorded through [`TelemetryValue::record`] — the
            /// point that enforces the typed-value contract.
            pub fn fields(&self) -> serde_json::Map<String, serde_json::Value> {
                match self {
                    $(
                        Self::$variant { $( $field ),* } => {
                            #[allow(unused_mut)]
                            let mut map = serde_json::Map::new();
                            $(
                                map.insert(
                                    stringify!($field).to_string(),
                                    TelemetryValue::record($field),
                                );
                            )*
                            map
                        }
                    )*
                }
            }
        }
    };
}

telemetry_events! {
    /// Once per successful bootstrap.
    AppStarted, "app_started", Info {},

    /// Bootstrap failed to bring the app up (the library won't open). Emitted by
    /// the bridge, which holds the telemetry sink before `init_app` runs.
    AppStartFailed, "app_start_failed", Error {
        kind: AppStartFailureKind,
    },

    /// A play command established a new playing context.
    PlaybackStarted, "playback_started", Info {
        source: PlaybackStartSource,
        track_count: u32,
    },

    /// A track start was initiated: the playing context is established and
    /// playback was requested. Emitted at that point, not when audio is
    /// confirmed flowing — a start whose track then fails to prepare or open
    /// its output still ships this, and that failure ships separately as
    /// `playback_failed`.
    TrackStarted, "track_started", Info {
        track_id: LocalId,
        transition: TrackTransition,
    },

    /// A playback operation failed (the free-text error stays in local logs).
    PlaybackFailed, "playback_failed", Error {
        operation: PlaybackOperation,
    },

    /// An import finished writing its release (desktop only).
    ImportCompleted, "import_completed", Info {
        track_count: u32,
        duration_ms: Duration,
    },

    /// An import failed (desktop only).
    ImportFailed, "import_failed", Error {},

    /// A sync cycle surfaced an error.
    SyncFailed, "sync_failed", Error {
        provider: CloudProvider,
        operation: SyncOperation,
    },

    /// A host UI screen was opened (emitted by the platform apps).
    ScreenOpened, "screen_opened", Info {
        screen: Screen,
    },

    /// A user-intent playback command reached the command loop.
    PlaybackCommand, "playback_command", Info {
        command: PlaybackCommandKind,
    },

    /// A release finished exporting its files to a user folder (desktop only).
    ExportCompleted, "export_completed", Info {
        release_id: LocalId,
    },

    /// A release export failed or its task panicked (desktop only).
    ExportFailed, "export_failed", Error {
        release_id: LocalId,
    },

    /// A release finished saving its tracks to a user folder (desktop only).
    SaveCompleted, "save_completed", Info {
        release_id: LocalId,
    },

    /// A release save failed or its task panicked (desktop only).
    SaveFailed, "save_failed", Error {
        release_id: LocalId,
    },

    /// A cloud provider connection was configured and started.
    CloudProviderConnected, "cloud_provider_connected", Info {
        provider: CloudProvider,
    },

    /// A release storage transition (pin/unpin/upload/detach-audio) completed.
    StorageTransferCompleted, "storage_transfer_completed", Info {
        action: ReleaseStorageAction,
        release_id: LocalId,
        file_count: u32,
    },

    /// A release storage transition failed.
    StorageTransferFailed, "storage_transfer_failed", Error {
        action: ReleaseStorageAction,
        release_id: LocalId,
    },

    /// Time from a play command's initiation to the track reaching Playing.
    FirstAudio, "first_audio", Info {
        track_id: LocalId,
        wait: Duration,
    },

    /// Time from a seek's initiation to the rebuilt stream being installed.
    SeekCompleted, "seek_completed", Info {
        track_id: LocalId,
        wait: Duration,
    },

    /// Playback starvation onset: the decoder couldn't keep the ring fed, at
    /// `track_time` into the track, as the player shows it.
    PlaybackStarved, "playback_starved", Warn {
        track_id: LocalId,
        track_time: bae_types::track_time::TrackTime,
    },

    /// A track drained to natural completion. `decode_errors` is the quality
    /// signal — a clean track reports zero.
    TrackCompleted, "track_completed", Info {
        track_id: LocalId,
        decode_errors: u64,
    },

    /// The OS keychain couldn't be opened at startup, so no library's keys
    /// can be read. Emitted by the bridge's host as it builds coven's
    /// installation.
    KeyringInitFailed, "keyring_init_failed", Error {},

    /// An impossible-state site fired. `kind` names which one; the count per
    /// version is the proactive-bug-detection signal.
    Anomaly, "anomaly", Warn {
        kind: AnomalyKind,
    },
}

#[cfg(test)]
#[path = "telemetry_tests.rs"]
mod tests;
