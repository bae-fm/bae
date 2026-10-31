use super::*;

#[test]
fn reading_pending_items_does_not_clear_a_connection_failure() {
    let snapshot = SyncStatusSnapshot {
        status: coven::SyncStatus::Disconnected,
        pending: None,
        retry_effect: coven::RetryEffect::Pass,
        error: None,
    };
    let state = SyncStatusState {
        state: Arc::new(Mutex::new(snapshot.clone())),
        values: tokio::sync::watch::channel(snapshot).0,
        connected: tokio::sync::watch::channel(false).0,
    };
    state.set_error(Some(UiError::internal("connection failed")));
    state.set_pending(Ok(Vec::new()));
    assert!(state.snapshot().error.is_some());
}

#[test]
fn pending_files_remain_waiting_after_the_sequence_is_synced() {
    let snapshot = SyncStatusSnapshot {
        status: coven::SyncStatus::Synced {
            finished_at: std::time::UNIX_EPOCH,
        },
        pending: Some(Ok(vec![super::super::PendingItem::before_open(
            coven::PendingRecord::new(
                coven::PendingSubject::Connection,
                coven::PendingReason::NoStorage,
            ),
        )])),
        retry_effect: coven::RetryEffect::Pass,
        error: None,
    };
    assert_eq!(snapshot.indicator(), SyncIndicator::Waiting { items: 1 });
}

#[test]
fn pending_authentication_offers_the_provider_sign_in() {
    let record = coven::PendingRecord::new(
        coven::PendingSubject::Connection,
        coven::PendingReason::Storage {
            provider: coven::CloudProvider::Dropbox,
            failure: coven::StorageFailure::Authentication,
            answered: true,
        },
    );
    assert_eq!(
        pending_action(&record, None),
        PendingAction::SignIn(coven::CloudProvider::Dropbox)
    );
}

/// A pin's failed download offers pinning the release again; with no release
/// file naming it, only its details.
#[test]
fn a_failed_pins_download_offers_pinning_its_release_again() {
    let record = coven::PendingRecord::new(
        coven::PendingSubject::Download {
            device: coven::DeviceId(2),
            file: coven::FileId(uuid::Uuid::from_u128(9)),
        },
        coven::PendingReason::Storage {
            provider: coven::CloudProvider::S3,
            failure: coven::StorageFailure::Network,
            answered: true,
        },
    );
    assert_eq!(
        pending_action(&record, Some("release")),
        PendingAction::RetryDownload {
            release_id: "release".into()
        }
    );
    assert_eq!(pending_action(&record, None), PendingAction::Review);
}

/// A move whose original went missing waits for it to be put back; one whose
/// original changed failed, and offers cancelling the release's upload.
#[test]
fn a_moves_original_offers_restoring_it_or_cancelling_the_upload() {
    let original = |failure| {
        coven::PendingRecord::new(
            coven::PendingSubject::Move {
                id: coven::MoveId(4),
            },
            coven::PendingReason::Original {
                path: "/music/a.flac".into(),
                failure,
            },
        )
    };
    assert_eq!(
        pending_action(
            &original(coven::FileSourceFailure::Missing),
            Some("release")
        ),
        PendingAction::RestoreFile
    );
    assert_eq!(
        pending_action(
            &original(coven::FileSourceFailure::Changed),
            Some("release")
        ),
        PendingAction::CancelUpload {
            id: coven::MoveId(4),
        }
    );
}

#[test]
fn missing_provider_permission_offers_sign_in() {
    let record = coven::PendingRecord::new(
        coven::PendingSubject::Connection,
        coven::PendingReason::Storage {
            provider: coven::CloudProvider::Dropbox,
            failure: coven::StorageFailure::AccessNotGranted {
                required: coven::OAuthAccess::Dropbox(coven::DropboxAccess::FullDropbox),
            },
            answered: true,
        },
    );
    assert_eq!(
        pending_action(&record, None),
        PendingAction::SignIn(coven::CloudProvider::Dropbox)
    );
}

#[test]
fn a_failed_pass_with_no_pending_records_does_not_claim_synced() {
    let snapshot = SyncStatusSnapshot {
        status: coven::SyncStatus::Failed {
            error: coven::SyncFailure::Storage(Arc::new(
                coven::StorageFailure::PermissionDenied.into(),
            )),
        },
        pending: Some(Ok(Vec::new())),
        retry_effect: coven::RetryEffect::Pass,
        error: None,
    };
    assert_eq!(snapshot.indicator(), SyncIndicator::Failed);
    assert!(matches!(
        snapshot.failure(),
        Some(UiError::Diagnostic {
            category: crate::ui::UiErrorCategory::Storage(coven::StorageFailure::PermissionDenied),
            ..
        })
    ));
}

