//! Airtight cross-check that the `core.*` localization catalog stays in sync
//! with the keys the `bridge_*_key` functions produce — in both directions:
//!
//! - `every_produced_key_exists_in_catalog`: every key a key fn can emit (plus
//!   every direct-reference key the UI uses) has a catalog entry. A renamed or
//!   dropped catalog key fails the build instead of rendering a raw key.
//! - `no_orphan_core_keys`: every `core.*` catalog entry is produced by a key
//!   fn or listed in `DIRECT_KEYS`. A catalog key no producer references is
//!   dead and must be deleted (or, if a real UI direct-reference, added to
//!   `DIRECT_KEYS`).
//!
//! Each keyed enum is covered by an explicit array of every variant AND an
//! inline exhaustive `match` with no `_` arm, so adding a variant is a compile
//! error here that forces updating the coverage.

use super::*;

/// `core.*` keys the UI references directly with its own args — not emitted
/// by any `bridge_*_key` fn. Kept in sync with the catalog by
/// `no_orphan_core_keys`.
const DIRECT_KEYS: &[&str] = &[
    // Storage queue summary (UI composes counts).
    "core.queue.uploading",
    "core.queue.downloading",
    "core.queue.output",
    "core.queue.failed",
    "core.queue.queued",
    "core.download.bytes_progress",
    // Eager-cache status records carry their localized title key directly;
    // each platform renders that field without another key function.
    "core.artwork_cache.scanning",
    "core.artwork_cache.downloading",
    "core.artwork_cache.cancelled",
    "core.artwork_cache.failed",
    "core.outbox.pending_deletes",
    "core.outbox.preparing",
    "core.outbox.prepared",
    "core.outbox.uploaded",
    "core.outbox.publishing",
    "core.outbox.cancelling",
    "core.outbox.retrying",
    "core.outbox.throughput",
    "core.outbox.eta",
    // Device-pairing cancellation has no phase enum because cancellation is
    // the command currently being awaited, not pairing progress.
    "core.pairing.cancelling",
    // Upload rows localize typed image kinds; original filenames render
    // verbatim. The cover label is one of them: no file role names a cover
    // any more — which image leads a release is the cover choice, not a
    // property of a file — so both desktops reach for this key directly.
    "core.outbox.file.artist_image",
    "core.outbox.file.unwinding",
    "core.import.role.cover",
    // Album total playing time: the UI switches on `BridgeDurationUnits` and
    // composes the hours and minutes words through the join pattern.
    "core.duration.hours",
    "core.duration.minutes",
    "core.duration.hours_minutes",
    // Release-group card pressing count.
    "core.import.pressings",
    // Disconnect-sync confirmation: releases that live only in the cloud (the
    // UI composes the count into its own base sentence).
    "core.sync.cloud_only_releases",
    // Generic lookup-failure line for the keyless `Diagnostic` variant:
    // `bridge_lookup_failure_key` returns `None`, the UI shows this line.
    "core.lookup.failure.diagnostic",
];

/// A stand-in cover choice for walking the file roles that carry one. The
/// key a role reads under never looks at it.
fn loc_cover_choice() -> BridgeCoverChoice {
    BridgeCoverChoice {
        selection: BridgeCoverSelection::ReleaseImage {
            file_id: String::new(),
        },
        preview_source: BridgeCoverImageSource::Local {
            path: String::new(),
        },
        thumbnail_source: BridgeCoverImageSource::Local {
            path: String::new(),
        },
    }
}

