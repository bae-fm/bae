//! Checks that the `core.*` catalog matches the keys the `bridge_*_key`
//! functions produce, in both directions, so a missing entry fails the tests
//! instead of showing a raw key.
//!
//! Each keyed enum is walked by an array of its variants and the production
//! function supplies each key, so the mapping is never copied here; a variant
//! left off an array shows up as an orphan catalog key.

use super::*;

/// `core.*` keys the UI names directly rather than through a `bridge_*_key`
/// function.
const DIRECT_KEYS: &[&str] = &[
    // The import summary names a draft read from the folder's own metadata.
    "core.import.metadata.file_metadata",
    // Storage queue summary (UI composes counts).
    "core.queue.uploading",
    "core.queue.downloading",
    "core.queue.failed",
    "core.queue.queued",
    "core.queue.stored",
    "core.queue.pinning",
    "core.queue.waiting",
    "core.queue.pinned",
    // A queue row whose release the library no longer holds.
    "core.queue.release_missing",
    "core.download.bytes_progress",
    // Transfer labels resolved by the UI.
    "core.outbox.throughput",
    "core.outbox.eta",
    // A member's or invite's S3 key, which the person deletes in the
    // provider's console: its id, and the line before removing or cancelling.
    "core.access_key.id",
    "core.access_key.forget",
    // An S3 invite's screen, and the question before cancelling an invite.
    "core.access_key.unused_invite",
    "core.invite.cancel_confirm.title",
    "core.invite.cancel_confirm.keep",
    // The sync activity list.
    "core.activity.title",
    "core.activity.attention",
    "core.activity.open_settings",
    "core.activity.syncing",
    "core.activity.idle",
    "core.activity.retry_now",
    // The cast picker words a lost device's end with its name.
    "core.cast.ended",
    // Returned by the device-reset confirmation object.
    "core.sync.reset_device.warning",
    // The device-reset screens' confirm button.
    "core.sync.reset_device.confirm",
    // The warning before restoring over this device's copy with a code.
    "core.sync.restore_device.warning",
    // A removal's confirmation: the UI words each count of what it would
    // lose through these.
    "core.library.removal.unsent_writes",
    "core.library.removal.unsent_files",
    // Album total playing time: the UI switches on `BridgeDurationUnits` and
    // composes the hours and minutes words through the join pattern.
    "core.duration.hours",
    "core.duration.minutes",
    "core.duration.hours_minutes",
    // Source-audio formatters compose these labels and numeric facts directly.
    "core.audio.label",
    "core.audio.layout.cue",
    "core.audio.layout.file",
    "core.audio.list_separator",
    // A release's labels line joins label names and catalog numbers with this.
    "core.label.list_separator",
    "core.audio.sample_rate_khz",
    "core.audio.bitrate_kbps",
    "core.audio.bit_depth",
    "core.audio.channels.count",
    "core.release.media",
    // The accessibility label of the arrow on a row whose facts came from a
    // catalog record.
    "core.identity.identified",
    // The pause alert's body, the same sentence for a side or a disc.
    "core.playback.pause.message",
    // A `BridgeFactTerm::Counted` part: the UI words the count and the
    // medium's label through it.
    "core.pressing.media_count",
    // `BridgeReleaseName::Numbered`: the UI words the number through it.
    "core.release.numbered",
];