/// Only a pass in progress is syncing: a finished, offline, blocked,
/// failed, stopped or absent sync is not, however long its loop runs.
#[test]
fn only_a_pass_in_progress_is_syncing() {
    let syncing = |status| {
        SyncStatusSnapshot {
            status,
            pending: Some(Ok(Vec::new())),
            retry_effect: coven::RetryEffect::Pass,
            error: None,
        }
        .syncing()
    };
    assert!(syncing(coven::SyncStatus::Syncing));
    assert!(!syncing(coven::SyncStatus::Synced {
        finished_at: std::time::UNIX_EPOCH,
    }));
    assert!(!syncing(coven::SyncStatus::Offline {
        error: Arc::new(coven::StorageFailure::PermissionDenied.into()),
    }));
    assert!(!syncing(coven::SyncStatus::Blocked {
        slot: coven::Slot(3),
        reason: coven::PendingReason::NoStorage,
        error: None,
    }));
    assert!(!syncing(coven::SyncStatus::Failed {
        error: coven::SyncFailure::Removed,
    }));
    assert!(!syncing(coven::SyncStatus::Stopped));
    assert!(!syncing(coven::SyncStatus::Disconnected));
}

#[test]
fn an_unread_pending_list_does_not_claim_synced() {
    let snapshot = SyncStatusSnapshot {
        status: coven::SyncStatus::Syncing,
        pending: None,
        retry_effect: coven::RetryEffect::Pass,
        error: None,
    };
    assert_eq!(snapshot.indicator(), SyncIndicator::Loading);
}

/// With nothing pending, the line names the status; it says it is still
/// checking only before the pending list's first read.
#[test]
fn with_nothing_pending_the_line_names_the_status() {
    let indicator = |status, error| {
        SyncStatusSnapshot {
            status,
            pending: Some(Ok(Vec::new())),
            retry_effect: coven::RetryEffect::Pass,
            error,
        }
        .indicator()
    };
    let offline = || coven::SyncStatus::Offline {
        error: Arc::new(coven::StorageFailure::PermissionDenied.into()),
    };
    assert_eq!(
        indicator(coven::SyncStatus::Syncing, None),
        SyncIndicator::Syncing
    );
    assert_eq!(indicator(offline(), None), SyncIndicator::Offline);
    assert_eq!(
        indicator(
            coven::SyncStatus::Blocked {
                slot: coven::Slot(3),
                reason: coven::PendingReason::NoStorage,
                error: None,
            },
            None
        ),
        SyncIndicator::Failed
    );
    assert_eq!(
        indicator(coven::SyncStatus::Stopped, None),
        SyncIndicator::Stopped
    );
    assert_eq!(
        indicator(
            coven::SyncStatus::Synced {
                finished_at: std::time::UNIX_EPOCH,
            },
            Some(UiError::diagnostic(
                crate::ui::UiErrorCategory::Internal,
                "start failed"
            ))
        ),
        SyncIndicator::Failed
    );
}

#[test]
fn a_pending_list_that_cannot_be_read_says_so() {
    let snapshot = SyncStatusSnapshot {
        status: coven::SyncStatus::Synced {
            finished_at: std::time::UNIX_EPOCH,
        },
        pending: Some(Err(UiError::diagnostic(
            crate::ui::UiErrorCategory::Internal,
            "database locked",
        ))),
        retry_effect: coven::RetryEffect::Pass,
        error: None,
    };
    assert_eq!(snapshot.indicator(), SyncIndicator::Unreadable);
}

fn record(reason: coven::PendingReason) -> coven::PendingRecord {
    coven::PendingRecord::new(coven::PendingSubject::Connection, reason)
}

fn failed(error: coven::SyncFailure) -> SyncStatusSnapshot {
    SyncStatusSnapshot {
        status: coven::SyncStatus::Failed { error },
        pending: Some(Ok(Vec::new())),
        retry_effect: coven::RetryEffect::Pass,
        error: None,
    }
}

fn category(snapshot: &SyncStatusSnapshot) -> crate::ui::UiErrorCategory {
    let Some(UiError::Diagnostic { category, .. }) = snapshot.failure() else {
        panic!("the failed pass is the status's failure");
    };
    category
}