/// Every key the `bridge_*_key` fns can emit. For each keyed enum an
/// explicit array of all variants feeds an inline exhaustive `match` that
/// re-derives the key, asserted equal to the production fn's output — so a
/// new variant fails to compile here.
fn produced_keys() -> Vec<String> {
    let mut keys = super::device_pairing_progress_tests::progress_keys();

    // bridge_transfer_action_key — every variant carries a key.
    for a in [
        BridgeReleaseStorageAction::MakeRemote,
        BridgeReleaseStorageAction::Pin,
        BridgeReleaseStorageAction::Unpin,
        BridgeReleaseStorageAction::MakeLocal,
    ] {
        let expected = match a {
            BridgeReleaseStorageAction::MakeRemote => "core.transfer.action.make_remote",
            BridgeReleaseStorageAction::Pin => "core.transfer.action.pin",
            BridgeReleaseStorageAction::Unpin => "core.transfer.action.unpin",
            BridgeReleaseStorageAction::MakeLocal => "core.transfer.action.make_local",
        };
        assert_eq!(bridge_transfer_action_key(a), expected);
        keys.push(expected.to_string());
    }

    // bridge_sheet_refused_codec_key — one key, no variants to walk.
    keys.push(bridge_sheet_refused_codec_key());
    keys.push(bridge_sheet_refused_unreadable_key());

    // bridge_upload_phase_bytes_key — each phase names itself beside the
    // bar it labels.
    for phase in [BridgeUploadPhase::Preparing, BridgeUploadPhase::Uploading] {
        let expected = match phase {
            BridgeUploadPhase::Preparing => "core.outbox.bytes.preparing",
            BridgeUploadPhase::Uploading => "core.outbox.bytes.uploading",
        };
        assert_eq!(bridge_upload_phase_bytes_key(phase), expected);
        keys.push(expected.to_string());
    }

    // bridge_network_folder_watch_key — one key, no variants to walk.
    keys.push(bridge_network_folder_watch_key());

    // bridge_file_evidence_key — each signal that can name a file words its
    // own hover.
    for signal in [BridgeEvidenceSignal::Barcode, BridgeEvidenceSignal::DiscId] {
        let expected = match signal {
            BridgeEvidenceSignal::Barcode => "core.import.evidence.barcode_in_image",
            BridgeEvidenceSignal::DiscId => "core.import.evidence.disc_id_from_file",
        };
        let evidence = BridgeFileEvidence {
            signal,
            value: "5099969394522".to_string(),
            file_id: "Back.jpg".to_string(),
        };
        assert_eq!(bridge_file_evidence_key(&evidence), expected);
        keys.push(expected.to_string());
    }

    // bridge_file_role_key — every role the scan can propose has a name.
    for role in [
        BridgeFileRole::Audio,
        BridgeFileRole::TrackSheet {
            binding: BridgeSheetBinding::Unresolved {
                requested: Vec::new(),
            },
            track_count: 0,
        },
        BridgeFileRole::Artwork {
            choice: loc_cover_choice(),
        },
        BridgeFileRole::Document,
        BridgeFileRole::Other,
    ] {
        let expected = match role {
            BridgeFileRole::Audio => "core.import.role.audio",
            BridgeFileRole::TrackSheet { .. } => "core.import.role.track_sheet",
            BridgeFileRole::Artwork { .. } => "core.import.role.artwork",
            BridgeFileRole::Document => "core.import.role.document",
            BridgeFileRole::Other => "core.import.role.other",
        };
        assert_eq!(bridge_file_role_key(&role), expected);
        keys.push(expected.to_string());
    }

    // bridge_file_role_choice_key — the roles a person can pick between.
    for choice in [BridgeFileRoleChoice::Audio, BridgeFileRoleChoice::NotATrack] {
        let expected = match choice {
            // Deliberately the same key the Audio role reads under: the
            // picker's option and the column's label name one thing.
            BridgeFileRoleChoice::Audio => "core.import.role.audio",
            BridgeFileRoleChoice::NotATrack => "core.import.role.not_a_track",
        };
        assert_eq!(bridge_file_role_choice_key(choice), expected);
        keys.push(expected.to_string());
    }

    // bridge_file_becomes_key — one slot, a run of slots, or none. The
    // single-slot case has its own key because "slot 12" and "slots 1-11"
    // are different sentences, not one sentence with a range in it.
    for becomes in [
        BridgeFileBecomes::Slots { first: 3, last: 3 },
        BridgeFileBecomes::Slots { first: 1, last: 11 },
        BridgeFileBecomes::NoSlots,
    ] {
        let expected = match becomes {
            BridgeFileBecomes::Slots { first, last } if first == last => "core.import.becomes.slot",
            BridgeFileBecomes::Slots { .. } => "core.import.becomes.slots",
            BridgeFileBecomes::NoSlots => "core.import.becomes.not_a_track",
        };
        assert_eq!(bridge_file_becomes_key(becomes), expected);
        keys.push(expected.to_string());
    }

    // bridge_file_row_kind_key — what a collapsed directory holds.
    for kind in [BridgeFileRowKind::Document, BridgeFileRowKind::Other] {
        let expected = match kind {
            BridgeFileRowKind::Document => "core.import.files.documents",
            BridgeFileRowKind::Other => "core.import.files.other",
        };
        assert_eq!(bridge_file_row_kind_key(kind), expected);
        keys.push(expected.to_string());
    }

    // bridge_slot_reconciliation_key — the tally above the slot table.
    for reconciliation in [
        BridgeSlotReconciliation::Agrees { count: 12 },
        BridgeSlotReconciliation::MoreFiles {
            files: 13,
            tracks: 12,
        },
        BridgeSlotReconciliation::MoreTracks {
            files: 11,
            tracks: 12,
        },
    ] {
        // An agreement draws no line, so it names no key.
        let expected: Option<&str> = match reconciliation {
            BridgeSlotReconciliation::Agrees { .. } => None,
            BridgeSlotReconciliation::MoreFiles { .. } => {
                Some("core.import.reconciliation.more_files")
            }
            BridgeSlotReconciliation::MoreTracks { .. } => {
                Some("core.import.reconciliation.more_tracks")
            }
        };
        assert_eq!(
            bridge_slot_reconciliation_key(reconciliation).as_deref(),
            expected
        );
        keys.extend(expected.map(str::to_string));
    }

    // bridge_sheet_binding_offer_key — an offered file needs no reason.
    for o in [
        BridgeSheetBindingOffer::Offered,
        BridgeSheetBindingOffer::RefusedCodec {
            codec: String::new(),
        },
        BridgeSheetBindingOffer::RefusedUnreadable,
    ] {
        let expected: Option<&str> = match o {
            BridgeSheetBindingOffer::Offered => None,
            BridgeSheetBindingOffer::RefusedCodec { .. } => Some("core.import.sheet.refused_codec"),
            BridgeSheetBindingOffer::RefusedUnreadable => {
                Some("core.import.sheet.refused_unreadable")
            }
        };
        assert_eq!(bridge_sheet_binding_offer_key(o).as_deref(), expected);
        if let Some(k) = expected {
            keys.push(k.to_string());
        }
    }

    // BridgeTrackSide::header_key — Flat carries no key (None). This is what
    // BridgeTrackGroup::header_key is built from at conversion.
    for s in [
        BridgeTrackSide::Sided {
            side_letter: "A".to_string(),
        },
        BridgeTrackSide::Disc { disc: 1 },
        BridgeTrackSide::Flat,
    ] {
        let expected: Option<&str> = match s {
            BridgeTrackSide::Sided { .. } => Some("core.track.side"),
            BridgeTrackSide::Disc { .. } => Some("core.track.disc"),
            BridgeTrackSide::Flat => None,
        };
        assert_eq!(s.header_key(), expected);
        if let Some(k) = expected {
            keys.push(k.to_string());
        }
    }

    // bridge_audio_channels_key — only 1 and 2 carry words.
    for (channels, expected) in [
        (1_i64, Some("core.audio.channels.mono")),
        (2, Some("core.audio.channels.stereo")),
    ] {
        assert_eq!(bridge_audio_channels_key(channels).as_deref(), expected);
        if let Some(k) = expected {
            keys.push(k.to_string());
        }
    }

    // bridge_cloud_provider_label_key — None (local-only) and S3 carry
    // keys; the brand-name providers pass through (None).
    for p in [
        None,
        Some(BridgeCloudProvider::S3),
        Some(BridgeCloudProvider::GoogleDrive),
        Some(BridgeCloudProvider::Dropbox),
        Some(BridgeCloudProvider::OneDrive),
        Some(BridgeCloudProvider::CloudKit),
    ] {
        let expected: Option<&str> = match p {
            None => Some("core.cloud.local_only"),
            Some(BridgeCloudProvider::S3) => Some("core.cloud.s3_compatible"),
            Some(
                BridgeCloudProvider::GoogleDrive
                | BridgeCloudProvider::Dropbox
                | BridgeCloudProvider::OneDrive
                | BridgeCloudProvider::CloudKit,
            ) => None,
        };
        assert_eq!(bridge_cloud_provider_label_key(p).as_deref(), expected);
        if let Some(k) = expected {
            keys.push(k.to_string());
        }
    }

    // bridge_invalid_reason_key — every variant carries a key.
    for r in [
        BridgeInvalidReason::CorruptAudioFile {
            path: String::new(),
        },
        BridgeInvalidReason::CorruptImage {
            path: String::new(),
        },
        BridgeInvalidReason::NoValidAudio,
    ] {
        let expected = match r {
            BridgeInvalidReason::CorruptAudioFile { .. } => "core.import.invalid.corrupt_audio",
            BridgeInvalidReason::CorruptImage { .. } => "core.import.invalid.corrupt_image",
            BridgeInvalidReason::NoValidAudio => "core.import.invalid.no_valid_audio",
        };
        assert_eq!(bridge_invalid_reason_key(r.clone()), expected);
        keys.push(expected.to_string());
    }

    // bridge_needs_you_key — every variant carries a key.
    for needs_you in [
        BridgeNeedsYou::AlreadyInLibrary,
        BridgeNeedsYou::SeveralMatches { count: 0 },
        BridgeNeedsYou::NoMatch,
        BridgeNeedsYou::NothingToLookUp,
        BridgeNeedsYou::TrackCountDisagrees {
            local: 0,
            source: 0,
        },
        BridgeNeedsYou::DurationsDisagree {
            probed_ms: 0,
            source_ms: 0,
            tolerance_ms: 0,
        },
        BridgeNeedsYou::SourceLengthsUnknown,
        BridgeNeedsYou::LocalDurationUnknown,
    ] {
        let expected = match needs_you {
            BridgeNeedsYou::AlreadyInLibrary => "core.import.triage.already_in_library",
            BridgeNeedsYou::SeveralMatches { .. } => "core.import.triage.several_matches",
            BridgeNeedsYou::NoMatch => "core.import.triage.no_match",
            BridgeNeedsYou::NothingToLookUp => "core.import.triage.nothing_to_look_up",
            BridgeNeedsYou::TrackCountDisagrees { .. } => {
                "core.import.triage.track_count_disagrees"
            }
            BridgeNeedsYou::DurationsDisagree { .. } => "core.import.triage.durations_disagree",
            BridgeNeedsYou::SourceLengthsUnknown => "core.import.triage.source_lengths_unknown",
            BridgeNeedsYou::LocalDurationUnknown => "core.import.triage.local_duration_unknown",
        };
        assert_eq!(bridge_needs_you_key(&needs_you), expected);
        keys.push(expected.to_string());
    }

    // bridge_prepare_step_key — every variant carries a key.
    for step in [
        BridgePrepareStep::Queued,
        BridgePrepareStep::ReadingFolder,
        BridgePrepareStep::ParsingMetadata,
        BridgePrepareStep::WritingCoverArt,
        BridgePrepareStep::DiscoveringFiles,
        BridgePrepareStep::ValidatingTracks,
    ] {
        let expected = match step {
            BridgePrepareStep::Queued => "core.import.prepare.queued",
            BridgePrepareStep::ReadingFolder => "core.import.prepare.reading_folder",
            BridgePrepareStep::ParsingMetadata => "core.import.prepare.parsing_metadata",
            BridgePrepareStep::WritingCoverArt => "core.import.prepare.writing_cover_art",
            BridgePrepareStep::DiscoveringFiles => "core.import.prepare.discovering_files",
            BridgePrepareStep::ValidatingTracks => "core.import.prepare.validating_tracks",
        };
        assert_eq!(bridge_prepare_step_key(step), expected);
        keys.push(expected.to_string());
    }

    // bridge_import_phase_key — every variant carries a key.
    for phase in [
        BridgeImportPhase::ReadingFiles,
        BridgeImportPhase::MeasuringLoudness,
        BridgeImportPhase::Finalizing,
    ] {
        let expected = match phase {
            BridgeImportPhase::ReadingFiles => "core.import.phase.reading_files",
            BridgeImportPhase::MeasuringLoudness => "core.import.phase.measuring_loudness",
            BridgeImportPhase::Finalizing => "core.import.phase.finalizing",
        };
        assert_eq!(bridge_import_phase_key(phase), expected);
        keys.push(expected.to_string());
    }

    // BridgeValidationReason::loc_key — every variant carries a key.
    for reason in [
        BridgeValidationReason::EmptyAlbumTitle,
        BridgeValidationReason::NoAlbumArtist,
        BridgeValidationReason::EmptyArtistName,
        BridgeValidationReason::InvalidYear,
    ] {
        let expected = match reason {
            BridgeValidationReason::EmptyAlbumTitle => "core.import.validation.empty_album_title",
            BridgeValidationReason::NoAlbumArtist => "core.import.validation.no_album_artist",
            BridgeValidationReason::EmptyArtistName => "core.import.validation.empty_artist_name",
            BridgeValidationReason::InvalidYear => "core.import.validation.invalid_year",
        };
        assert_eq!(reason.loc_key(), expected);
        keys.push(expected.to_string());
    }

    // bridge_lookup_failure_key — all keyed variants must produce catalog
    // keys; Diagnostic carries no key.
    for f in [
        BridgeLookupFailure::Network,
        BridgeLookupFailure::Provider { status: Some(503) },
        BridgeLookupFailure::Provider { status: None },
        BridgeLookupFailure::Timeout,
        BridgeLookupFailure::RateLimited,
        BridgeLookupFailure::Credentials,
        BridgeLookupFailure::ArtworkAnalysis,
    ] {
        keys.push(
            bridge_lookup_failure_key(f)
                .expect("typed lookup failure is keyed")
                .to_string(),
        );
    }
    assert!(bridge_lookup_failure_key(BridgeLookupFailure::Diagnostic {
        detail: String::new(),
    })
    .is_none());

    // bridge_error_category_key — every variant carries a key.
    for c in [
        BridgeErrorCategory::Database,
        BridgeErrorCategory::Config,
        BridgeErrorCategory::Internal,
        BridgeErrorCategory::Import,
        BridgeErrorCategory::Export,
        BridgeErrorCategory::Save,
        BridgeErrorCategory::CloudSetup {
            failure: BridgeCloudHomeSetupFailure::Authentication,
        },
        BridgeErrorCategory::CloudSetup {
            failure: BridgeCloudHomeSetupFailure::PermissionDenied,
        },
        BridgeErrorCategory::CloudSetup {
            failure: BridgeCloudHomeSetupFailure::ContainerNotFound,
        },
        BridgeErrorCategory::CloudSetup {
            failure: BridgeCloudHomeSetupFailure::RegionMismatch,
        },
        BridgeErrorCategory::CloudSetup {
            failure: BridgeCloudHomeSetupFailure::QuotaExceeded,
        },
        BridgeErrorCategory::CloudSetup {
            failure: BridgeCloudHomeSetupFailure::InvalidConfiguration,
        },
        BridgeErrorCategory::CloudSetup {
            failure: BridgeCloudHomeSetupFailure::LocationOccupied,
        },
        BridgeErrorCategory::CloudSetup {
            failure: BridgeCloudHomeSetupFailure::Network,
        },
        BridgeErrorCategory::CloudSetup {
            failure: BridgeCloudHomeSetupFailure::DeviceIdentityMissing,
        },
        BridgeErrorCategory::CloudSetup {
            failure: BridgeCloudHomeSetupFailure::SecureStorage,
        },
        BridgeErrorCategory::CloudSetup {
            failure: BridgeCloudHomeSetupFailure::Internal,
        },
        BridgeErrorCategory::DeviceIdentityMissing,
        BridgeErrorCategory::Credentials,
        BridgeErrorCategory::Network,
        BridgeErrorCategory::Keyring,
        BridgeErrorCategory::KeyringLocked,
        BridgeErrorCategory::Membership,
        BridgeErrorCategory::DeviceJoin {
            failure: BridgeDeviceJoinFailure::Expired,
        },
        BridgeErrorCategory::DeviceJoin {
            failure: BridgeDeviceJoinFailure::OwnerOffline,
        },
        BridgeErrorCategory::DeviceJoin {
            failure: BridgeDeviceJoinFailure::OwnerEnded,
        },
        BridgeErrorCategory::AirPlayUnsupported,
    ] {
        let expected = match c {
            BridgeErrorCategory::Database => "core.error.category.database",
            BridgeErrorCategory::Config => "core.error.category.config",
            BridgeErrorCategory::Internal => "core.error.category.internal",
            BridgeErrorCategory::Import => "core.error.category.import",
            BridgeErrorCategory::Export => "core.error.category.export",
            BridgeErrorCategory::Save => "core.error.category.save",
            BridgeErrorCategory::CloudSetup { failure } => match failure {
                BridgeCloudHomeSetupFailure::Authentication => "core.error.category.credentials",
                BridgeCloudHomeSetupFailure::PermissionDenied => {
                    "core.error.cloud_setup.permission_denied"
                }
                BridgeCloudHomeSetupFailure::ContainerNotFound => {
                    "core.error.cloud_setup.container_not_found"
                }
                BridgeCloudHomeSetupFailure::RegionMismatch => {
                    "core.error.cloud_setup.region_mismatch"
                }
                BridgeCloudHomeSetupFailure::QuotaExceeded => {
                    "core.error.cloud_setup.quota_exceeded"
                }
                BridgeCloudHomeSetupFailure::InvalidConfiguration => {
                    "core.error.cloud_setup.invalid_configuration"
                }
                BridgeCloudHomeSetupFailure::LocationOccupied => {
                    "core.error.cloud_setup.location_occupied"
                }
                BridgeCloudHomeSetupFailure::Network => "core.error.category.network",
                BridgeCloudHomeSetupFailure::DeviceIdentityMissing => "core.error.identity_missing",
                BridgeCloudHomeSetupFailure::SecureStorage => "core.error.category.keyring",
                BridgeCloudHomeSetupFailure::Internal => "core.error.category.internal",
            },
            BridgeErrorCategory::DeviceIdentityMissing => "core.error.identity_missing",
            BridgeErrorCategory::Credentials => "core.error.category.credentials",
            BridgeErrorCategory::Network => "core.error.category.network",
            BridgeErrorCategory::Keyring => "core.error.category.keyring",
            BridgeErrorCategory::KeyringLocked => "core.error.keyring.locked",
            BridgeErrorCategory::Membership => "core.error.category.membership",
            BridgeErrorCategory::DeviceJoin { failure } => match failure {
                BridgeDeviceJoinFailure::Expired => "core.error.join.expired",
                BridgeDeviceJoinFailure::OwnerOffline => "core.error.join.owner_offline",
                BridgeDeviceJoinFailure::OwnerEnded => "core.error.join.owner_ended",
            },
            BridgeErrorCategory::AirPlayUnsupported => "core.error.category.airplay_unsupported",
        };
        assert_eq!(bridge_error_category_key(c), expected);
        keys.push(expected.to_string());
    }

    // bridge_entity_not_found_key — every variant carries a key.
    for e in [
        BridgeEntityKind::Library,
        BridgeEntityKind::Album,
        BridgeEntityKind::Release,
        BridgeEntityKind::Track,
        BridgeEntityKind::File,
    ] {
        let expected = match e {
            BridgeEntityKind::Library => "core.error.not_found.library",
            BridgeEntityKind::Album => "core.error.not_found.album",
            BridgeEntityKind::Release => "core.error.not_found.release",
            BridgeEntityKind::Track => "core.error.not_found.track",
            BridgeEntityKind::File => "core.error.not_found.file",
        };
        assert_eq!(bridge_entity_not_found_key(e), expected);
        keys.push(expected.to_string());
    }

    // bridge_error_line_key — Cancelled carries no line (None); the other two
    // agree with the per-part key fns above, so an error has exactly one line
    // and it is not re-derived anywhere. The keys themselves are already
    // pushed by those loops, so nothing is added here.
    for e in [
        BridgeError::Cancelled,
        BridgeError::NotFound {
            entity: BridgeEntityKind::Album,
            id: "a".to_string(),
        },
        BridgeError::internal(""),
    ] {
        let expected: Option<String> = match &e {
            BridgeError::Cancelled => None,
            BridgeError::NotFound { entity, .. } => Some(bridge_entity_not_found_key(*entity)),
            BridgeError::Diagnostic { category, .. } => Some(bridge_error_category_key(*category)),
        };
        assert_eq!(bridge_error_line_key(&e), expected);
    }

    // bridge_playback_error_reason_key — Diagnostic carries no key (None).
    for r in [
        BridgePlaybackErrorReason::SyncDisconnected,
        BridgePlaybackErrorReason::UploadPending,
        BridgePlaybackErrorReason::Diagnostic {
            error: BridgeError::internal(""),
        },
    ] {
        let expected: Option<&str> = match r {
            BridgePlaybackErrorReason::SyncDisconnected => {
                Some("core.playback.error.sync_disconnected")
            }
            BridgePlaybackErrorReason::UploadPending => Some("core.playback.error.upload_pending"),
            BridgePlaybackErrorReason::Diagnostic { .. } => None,
        };
        assert_eq!(bridge_playback_error_reason_key(&r).as_deref(), expected);
        if let Some(k) = expected {
            keys.push(k.to_string());
        }
    }

    keys.extend(
        [
            bae_core::playback::SIDE_PAUSE_TITLE_KEY,
            bae_core::playback::SIDE_PAUSE_VINYL_MESSAGE_KEY,
            bae_core::playback::SIDE_PAUSE_CASSETTE_MESSAGE_KEY,
        ]
        .into_iter()
        .map(str::to_string),
    );

    keys
}