/// Every key the `bridge_*_key` functions can emit. The assertions beside the
/// loops check what neither catalog direction can see: that a value names no
/// key, or that two values share one.
fn produced_keys() -> Vec<String> {
    let mut keys = Vec::new();

    // bridge_transfer_action_key — every variant carries a key.
    for a in [
        BridgeReleaseStorageAction::Upload,
        BridgeReleaseStorageAction::Pin,
        BridgeReleaseStorageAction::Unpin,
        BridgeReleaseStorageAction::DetachAudio,
    ] {
        keys.push(bridge_transfer_action_key(a));
    }

    // bridge_sheet_refused_codec_key — one key, no variants to walk.
    keys.push(bridge_sheet_refused_codec_key());

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
        BridgeCloudProvider::Dropbox,
        BridgeCloudProvider::OneDrive,
        BridgeCloudProvider::CloudKit,
    ] {
        assert!(bridge_cloud_provider_label_key(Some(p)).is_none());
    }

    // bridge_invalid_reason_key — every variant carries a key.
    for r in [
        BridgeInvalidReason::SameName {
            name: "Track.flac".to_string(),
        },
        BridgeInvalidReason::DamagedAudio { count: 2 },
        BridgeInvalidReason::NoValidAudio,
    ] {
        keys.push(bridge_invalid_reason_key(r));
    }

    // bridge_audio_damage_key — every variant carries a key.
    for damage in [BridgeAudioDamage::Empty, BridgeAudioDamage::Unreadable] {
        keys.push(bridge_audio_damage_key(damage));
    }

    // bridge_unusable_barcode_key — every variant carries a key.
    for reason in [
        BridgeUnusableBarcode::NotACode,
        BridgeUnusableBarcode::TooShort,
        BridgeUnusableBarcode::Placeholder,
        BridgeUnusableBarcode::StoreInternal,
    ] {
        keys.push(bridge_unusable_barcode_key(reason));
    }

    // bridge_folder_check_key — every variant carries a key.
    for folder_check in [
        BridgeFolderCheck::SourceTracksUnknown,
        BridgeFolderCheck::MediumDisagrees {
            folder: BridgeMediumConflict::CdRip,
        },
        BridgeFolderCheck::MediumDisagrees {
            folder: BridgeMediumConflict::NotCdAudio {
                sample_rate_hz: None,
            },
        },
    ] {
        keys.push(bridge_folder_check_key(&folder_check));
    }

    // bridge_prepare_step_key — every variant carries a key.
    for step in [
        BridgePrepareStep::Queued,
        BridgePrepareStep::ValidatingSourceFiles,
        BridgePrepareStep::FetchingArtistPictures,
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

    // bridge_lookup_failure_key — every variant produces a catalog key.
    for f in [
        BridgeLookupFailure::Network,
        BridgeLookupFailure::Provider { status: Some(503) },
        BridgeLookupFailure::Provider { status: None },
        BridgeLookupFailure::Timeout,
    ] {
        keys.push(bridge_lookup_failure_key(f));
    }

    // bridge_error_category_key — every variant carries a key.
    for c in [
        BridgeErrorCategory::Database,
        BridgeErrorCategory::Config,
        BridgeErrorCategory::Internal,
        BridgeErrorCategory::SyncUpdateRequired,
        BridgeErrorCategory::Import,
        BridgeErrorCategory::ImportData,
        BridgeErrorCategory::MetadataTrackCount {
            folder_tracks: 12,
            release_tracks: 10,
        },
        BridgeErrorCategory::TiedMediums {
            mediums: vec![2, 4],
        },
        BridgeErrorCategory::ReleaseChangedSinceOpened,
        BridgeErrorCategory::CandidateImportInProgress,
        BridgeErrorCategory::CandidateBeingIdentified,
        BridgeErrorCategory::FilesAlreadyImported,
        BridgeErrorCategory::Export,
        BridgeErrorCategory::Save,
        BridgeErrorCategory::DeviceResetRequired,
        BridgeErrorCategory::DeviceRemoved,
        BridgeErrorCategory::StorageCheck {
            check: BridgeStorageCheck::Read,
            failure: BridgeStorageFailure::Network,
        },
        BridgeErrorCategory::S3VersioningUnknown {
            failure: BridgeStorageFailure::Network,
        },
        BridgeErrorCategory::S3VersioningUnsupported {
            state: BridgeS3BucketVersioning::Enabled,
        },
        BridgeErrorCategory::StorageLocationChangeUnsupported,
        BridgeErrorCategory::S3Prefix {
            problem: BridgeS3PrefixProblem::EmptySegment,
        },
        BridgeErrorCategory::S3Prefix {
            problem: BridgeS3PrefixProblem::InvalidCharacter {
                segment: "bae.lib".into(),
                character: ".".into(),
            },
        },
        BridgeErrorCategory::S3Prefix {
            problem: BridgeS3PrefixProblem::SegmentTooLong {
                segment: "b".repeat(256),
            },
        },
        BridgeErrorCategory::DeviceIdentityMissing,
        BridgeErrorCategory::Credentials,
        BridgeErrorCategory::Network,
        BridgeErrorCategory::Keyring,
        BridgeErrorCategory::KeyringLocked,
        BridgeErrorCategory::KeychainEntitlementMissing,
        BridgeErrorCategory::SavedRestoreCodesFull,
        BridgeErrorCategory::SavedRestoreCodeTooLarge,
        BridgeErrorCategory::Membership,
        BridgeErrorCategory::InviteJoining,
        BridgeErrorCategory::Renderer,
        BridgeErrorCategory::GroupingBlocked {
            reason: BridgeGroupingBlock::SourceChanged,
        },
        BridgeErrorCategory::GroupingBlocked {
            reason: BridgeGroupingBlock::SourceGone,
        },
        BridgeErrorCategory::GroupingBlocked {
            reason: BridgeGroupingBlock::FolderFilesTaken,
        },
        BridgeErrorCategory::GroupingBlocked {
            reason: BridgeGroupingBlock::FolderFilesContested,
        },
        BridgeErrorCategory::GroupingBlocked {
            reason: BridgeGroupingBlock::FolderFilesDownloading,
        },
        BridgeErrorCategory::AirPlayUnsupported,
        BridgeErrorCategory::AirPlayUnreadableSettings,
        BridgeErrorCategory::ReplacementUnfinished {
            by: BridgeReplacedBy::Reset,
        },
        BridgeErrorCategory::ReplacementUnfinished {
            by: BridgeReplacedBy::Restore,
        },
    ] {
        keys.push(bridge_error_category_key(c));
    }

    for failure in [
        BridgeStorageFailure::ProviderUnavailable,
        BridgeStorageFailure::AccessNotRegistered {
            required: crate::BridgeOAuthAccess::OneDrive,
        },
        BridgeStorageFailure::AccessNotGranted {
            required: crate::BridgeOAuthAccess::OneDrive,
        },
        BridgeStorageFailure::DropboxAppFolderNotShareable,
        BridgeStorageFailure::Network,
        BridgeStorageFailure::Authentication,
        BridgeStorageFailure::PermissionDenied,
        BridgeStorageFailure::NotFound,
        BridgeStorageFailure::AlreadyExists,
        BridgeStorageFailure::Conflict,
        BridgeStorageFailure::ContainerNotFound,
        BridgeStorageFailure::RegionMismatch,
        BridgeStorageFailure::OwnBucketRequired {
            reason: BridgeOwnBucketReason::Prefix,
        },
        BridgeStorageFailure::StoreExists,
        BridgeStorageFailure::ClockSkew,
        BridgeStorageFailure::QuotaExceeded,
        BridgeStorageFailure::RateLimited,
        BridgeStorageFailure::InvalidConfiguration,
        BridgeStorageFailure::Refused,
        BridgeStorageFailure::Protocol,
        BridgeStorageFailure::InvalidPath,
        BridgeStorageFailure::InvalidRange,
        BridgeStorageFailure::NotStoreOwner,
        BridgeStorageFailure::InviteeIsStoreOwner,
        BridgeStorageFailure::AccountIdUnavailable,
        BridgeStorageFailure::InvitationMismatch,
        BridgeStorageFailure::SessionMismatch,
        BridgeStorageFailure::SessionIdentityUnknown,
        BridgeStorageFailure::SessionExpired,
        BridgeStorageFailure::InvalidPart,
        BridgeStorageFailure::SingleRequestTooLarge {
            size: 200,
            limit: 100,
        },
        BridgeStorageFailure::CopyTooLarge {
            size: 200,
            limit: 100,
        },
        BridgeStorageFailure::Encoding,
        BridgeStorageFailure::File,
        BridgeStorageFailure::MemberKeysMissing,
        BridgeStorageFailure::CatchUpRequired,
    ] {
        keys.push(bridge_error_category_key(BridgeErrorCategory::Storage {
            failure,
        }));
    }
    keys.extend(
        pending_subject_messages()
            .into_iter()
            .map(|message| message.key),
    );
    for reason in [
        PendingReasonKind::Missing,
        PendingReasonKind::Waits,
        PendingReasonKind::KeyUnavailable,
        PendingReasonKind::UpdateRequired,
        PendingReasonKind::NoStorage,
        PendingReasonKind::Storage,
        PendingReasonKind::Refused,
        PendingReasonKind::WritesRefused,
        PendingReasonKind::ProviderPending,
        PendingReasonKind::PendingOwner,
        PendingReasonKind::AccountInUse,
        PendingReasonKind::AccessRemains,
        PendingReasonKind::FileUnavailable,
        PendingReasonKind::FileChanged,
        PendingReasonKind::FileLost,
        PendingReasonKind::Failed,
        PendingReasonKind::Rejected,
        PendingReasonKind::Cleanup,
        PendingReasonKind::Paused,
        PendingReasonKind::Removed,
        PendingReasonKind::MemberKeysReplaced,
    ] {
        keys.push(reason.key().to_string());
    }
    for action in [
        BridgePendingAction::Wait,
        BridgePendingAction::UpdateApp,
        BridgePendingAction::SignIn {
            provider: BridgeCloudProvider::Dropbox,
        },
        BridgePendingAction::Reconnect,
        BridgePendingAction::RestoreFile,
        BridgePendingAction::ResumeUploads,
        BridgePendingAction::RetryOperation { id: 1 },
        BridgePendingAction::RetryDownload {
            release_id: "release".to_string(),
        },
        BridgePendingAction::CancelUpload { id: 1 },
        BridgePendingAction::RestoreFromCode,
        BridgePendingAction::Review,
    ] {
        keys.extend(action.key().map(str::to_string));
    }
    assert!(BridgePendingAction::Wait.key().is_none());
    for indicator in [
        BridgeSyncIndicator::Unconnected,
        BridgeSyncIndicator::Loading,
        BridgeSyncIndicator::Stopped,
        BridgeSyncIndicator::Syncing,
        BridgeSyncIndicator::Synced,
        BridgeSyncIndicator::Offline,
        BridgeSyncIndicator::Failed,
        BridgeSyncIndicator::Unreadable,
        BridgeSyncIndicator::Waiting { items: 2 },
    ] {
        keys.extend(bridge_sync_indicator_key(&indicator));
    }
    for outcome in [
        BridgeSyncRetryOutcome::Synced,
        BridgeSyncRetryOutcome::Failed {
            error: BridgeError::Cancelled,
        },
        BridgeSyncRetryOutcome::CoolingDown {
            until: std::time::UNIX_EPOCH,
        },
        BridgeSyncRetryOutcome::NotSyncing,
    ] {
        keys.extend(bridge_sync_retry_outcome_key(&outcome));
    }

    for instruction in [
        BridgeProviderSignOut::RemoveAppAccess {
            provider: BridgeCloudProvider::Dropbox,
        },
        BridgeProviderSignOut::RemoveFromAppleAccount,
    ] {
        keys.push(bridge_provider_sign_out_key(instruction));
    }
    // bridge_upload_activity_key — each variant carries a key.
    for activity in [
        BridgeUploadActivity::Uploading,
        BridgeUploadActivity::Queued,
    ] {
        keys.push(bridge_upload_activity_key(activity));
    }
    // bridge_entity_not_found_key — every variant carries a key.
    keys.push(bridge_entity_not_found_key(BridgeEntityKind::Library));

    // bridge_error_line_key — Cancelled has no line; the others must match
    // the per-part key functions above, whose loops already pushed the keys.
    for e in [
        BridgeError::Cancelled,
        BridgeError::NotFound {
            entity: BridgeEntityKind::Library,
            id: "a".to_string(),
        },
        BridgeError::internal(""),
    ] {
        let expected: Option<String> = match &e {
            BridgeError::Cancelled => None,
            BridgeError::NotFound { entity, .. } => Some(bridge_entity_not_found_key(*entity)),
            BridgeError::Diagnostic { category, .. } => {
                Some(bridge_error_category_key(category.clone()))
            }
        };
        assert_eq!(bridge_error_line_key(&e), expected);
    }

    // bridge_playback_error_reason_key — Diagnostic carries no key.
    for r in [
        BridgePlaybackErrorReason::SyncDisconnected,
        BridgePlaybackErrorReason::UploadPending,
        BridgePlaybackErrorReason::NotUploaded,
        BridgePlaybackErrorReason::OriginalMissing,
        BridgePlaybackErrorReason::AudioOutput {
            problem: BridgeAudioOutputProblem::DeviceUnavailable,
        },
        BridgePlaybackErrorReason::AudioOutput {
            problem: BridgeAudioOutputProblem::UnsupportedFormat,
        },
        BridgePlaybackErrorReason::AudioOutput {
            problem: BridgeAudioOutputProblem::AirPlayRefused,
        },
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
        playability_messages()
            .into_iter()
            .map(|message| message.key),
    );

    for boundary in [BridgePauseBoundary::Side, BridgePauseBoundary::Disc] {
        keys.push(bridge_pause_boundary_title_key(boundary));
        keys.push(bridge_pause_boundary_countdown_key(boundary));
        keys.push(bridge_pause_boundary_keep_pausing_key(boundary));
    }

    // The pressing vocabularies — every region, status and packaging carries
    // a key; a medium or a Discogs detail carries one where it is a word, and
    // is printed as its term where it is a name, a size or a file type.
    keys.extend(bridge_regions().into_iter().map(bridge_region_key));
    keys.extend(
        bridge_release_statuses()
            .into_iter()
            .map(bridge_release_status_key),
    );
    keys.extend(bridge_packagings().into_iter().map(bridge_packaging_key));
    for label in bridge_media().into_iter().map(bridge_medium_label).chain(
        bridge_discogs_details()
            .into_iter()
            .map(bridge_discogs_detail_label),
    ) {
        if let BridgeTermLabel::Localized { key } = label {
            keys.push(key);
        }
    }
    assert_eq!(
        bridge_medium_label(BridgeMedium::Cd),
        BridgeTermLabel::Verbatim {
            text: "CD".to_string()
        }
    );
    assert_eq!(
        bridge_discogs_detail_label(BridgeDiscogsDetail::Size12In),
        BridgeTermLabel::Verbatim {
            text: "12\"".to_string()
        }
    );

    keys
}

