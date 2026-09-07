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
//! Each keyed enum is covered by an explicit array of every variant, whose
//! keys the production fn is asked for. The mapping is never restated here, so
//! there is no second copy to drift: a variant left off an array shows up as
//! an orphan catalog key the moment its entry lands.

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
    "core.outbox.source_unavailable",
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
    // Source-audio formatters compose these labels and numeric facts directly.
    "core.audio.label",
    "core.audio.layout.cue",
    "core.audio.list_separator",
    "core.audio.mixed",
    "core.audio.sample_rate_khz",
    "core.audio.bitrate_kbps",
    "core.audio.bit_depth",
    "core.audio.channels.count",
    "core.release.media",
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

/// Every key the `bridge_*_key` fns can emit. Each keyed enum is walked by an
/// explicit array of all its variants and the production fn is asked for the
/// key — the keys are never restated here, so this cannot drift into a second
/// copy of the mapping. A variant left off an array surfaces as an orphan in
/// `no_orphan_core_keys` as soon as its catalog entry lands. The assertions
/// beside the loops carry only what neither catalog direction can see: that a
/// value names no key, and that two values deliberately name the same one.
fn produced_keys() -> Vec<String> {
    let mut keys = super::device_pairing_progress_tests::progress_keys();

    // bridge_transfer_action_key — every variant carries a key.
    for a in [
        BridgeReleaseStorageAction::MakeRemote,
        BridgeReleaseStorageAction::Pin,
        BridgeReleaseStorageAction::Unpin,
        BridgeReleaseStorageAction::MakeLocal,
    ] {
        keys.push(bridge_transfer_action_key(a));
    }

    // bridge_sheet_refused_codec_key — one key, no variants to walk.
    keys.push(bridge_sheet_refused_codec_key());

    // bridge_upload_phase_bytes_key — each phase names itself beside the
    // bar it labels.
    for phase in [BridgeUploadPhase::Preparing, BridgeUploadPhase::Uploading] {
        keys.push(bridge_upload_phase_bytes_key(phase));
    }

    // bridge_network_folder_watch_key — one key, no variants to walk.
    keys.push(bridge_network_folder_watch_key());

    // bridge_file_evidence_key — each signal that can name a file words its
    // own hover.
    for signal in [BridgeEvidenceSignal::Barcode, BridgeEvidenceSignal::DiscId] {
        keys.push(bridge_file_evidence_key(&BridgeFileEvidence {
            signal,
            value: "5099969394522".to_string(),
            file_id: "Back.jpg".to_string(),
        }));
    }

    // bridge_file_role_key — every role the scan can propose has a name.
    for role in [
        BridgeFileRole::Audio,
        BridgeFileRole::TrackSheet { track_count: 0 },
        BridgeFileRole::Artwork {
            choice: loc_cover_choice(),
        },
        BridgeFileRole::Document,
        BridgeFileRole::Other,
    ] {
        keys.push(bridge_file_role_key(&role));
    }

    // bridge_file_role_choice_key — the roles a person can pick between.
    for choice in [BridgeFileRoleChoice::Audio, BridgeFileRoleChoice::NotATrack] {
        keys.push(bridge_file_role_choice_key(choice));
    }
    // The picker's option and the column's label name one thing, so they read
    // under one key.
    assert_eq!(
        bridge_file_role_choice_key(BridgeFileRoleChoice::Audio),
        bridge_file_role_key(&BridgeFileRole::Audio)
    );

    // bridge_file_becomes_key — one slot, a run of slots, or none. The
    // single-slot case has its own key because "slot 12" and "slots 1-11"
    // are different sentences, not one sentence with a range in it.
    for becomes in [
        BridgeFileBecomes::Slots { first: 3, last: 3 },
        BridgeFileBecomes::Slots { first: 1, last: 11 },
        BridgeFileBecomes::NoSlots,
    ] {
        keys.push(bridge_file_becomes_key(becomes));
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
        keys.extend(bridge_slot_reconciliation_key(reconciliation));
    }
    // An agreement draws no line, so it names no key.
    assert!(
        bridge_slot_reconciliation_key(BridgeSlotReconciliation::Agrees { count: 12 }).is_none()
    );

    // bridge_sheet_binding_offer_key — an offered file needs no reason.
    for o in [
        BridgeSheetBindingOffer::Offered,
        BridgeSheetBindingOffer::RefusedCodec {
            codec: String::new(),
        },
        BridgeSheetBindingOffer::RefusedTiming,
        BridgeSheetBindingOffer::RefusedUnreadable,
    ] {
        keys.extend(bridge_sheet_binding_offer_key(o));
    }
    assert!(bridge_sheet_binding_offer_key(BridgeSheetBindingOffer::Offered).is_none());

    // BridgeTrackSide::header_key — this is what BridgeTrackGroup::header_key
    // is built from at conversion.
    for s in [
        BridgeTrackSide::Sided {
            side_letter: "A".to_string(),
        },
        BridgeTrackSide::Disc { disc: 1 },
        BridgeTrackSide::Flat,
    ] {
        keys.extend(s.header_key().map(str::to_string));
    }
    // A flat track list has no header to word.
    assert!(BridgeTrackSide::Flat.header_key().is_none());

    // bridge_audio_channels_key — only 1 and 2 carry words.
    for channels in [1_i64, 2] {
        keys.extend(bridge_audio_channels_key(channels));
    }
    assert!(bridge_audio_channels_key(6).is_none());

    // bridge_cloud_provider_label_key — None (local-only) and S3 carry keys.
    for p in [None, Some(BridgeCloudProvider::S3)] {
        keys.extend(bridge_cloud_provider_label_key(p));
    }
    // The brand-name providers render their own names, so they name no key.
    for p in [
        BridgeCloudProvider::GoogleDrive,
        BridgeCloudProvider::Dropbox,
        BridgeCloudProvider::OneDrive,
        BridgeCloudProvider::CloudKit,
    ] {
        assert!(bridge_cloud_provider_label_key(Some(p)).is_none());
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
        keys.push(bridge_invalid_reason_key(r));
    }

    // bridge_needs_you_key — every variant carries a key.
    for needs_you in [
        BridgeNeedsYou::AlreadyInLibrary,
        BridgeNeedsYou::SeveralMatches { count: 0 },
        BridgeNeedsYou::NoMatch,
        BridgeNeedsYou::NothingToLookUp,
        BridgeNeedsYou::LookupFailed,
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
        keys.push(bridge_needs_you_key(&needs_you));
    }

    // bridge_prepare_step_key — every variant carries a key.
    for step in [
        BridgePrepareStep::Queued,
        BridgePrepareStep::ValidatingSourceFiles,
    ] {
        keys.push(bridge_prepare_step_key(step));
    }

    // bridge_import_phase_key — every variant carries a key.
    for phase in [
        BridgeImportPhase::ReadingFiles,
        BridgeImportPhase::MeasuringLoudness,
        BridgeImportPhase::Finalizing,
    ] {
        keys.push(bridge_import_phase_key(phase));
    }

    // BridgeValidationReason::loc_key — every variant carries a key.
    for reason in [
        BridgeValidationReason::EmptyAlbumTitle,
        BridgeValidationReason::NoAlbumArtist,
        BridgeValidationReason::EmptyArtistName,
        BridgeValidationReason::InvalidYear,
    ] {
        keys.push(reason.loc_key().to_string());
    }

    // bridge_lookup_failure_key — all keyed variants must produce catalog
    // keys; Diagnostic carries no key.
    for f in [
        BridgeLookupFailure::Network,
        BridgeLookupFailure::Provider { status: Some(503) },
        BridgeLookupFailure::Provider { status: None },
        BridgeLookupFailure::Timeout,
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

    // bridge_lookup_failure_brief_key — total over the variants, with the
    // status split walked so both sides of it produce their key.
    for f in [
        BridgeLookupFailure::Network,
        BridgeLookupFailure::Provider { status: Some(429) },
        BridgeLookupFailure::Provider { status: Some(503) },
        BridgeLookupFailure::Provider { status: Some(404) },
        BridgeLookupFailure::Provider { status: None },
        BridgeLookupFailure::Timeout,
        BridgeLookupFailure::ArtworkAnalysis,
        BridgeLookupFailure::Diagnostic {
            detail: String::new(),
        },
    ] {
        keys.push(bridge_lookup_failure_brief_key(f));
    }
    // A 429 and a 503 are both the provider refusing for now, so they read
    // alike — and neither reads like a 404.
    assert_eq!(
        bridge_lookup_failure_brief_key(BridgeLookupFailure::Provider { status: Some(429) }),
        bridge_lookup_failure_brief_key(BridgeLookupFailure::Provider { status: Some(503) })
    );
    assert_ne!(
        bridge_lookup_failure_brief_key(BridgeLookupFailure::Provider { status: Some(429) }),
        bridge_lookup_failure_brief_key(BridgeLookupFailure::Provider { status: Some(404) })
    );

    // bridge_error_category_key — every variant carries a key.
    for c in [
        BridgeErrorCategory::Database,
        BridgeErrorCategory::Config,
        BridgeErrorCategory::Internal,
        BridgeErrorCategory::SyncUpdateRequired,
        BridgeErrorCategory::Import,
        BridgeErrorCategory::CandidateImportInProgress,
        BridgeErrorCategory::CandidateAlreadyImported,
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
        keys.push(bridge_error_category_key(c));
    }

    // bridge_entity_not_found_key — every variant carries a key.
    for e in [
        BridgeEntityKind::Library,
        BridgeEntityKind::Album,
        BridgeEntityKind::Release,
        BridgeEntityKind::Track,
        BridgeEntityKind::File,
    ] {
        keys.push(bridge_entity_not_found_key(e));
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

    // bridge_playback_error_reason_key — Diagnostic carries no key.
    for r in [
        BridgePlaybackErrorReason::SyncDisconnected,
        BridgePlaybackErrorReason::UploadPending,
        BridgePlaybackErrorReason::Diagnostic {
            error: BridgeError::internal(""),
        },
    ] {
        keys.extend(bridge_playback_error_reason_key(&r));
    }
    assert!(
        bridge_playback_error_reason_key(&BridgePlaybackErrorReason::Diagnostic {
            error: BridgeError::internal(""),
        })
        .is_none()
    );

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
