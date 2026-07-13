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

use crate::album_detail::ReleaseStorageAction;
use crate::config::CloudProvider;
use crate::diagnostics::DiagnosticLevel;

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

/// Declare a closed, field-less enum and implement [`TelemetryValue`] for it,
/// recording each variant as an explicit snake_case string. The impl-only arm
/// wires a foreign type (e.g. coven's `CloudProvider`) whose definition lives
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
        Starvation => "starvation",
        DeviceRebuild => "device_rebuild",
        StreamStart => "stream_start",
        Read => "read",
        Preview => "preview",
    }
}

telemetry_value_enum! {
    /// The sync operation whose failure produced a `SyncFailed` event. Today the
    /// only place sync errors surface as library events is the sync-loop status
    /// listener, so `Cycle` is the only emitting site.
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
        PlayLibraryShuffled => "play_library_shuffled",
        Next => "next",
        Previous => "previous",
        Seek => "seek",
        Pause => "pause",
        Resume => "resume",
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
        ResumeCacheCorrupt => "resume_cache_corrupt",
        ResumePersistFailed => "resume_persist_failed",
        GaplessBoundaryDesync => "gapless_boundary_desync",
        SidePauseDesync => "side_pause_desync",
        PreloadStateMissing => "preload_state_missing",
        QueueEntryUnknown => "queue_entry_unknown",
        QueueTrackNoMetadata => "queue_track_no_metadata",
        ChangesetMissingFk => "changeset_missing_fk",
        AudioFormatOrphaned => "audio_format_orphaned",
        BlobIdInvalid => "blob_id_invalid",
        EncryptionKeyMissing => "encryption_key_missing",
        EventBusLagged => "event_bus_lagged",
        ImportErrorWriteFailed => "import_error_write_failed",
    }
}