/// Every pending subject's message, one per subject and operation kind.
fn pending_subject_messages() -> Vec<BridgeMessage> {
    use bae_core::library::PendingSubjectName as S;
    use coven::OperationKind as O;
    let operations = [
        O::CreateStore,
        O::RenameStore,
        O::AddDevice,
        O::SetRole,
        O::SetAccess,
        O::RemoveMember,
        O::RemoveDevice,
        O::RenameCircle,
        O::RemoveCircleMember,
        O::RetainSequence,
        O::RetainFiles,
        O::WriteSnapshot,
        O::SchemaChange,
        O::Reset,
        O::ReloadFromSnapshot,
        O::RequiredReload,
        O::RotateKey,
        O::CreateCircle,
        O::AddCircleMember,
        O::DeleteCircle,
        O::Invite,
        O::RevokeAccess,
        O::ReplaceMemberKeys,
    ];
    [
        S::ReleaseFile {
            file_name: "01 Track.flac".into(),
            album_title: "Album Title".into(),
        },
        S::Cover {
            album_title: "Album Title".into(),
        },
        S::ArtistImage,
        S::Avatar,
        S::DeletedReleaseFile,
        S::File,
        S::LocalChanges,
        S::LibraryChanges,
        S::KeyCopy,
        S::Snapshot,
        S::Retention,
        S::LibraryData,
        S::CircleData,
        S::JoinRequest,
        S::Connection,
    ]
    .into_iter()
    .chain(operations.map(S::Operation))
    .map(subject_message)
    .collect()
}

