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
    let event = TelemetryEvent::PlaybackFailed { operation: PlaybackOperation::Preload };
    assert_eq!(event.level(), DiagnosticLevel::Error);
    assert_eq!(event.fields()["operation"], serde_json::json!("preload"));
}

#[test]
fn cloud_provider_records_provider_key() {
    let event = TelemetryEvent::SyncFailed {
        provider: CloudProvider::OneDrive,
        operation: SyncOperation::Cycle,
    };
    let fields = event.fields();
    assert_eq!(fields["provider"], serde_json::json!("onedrive"));
    assert_eq!(fields["operation"], serde_json::json!("cycle"));
}

#[test]
fn playback_command_records_kind_as_snake_case() {
    let event =
        TelemetryEvent::PlaybackCommand { command: PlaybackCommandKind::PlayLibraryShuffled };
    assert_eq!(event.name(), "playback_command");
    assert_eq!(event.level(), DiagnosticLevel::Info);
    assert_eq!(event.fields()["command"], serde_json::json!("play_library_shuffled"));
}

#[test]
fn track_completed_carries_decode_error_count_as_number() {
    let event = TelemetryEvent::TrackCompleted {
        track_id: LocalId("a29b9a29-8c0c-4aee-84c8-0af69eb9ea1e".to_string()),
        decode_errors: 3,
    };
    let fields = event.fields();
    assert_eq!(fields["track_id"], serde_json::json!("a29b9a29-8c0c-4aee-84c8-0af69eb9ea1e"));
    assert_eq!(fields["decode_errors"], serde_json::json!(3));
    assert!(fields["decode_errors"].is_number());
}

#[test]
fn anomaly_is_warn_with_snake_case_kind() {
    let event = TelemetryEvent::Anomaly { kind: AnomalyKind::ResumePersistFailed };
    assert_eq!(event.name(), "anomaly");
    assert_eq!(event.level(), DiagnosticLevel::Warn);
    assert_eq!(event.fields()["kind"], serde_json::json!("resume_persist_failed"));
}

#[test]
fn storage_transfer_records_action_via_foreign_impl() {
    let event = TelemetryEvent::StorageTransferCompleted {
        action: ReleaseStorageAction::Upload,
        release_id: LocalId("e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e".to_string()),
        file_count: 12,
    };
    let fields = event.fields();
    assert_eq!(fields["action"], serde_json::json!("upload"));
    assert_eq!(fields["release_id"], serde_json::json!("e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e"));
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
fn app_start_failed_is_error_with_snake_case_kind() {
    let event = TelemetryEvent::AppStartFailed { kind: AppStartFailureKind::LibraryNotFound };
    assert_eq!(event.name(), "app_start_failed");
    assert_eq!(event.level(), DiagnosticLevel::Error);
    assert_eq!(event.fields()["kind"], serde_json::json!("library_not_found"));
}

#[test]
fn keyring_init_failed_is_field_less_error() {
    let event = TelemetryEvent::KeyringInitFailed {};
    assert_eq!(event.name(), "keyring_init_failed");
    assert_eq!(event.level(), DiagnosticLevel::Error);
    assert!(event.fields().is_empty());
}

#[test]
fn export_completed_and_failed_carry_release_and_differ_by_level() {
    let completed = TelemetryEvent::ExportCompleted { release_id: LocalId("rel-7".to_string()) };
    assert_eq!(completed.name(), "export_completed");
    assert_eq!(completed.level(), DiagnosticLevel::Info);
    assert_eq!(completed.fields()["release_id"], serde_json::json!("rel-7"));

    let failed = TelemetryEvent::ExportFailed { release_id: LocalId("rel-7".to_string()) };
    assert_eq!(failed.name(), "export_failed");
    assert_eq!(failed.level(), DiagnosticLevel::Error);
    assert_eq!(failed.fields()["release_id"], serde_json::json!("rel-7"));
}

#[test]
fn save_completed_and_failed_carry_release_and_differ_by_level() {
    let completed = TelemetryEvent::SaveCompleted { release_id: LocalId("rel-9".to_string()) };
    assert_eq!(completed.name(), "save_completed");
    assert_eq!(completed.level(), DiagnosticLevel::Info);
    assert_eq!(completed.fields()["release_id"], serde_json::json!("rel-9"));

    let failed = TelemetryEvent::SaveFailed { release_id: LocalId("rel-9".to_string()) };
    assert_eq!(failed.name(), "save_failed");
    assert_eq!(failed.level(), DiagnosticLevel::Error);
    assert_eq!(failed.fields()["release_id"], serde_json::json!("rel-9"));
}

#[test]
fn cloud_provider_connected_records_provider() {
    let connected = TelemetryEvent::CloudProviderConnected { provider: CloudProvider::Dropbox };
    assert_eq!(connected.name(), "cloud_provider_connected");
    assert_eq!(connected.level(), DiagnosticLevel::Info);
    assert_eq!(connected.fields()["provider"], serde_json::json!("dropbox"));
}

#[test]
fn storage_transfer_failed_is_error_with_action_and_release() {
    let event = TelemetryEvent::StorageTransferFailed {
        action: ReleaseStorageAction::Unpin,
        release_id: LocalId("rel-3".to_string()),
    };
    assert_eq!(event.name(), "storage_transfer_failed");
    assert_eq!(event.level(), DiagnosticLevel::Error);
    let fields = event.fields();
    assert_eq!(fields["action"], serde_json::json!("unpin"));
    assert_eq!(fields["release_id"], serde_json::json!("rel-3"));
}

#[test]
fn seek_completed_wait_records_whole_milliseconds() {
    let event = TelemetryEvent::SeekCompleted {
        track_id: LocalId("track-4".to_string()),
        wait: Duration::from_millis(320),
    };
    assert_eq!(event.name(), "seek_completed");
    assert_eq!(event.level(), DiagnosticLevel::Info);
    let fields = event.fields();
    assert_eq!(fields["track_id"], serde_json::json!("track-4"));
    assert_eq!(fields["wait"], serde_json::json!(320));
    assert!(fields["wait"].is_number());
}

#[test]
fn playback_starved_is_warn_with_track_time_as_number() {
    let event = TelemetryEvent::PlaybackStarved {
        track_id: LocalId("track-5".to_string()),
        track_time: bae_types::track_time::TrackTime::from_millis(-1_500),
    };
    assert_eq!(event.name(), "playback_starved");
    assert_eq!(event.level(), DiagnosticLevel::Warn);
    let fields = event.fields();
    assert_eq!(fields["track_id"], serde_json::json!("track-5"));
    assert_eq!(fields["track_time"], serde_json::json!(-1_500));
    assert!(fields["track_time"].is_number());
}