/// A removed device says it was removed, and nothing it offers would
/// register it again: its record is only reviewed, never reset.
#[test]
fn a_removed_device_says_so_and_offers_nothing_that_registers_it() {
    assert_eq!(
        pending_action(&record(coven::PendingReason::Removed), None),
        PendingAction::Review
    );
    assert_eq!(
        category(&failed(coven::SyncFailure::Removed)),
        crate::ui::UiErrorCategory::DeviceRemoved
    );
}

/// Keys replaced by another of the person's devices are restored with the
/// person's current code, not by resetting this device.
#[test]
fn replaced_member_keys_offer_restoring_from_the_code() {
    let member = coven::MemberId(uuid::Uuid::from_u128(7));
    assert_eq!(
        pending_action(
            &record(coven::PendingReason::MemberKeysReplaced { member }),
            None
        ),
        PendingAction::RestoreFromCode
    );
}

/// Another copy of this device is the one sync failure a reset answers.
#[test]
fn another_copy_of_this_device_is_answered_by_a_reset() {
    assert_eq!(
        category(&failed(coven::SyncFailure::DeviceIdInUse(
            coven::DeviceIdEvidence::Replaced
        ))),
        crate::ui::UiErrorCategory::DeviceResetRequired
    );
}

fn state(status: coven::SyncStatus) -> SyncStatusState {
    let snapshot = SyncStatusSnapshot {
        status,
        pending: None,
        retry_effect: coven::RetryEffect::Pass,
        error: None,
    };
    SyncStatusState {
        state: Arc::new(Mutex::new(snapshot.clone())),
        values: tokio::sync::watch::channel(snapshot).0,
        connected: tokio::sync::watch::channel(true).0,
    }
}

/// An S3 request that never reached the provider, as coven reports it:
/// its class, and a cause whose message names each layer below it.
fn unreachable_s3() -> Arc<coven::StorageError> {
    Arc::new(coven::StorageError::Provider {
        provider: coven::CloudProvider::S3,
        failure: coven::StorageFailure::Network,
        source: Box::new(std::io::Error::other(
            "S3 request failed: dispatch failure: io error: client error (Connect): \
             invalid peer certificate: UnknownIssuer",
        )),
    })
}

const UNREACHABLE_S3: &str = "S3: Network: S3 request failed: dispatch failure: io error: \
     client error (Connect): invalid peer certificate: UnknownIssuer";

fn connection_record(failure: coven::StorageFailure) -> super::super::PendingItem {
    super::super::PendingItem::before_open(record(coven::PendingReason::Storage {
        provider: coven::CloudProvider::S3,
        answered: failure != coven::StorageFailure::Network,
        failure,
    }))
}

/// A retry that didn't sync answers with what the status shows for the
/// same failure: an offline pass is the network's, every layer of its
/// cause kept, and a removed device says so.
#[test]
fn a_failed_retry_answers_with_the_failure_the_status_shows() {
    use crate::ui::UiErrorCategory as C;
    let offline =
        SyncRetryOutcome::from(coven::RetryOutcome::Failed(coven::PassFailure::Offline {
            error: unreachable_s3(),
        }));
    assert!(
        matches!(&offline, SyncRetryOutcome::Failed(UiError::Diagnostic {
            category: C::Network,
            detail,
        }) if detail == UNREACHABLE_S3),
        "{offline:?}"
    );
    let removed = SyncRetryOutcome::from(coven::RetryOutcome::Failed(coven::PassFailure::Failed {
        error: coven::SyncFailure::Removed,
    }));
    assert!(
        matches!(
            removed,
            SyncRetryOutcome::Failed(UiError::Diagnostic {
                category: C::DeviceRemoved,
                ..
            })
        ),
        "{removed:?}"
    );
}

/// A slot blocked since before this process opened has no error of its
/// own; the retry's answer still says why, from the slot's reason.
#[test]
fn a_retry_held_by_a_slot_blocked_before_opening_names_its_reason() {
    let blocked =
        SyncRetryOutcome::from(coven::RetryOutcome::Failed(coven::PassFailure::Blocked {
            slot: coven::Slot(7),
            reason: coven::PendingReason::Removed,
            error: None,
        }));
    assert!(
        matches!(&blocked, SyncRetryOutcome::Failed(UiError::Diagnostic { detail, .. })
            if detail.contains("Slot(7)") && detail.contains("Removed")),
        "{blocked:?}"
    );
}