/// Every playability's line: none for a release this device can play, or
/// whose originals the disk hasn't answered for.
fn playability_messages() -> Vec<BridgeMessage> {
    let name = || "Device Name".to_string();
    [
        BridgeReleasePlayability::Playable,
        BridgeReleasePlayability::OnOtherDevice {
            device: BridgeOtherDevice::Active { name: name() },
        },
        BridgeReleasePlayability::OnOtherDevice {
            device: BridgeOtherDevice::Former {
                name: name(),
                state: BridgeFormerDeviceState::Removed,
            },
        },
        BridgeReleasePlayability::OnOtherDevice {
            device: BridgeOtherDevice::Former {
                name: name(),
                state: BridgeFormerDeviceState::Replaced,
            },
        },
        BridgeReleasePlayability::OriginalMissing,
        BridgeReleasePlayability::CheckingOriginals,
    ]
    .iter()
    .filter_map(bridge_release_playability_line)
    .collect()
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

    let orphans: Vec<_> = cat
        .messages
        .keys()
        .filter(|key| key.starts_with("core.") && !accounted.contains(*key))
        .collect();
    assert!(
        orphans.is_empty(),
        "catalog keys without a producer: {orphans:?}"
    );
}

/// A subject or playability message names its arguments as its source
/// message does and in the same order, since Apple's catalog takes them by
/// position.
#[test]
fn message_arguments_follow_the_source_message() {
    let cat = catalog();
    for message in pending_subject_messages()
        .into_iter()
        .chain(playability_messages())
    {
        let source = &cat.messages[&message.key].value;
        let expected = bae_loc::mf1::referenced_args(
            &bae_loc::mf1::parse(source).expect("source message parses"),
        );
        let names: Vec<_> = message.args.iter().map(|arg| arg.name.clone()).collect();
        assert_eq!(names, expected, "`{}` arguments", message.key);
    }
}