fn catalog() -> bae_loc::Catalog {
    bae_loc::Catalog::from_toml(include_str!("../../loc/catalog.toml")).expect("catalog parses")
}

/// Missing-key direction: every produced key and every direct-reference key
/// has a catalog entry.
#[test]
fn every_produced_key_exists_in_catalog() {
    let cat = catalog();
    for key in produced_keys()
        .iter()
        .map(String::as_str)
        .chain(DIRECT_KEYS.iter().copied())
    {
        assert!(
            cat.messages.contains_key(key),
            "catalog missing `{key}` — a key fn or DIRECT_KEYS produces it but the entry is gone"
        );
    }
}

/// Orphan direction: every `core.*` catalog entry is produced by a key fn
/// or listed in `DIRECT_KEYS`.
#[test]
fn no_orphan_core_keys() {
    let cat = catalog();
    let mut accounted: std::collections::HashSet<String> = produced_keys().into_iter().collect();
    accounted.extend(DIRECT_KEYS.iter().map(|k| k.to_string()));

    for key in cat.messages.keys() {
        if !key.starts_with("core.") {
            continue;
        }
        assert!(
            accounted.contains(key),
            "catalog key `{key}` has no producer — delete it or add a producer \
             (a bridge_*_key fn) or list it in DIRECT_KEYS"
        );
    }
}