/// The provider's cooldown keeps its end, and sync not running says so.
#[test]
fn a_retry_that_could_not_run_says_why() {
    let until = std::time::UNIX_EPOCH + std::time::Duration::from_secs(600);
    assert!(matches!(
        SyncRetryOutcome::from(coven::RetryOutcome::CoolingDown { until }),
        SyncRetryOutcome::CoolingDown { until: at } if at == until
    ));
    assert!(matches!(
        SyncRetryOutcome::from(coven::RetryOutcome::NotSyncing),
        SyncRetryOutcome::NotSyncing
    ));
    assert!(matches!(
        SyncRetryOutcome::from(coven::RetryOutcome::Synced { finished_at: until }),
        SyncRetryOutcome::Synced
    ));
}

#[test]
fn an_offline_status_names_every_layer_of_its_cause() {
    let state = state(coven::SyncStatus::Offline {
        error: unreachable_s3(),
    });
    assert_eq!(
        state.snapshot().failure(),
        Some(UiError::Diagnostic {
            category: crate::ui::UiErrorCategory::Network,
            detail: UNREACHABLE_S3.to_string(),
        })
    );
}

/// The connection's record keeps only the failure's class; while the
/// status holds the error behind it, the record carries that error,
/// whichever of the two arrives first.
#[test]
fn the_connection_record_carries_the_error_behind_it() {
    let offline = coven::SyncStatus::Offline {
        error: unreachable_s3(),
    };
    let cause = |state: &SyncStatusState| {
        let Some(Ok(items)) = state.snapshot().pending else {
            panic!("the pending records were read");
        };
        items[0].wait.cause.as_ref().map(ToString::to_string)
    };

    let status_first = state(offline.clone());
    status_first.set_pending(Ok(vec![connection_record(coven::StorageFailure::Network)]));
    assert_eq!(cause(&status_first).as_deref(), Some(UNREACHABLE_S3));

    let records_first = state(coven::SyncStatus::Syncing);
    records_first.set_pending(Ok(vec![connection_record(coven::StorageFailure::Network)]));
    assert_eq!(cause(&records_first), None);
    records_first.set_status(offline);
    assert_eq!(cause(&records_first).as_deref(), Some(UNREACHABLE_S3));
    records_first.set_status(coven::SyncStatus::Syncing);
    assert_eq!(cause(&records_first), None);
}

/// A record of another failure, or a status error with no cause left (as
/// rebuilt from the record after a restart), names nothing more than the
/// record does.
#[test]
fn the_connection_record_names_only_a_matching_error_with_a_cause() {
    let other = state(coven::SyncStatus::Offline {
        error: unreachable_s3(),
    });
    other.set_pending(Ok(vec![connection_record(
        coven::StorageFailure::Authentication,
    )]));
    let restarted = state(coven::SyncStatus::Failed {
        error: coven::SyncFailure::Storage(Arc::new(coven::StorageFailure::Network.into())),
    });
    restarted.set_pending(Ok(vec![connection_record(coven::StorageFailure::Network)]));
    for state in [other, restarted] {
        let Some(Ok(items)) = state.snapshot().pending else {
            panic!("the pending records were read");
        };
        assert!(items[0].wait.cause.is_none());
    }
}

/// A failure is logged once, with its whole cause, when it appears or
/// changes; the same failure published again, or a change to anything
/// else, logs nothing more.
#[test]
fn a_new_sync_failure_logs_one_warning_with_its_cause() {
    let state = state(coven::SyncStatus::Syncing);
    let offline = coven::SyncStatus::Offline {
        error: unreachable_s3(),
    };
    let logs = crate::test_logs::capture_warn_logs(|| {
        state.set_status(offline.clone());
        state.set_status(offline.clone());
        state.set_pending(Ok(vec![connection_record(coven::StorageFailure::Network)]));
    });
    assert_eq!(logs.matches(UNREACHABLE_S3).count(), 1, "{logs}");
    assert!(logs.contains("Network"), "{logs}");

    let logs = crate::test_logs::capture_warn_logs(|| {
        state.set_status(coven::SyncStatus::Syncing);
        state.set_status(offline);
    });
    assert_eq!(logs.matches(UNREACHABLE_S3).count(), 1, "{logs}");
}