telemetry_value_enum! {
    impl ReleaseStorageAction {
        ReleaseStorageAction::MakeRemote => "make_remote",
        ReleaseStorageAction::Pin => "pin",
        ReleaseStorageAction::Unpin => "unpin",
        ReleaseStorageAction::MakeLocal => "make_local",
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
    impl CloudProvider {
        CloudProvider::S3 => "s3",
        CloudProvider::GoogleDrive => "google_drive",
        CloudProvider::Dropbox => "dropbox",
        CloudProvider::OneDrive => "onedrive",
        CloudProvider::CloudKit => "cloudkit",
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

    /// A cloud provider connection was configured and started.
    CloudProviderConnected, "cloud_provider_connected", Info {
        provider: CloudProvider,
    },

    /// A cloud provider was disconnected.
    CloudProviderDisconnected, "cloud_provider_disconnected", Info {
        provider: CloudProvider,
    },

    /// A release storage transition (pin/unpin/make-remote/make-local) completed.
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

    /// Playback starvation onset: the decoder couldn't keep the ring fed.
    PlaybackStarved, "playback_starved", Warn {
        track_id: LocalId,
        position_ms: u64,
    },

    /// A track drained to natural completion. `decode_errors` is the quality
    /// signal — a clean track reports zero.
    TrackCompleted, "track_completed", Info {
        track_id: LocalId,
        decode_errors: u64,
    },

    /// Keyring store initialization failed (runs before diagnostics exists; the
    /// outcome is read back and shipped at bootstrap).
    KeyringInitFailed, "keyring_init_failed", Error {},

    /// An impossible-state site fired. `kind` names which one; the count per
    /// version is the proactive-bug-detection signal.
    Anomaly, "anomaly", Warn {
        kind: AnomalyKind,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_started_has_no_fields() {
        let event = TelemetryEvent::AppStarted {};
        assert_eq!(event.name(), "app_started");
        assert_eq!(event.level(), DiagnosticLevel::Info);
        assert!(event.fields().is_empty());
    }

    #[test]
    fn numeric_and_enum_fields_record_by_type() {
        let event = TelemetryEvent::PlaybackStarted {
            source: PlaybackStartSource::LibraryShuffled,
            track_count: 12,
        };
        let fields = event.fields();
        // Enum → snake_case string, number → JSON number (not a string).
        assert_eq!(fields["source"], serde_json::json!("library_shuffled"));
        assert_eq!(fields["track_count"], serde_json::json!(12));
        assert!(fields["track_count"].is_number());
    }

    #[test]
    fn local_id_records_the_raw_string() {
        let event = TelemetryEvent::TrackStarted {
            track_id: LocalId("track-123".to_string()),
            transition: TrackTransition::Gapless,
        };
        let fields = event.fields();
        assert_eq!(fields["track_id"], serde_json::json!("track-123"));
        assert_eq!(fields["transition"], serde_json::json!("gapless"));
    }

    #[test]
    fn duration_records_whole_milliseconds_as_a_number() {
        let event = TelemetryEvent::ImportCompleted {
            track_count: 3,
            duration_ms: Duration::from_millis(4500),
        };
        let fields = event.fields();
        assert_eq!(fields["duration_ms"], serde_json::json!(4500));
        assert!(fields["duration_ms"].is_number());
    }

    #[test]
    fn error_level_events_declare_error() {
        let event = TelemetryEvent::PlaybackFailed {
            operation: PlaybackOperation::Preload,
        };
        assert_eq!(event.level(), DiagnosticLevel::Error);
        assert_eq!(event.fields()["operation"], serde_json::json!("preload"));
    }

    #[test]
    fn cloud_provider_records_snake_case() {
        let event = TelemetryEvent::SyncFailed {
            provider: CloudProvider::GoogleDrive,
            operation: SyncOperation::Cycle,
        };
        let fields = event.fields();
        assert_eq!(fields["provider"], serde_json::json!("google_drive"));
        assert_eq!(fields["operation"], serde_json::json!("cycle"));
    }

    #[test]
    fn playback_command_records_kind_as_snake_case() {
        let event = TelemetryEvent::PlaybackCommand {
            command: PlaybackCommandKind::PlayLibraryShuffled,
        };
        assert_eq!(event.name(), "playback_command");
        assert_eq!(event.level(), DiagnosticLevel::Info);
        assert_eq!(
            event.fields()["command"],
            serde_json::json!("play_library_shuffled")
        );
    }

    #[test]
    fn track_completed_carries_decode_error_count_as_number() {
        let event = TelemetryEvent::TrackCompleted {
            track_id: LocalId("track-9".to_string()),
            decode_errors: 3,
        };
        let fields = event.fields();
        assert_eq!(fields["track_id"], serde_json::json!("track-9"));
        assert_eq!(fields["decode_errors"], serde_json::json!(3));
        assert!(fields["decode_errors"].is_number());
    }

    #[test]
    fn anomaly_is_warn_with_snake_case_kind() {
        let event = TelemetryEvent::Anomaly {
            kind: AnomalyKind::ResumeCacheCorrupt,
        };
        assert_eq!(event.name(), "anomaly");
        assert_eq!(event.level(), DiagnosticLevel::Warn);
        assert_eq!(
            event.fields()["kind"],
            serde_json::json!("resume_cache_corrupt")
        );
    }

    #[test]
    fn storage_transfer_records_action_via_foreign_impl() {
        let event = TelemetryEvent::StorageTransferCompleted {
            action: ReleaseStorageAction::MakeRemote,
            release_id: LocalId("rel-1".to_string()),
            file_count: 12,
        };
        let fields = event.fields();
        assert_eq!(fields["action"], serde_json::json!("make_remote"));
        assert_eq!(fields["release_id"], serde_json::json!("rel-1"));
        assert_eq!(fields["file_count"], serde_json::json!(12));
        assert!(fields["file_count"].is_number());
    }

    #[test]
    fn first_audio_wait_records_whole_milliseconds() {
        let event = TelemetryEvent::FirstAudio {
            track_id: LocalId("track-1".to_string()),
            wait: Duration::from_millis(850),
        };
        let fields = event.fields();
        assert_eq!(fields["wait"], serde_json::json!(850));
        assert!(fields["wait"].is_number());
    }

    #[test]
    fn keyring_init_failed_is_field_less_error() {
        let event = TelemetryEvent::KeyringInitFailed {};
        assert_eq!(event.name(), "keyring_init_failed");
        assert_eq!(event.level(), DiagnosticLevel::Error);
        assert!(event.fields().is_empty());
    }
}