/// A blocked slot surfaces the error that blocked it, in the class of its
/// reason; a device that opened already blocked has no error to show.
#[test]
fn a_blocked_slot_surfaces_the_error_that_blocked_it() {
    let blocked = |error| SyncStatusSnapshot {
        status: coven::SyncStatus::Blocked {
            slot: coven::Slot(3),
            reason: coven::PendingReason::Storage {
                provider: coven::CloudProvider::S3,
                failure: coven::StorageFailure::Network,
                answered: false,
            },
            error,
        },
        pending: Some(Ok(Vec::new())),
        retry_effect: coven::RetryEffect::Pass,
        error: None,
    };
    assert_eq!(
        blocked(Some(
            unreachable_s3() as Arc<dyn std::error::Error + Send + Sync>
        ))
        .failure(),
        Some(UiError::Diagnostic {
            category: crate::ui::UiErrorCategory::Storage(coven::StorageFailure::Network),
            detail: UNREACHABLE_S3.to_string(),
        })
    );
    assert_eq!(blocked(None).failure(), None);
}

/// coven logs the error that blocked a slot itself, so publishing it
/// here logs nothing more.
#[test]
fn a_blocked_slot_is_not_logged_again() {
    let state = state(coven::SyncStatus::Syncing);
    let logs = crate::test_logs::capture_warn_logs(|| {
        state.set_status(coven::SyncStatus::Blocked {
            slot: coven::Slot(3),
            reason: coven::PendingReason::NoStorage,
            error: Some(unreachable_s3()),
        });
    });
    assert!(!logs.contains(UNREACHABLE_S3), "{logs}");
    assert!(state.snapshot().failure().is_some());
}

/// coven holds the error behind every record but the connection's; naming
/// the connection's from the status leaves the others' as coven gave them.
#[test]
fn a_record_keeps_the_error_coven_holds_for_it() {
    let state = state(coven::SyncStatus::Syncing);
    let mut record = coven::PendingRecord::new(
        coven::PendingSubject::Audience(coven::Audience::Store),
        coven::PendingReason::Failed(coven::LocalFailure::Disk),
    );
    record.cause = Some(Arc::new(std::io::Error::other("disk full")));
    state.set_pending(Ok(vec![super::super::PendingItem::before_open(record)]));
    state.set_status(coven::SyncStatus::Offline {
        error: unreachable_s3(),
    });
    let Some(Ok(items)) = state.snapshot().pending else {
        panic!("the pending records were read");
    };
    assert_eq!(
        items[0]
            .wait
            .cause
            .as_ref()
            .map(ToString::to_string)
            .as_deref(),
        Some("disk full")
    );
}

fn storage(failure: coven::StorageFailure) -> coven::PendingReason {
    coven::PendingReason::Storage {
        provider: coven::CloudProvider::S3,
        failure,
        answered: true,
    }
}

/// A status, the pending records read beside it, and coven's answer to
/// what a retry would do now.
fn snapshot(
    status: coven::SyncStatus,
    records: Vec<coven::PendingRecord>,
    retry_effect: coven::RetryEffect,
) -> SyncStatusSnapshot {
    SyncStatusSnapshot {
        status,
        pending: Some(Ok(records
            .into_iter()
            .map(super::super::PendingItem::before_open)
            .collect())),
        error: None,
        retry_effect,
    }
}

/// A pass whose retry runs a pass, beside `records`.
fn retried(status: coven::SyncStatus, records: Vec<coven::PendingRecord>) -> bool {
    snapshot(status, records, coven::RetryEffect::Pass).retry_offered()
}

fn file(reason: coven::PendingReason) -> coven::PendingRecord {
    coven::PendingRecord::new(
        coven::PendingSubject::Upload {
            device: coven::DeviceId(2),
            file: coven::FileId(uuid::Uuid::from_u128(9)),
        },
        reason,
    )
}

fn synced() -> coven::SyncStatus {
    coven::SyncStatus::Synced {
        finished_at: std::time::UNIX_EPOCH,
    }
}

/// Only a retry coven would run a pass for is offered: never while a
/// provider's cooldown, a failure only the person can clear or stopped sync
/// would answer it at once, whatever failed.
#[test]
fn retry_now_is_offered_only_when_coven_would_run_a_pass() {
    let offline = || coven::SyncStatus::Offline {
        error: unreachable_s3(),
    };
    assert!(snapshot(offline(), Vec::new(), coven::RetryEffect::Pass).retry_offered());
    for effect in [
        coven::RetryEffect::CoolingDown {
            until: std::time::UNIX_EPOCH + std::time::Duration::from_secs(600),
        },
        coven::RetryEffect::Failed,
        coven::RetryEffect::NotSyncing,
    ] {
        assert!(
            !snapshot(offline(), Vec::new(), effect).retry_offered(),
            "{effect:?}"
        );
    }
}

/// The pass's own failure is what its next pass retries: storage
/// unreachable or failing, or a local failure.
#[test]
fn a_failed_pass_offers_retry() {
    assert!(retried(
        coven::SyncStatus::Failed {
            error: coven::SyncFailure::Storage(Arc::new(coven::StorageFailure::Network.into())),
        },
        Vec::new(),
    ));
    assert!(retried(
        coven::SyncStatus::Failed {
            error: coven::SyncFailure::Other(Arc::new(std::io::Error::other("disk full"))),
        },
        Vec::new(),
    ));
}

/// A failure only an update, a reset or nothing clears offers no retry.
#[test]
fn a_failure_no_retry_clears_offers_none() {
    for error in [
        coven::SyncFailure::Removed,
        coven::SyncFailure::DeviceIdInUse(coven::DeviceIdEvidence::Replaced),
        coven::SyncFailure::UpdateRequired(coven::RequiredUpdate::AppSchema { version: 9 }),
    ] {
        assert!(!retried(coven::SyncStatus::Failed { error }, Vec::new()));
    }
}

/// A slot blocked by a failure coven retries offers Retry Now; one blocked
/// for good, or waiting on what another device hasn't sent, doesn't.
#[test]
fn a_blocked_slot_offers_retry_only_for_a_retried_failure() {
    let blocked = |reason: coven::PendingReason| {
        retried(
            coven::SyncStatus::Blocked {
                slot: coven::Slot(3),
                reason,
                error: None,
            },
            Vec::new(),
        )
    };
    assert!(blocked(storage(coven::StorageFailure::Network)));
    assert!(blocked(coven::PendingReason::Failed(
        coven::LocalFailure::Disk
    )));
    assert!(!blocked(coven::PendingReason::Removed));
    assert!(!blocked(storage(coven::StorageFailure::Protocol)));
    assert!(!blocked(coven::PendingReason::Waits(
        coven::Prerequisite::SequenceCatchUp
    )));
}

/// coven's own operation that failed its last attempt offers Retry Now when
/// it retries by itself, and not when it waits for the person.
#[test]
fn a_failed_operation_offers_retry_only_when_coven_retries_it() {
    let failed = |reason| {
        let operation = coven::PendingRecord::new(
            coven::PendingSubject::Operation {
                id: coven::OperationId(5),
                kind: coven::OperationKind::WriteSnapshot,
            },
            reason,
        );
        retried(
            coven::SyncStatus::Failed {
                error: coven::SyncFailure::Operation(Box::new(operation.clone())),
            },
            vec![operation],
        )
    };
    assert!(failed(storage(coven::StorageFailure::Network)));
    assert!(!failed(storage(coven::StorageFailure::Authentication)));
}

/// With the pass synced, a record of a failure coven retries by itself,
/// such as an upload a server error refused, offers Retry Now; a record
/// waiting for the person or for other work doesn't, nor does nothing
/// pending, nor a list not read yet.
#[test]
fn a_synced_library_offers_retry_for_a_retried_record() {
    let with = |reasons: Vec<coven::PendingReason>| {
        retried(synced(), reasons.into_iter().map(file).collect())
    };
    assert!(with(vec![storage(coven::StorageFailure::Network)]));
    assert!(with(vec![storage(coven::StorageFailure::RateLimited)]));
    assert!(with(vec![coven::PendingReason::Cleanup {
        operation: Box::new(storage(coven::StorageFailure::Protocol)),
        cleanup: Box::new(storage(coven::StorageFailure::Network)),
    }]));
    assert!(!with(vec![storage(coven::StorageFailure::Authentication)]));
    assert!(!with(vec![storage(coven::StorageFailure::Protocol)]));
    assert!(!with(vec![coven::PendingReason::Paused]));
    assert!(!with(vec![coven::PendingReason::Waits(
        coven::Prerequisite::SequenceCatchUp
    )]));
    assert!(!with(Vec::new()));
    let mut unread = snapshot(
        synced(),
        vec![file(storage(coven::StorageFailure::Network))],
        coven::RetryEffect::Pass,
    );
    unread.pending = None;
    assert!(!unread.retry_offered());
}
