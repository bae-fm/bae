use super::*;
use crate::sync_shown::SyncStatusWord;
use crate::PendingRow;

/// How coven's sync state names a cause: the state it shows.
pub(crate) type Names = fn(coven::SyncCause) -> coven::SyncState;

/// The status from coven's sync state and the rows read beside it: the
/// records, coven's answer to what a retry would do, and why the last start
/// failed.
pub(crate) fn read(
    state: coven::SyncState,
    records: Vec<coven::PendingRecord>,
    retry_effect: coven::RetryEffect,
    error: Option<UiError>,
) -> SyncStatusSnapshot {
    SyncStatusSnapshot::read(
        coven::SyncStatus { state, last_synced: None },
        SyncStatusRows {
            pending: records
                .into_iter()
                .map(|record| PendingItem::listed(PendingRow { record, file_row: None }))
                .collect(),
            start_failure: error,
            retry_effect,
            storage_setup: None,
            provider: None,
        },
    )
}

/// coven's sync state, the pending records read beside it, and coven's
/// answer to what a retry would do now.
pub(crate) fn snapshot(
    state: coven::SyncState,
    records: Vec<coven::PendingRecord>,
    retry_effect: coven::RetryEffect,
) -> SyncStatusSnapshot {
    read(state, records, retry_effect, None)
}

/// coven's sync state with nothing pending, while sync runs.
pub(crate) fn alone(state: coven::SyncState) -> SyncStatusSnapshot {
    snapshot(state, Vec::new(), coven::RetryEffect::Pass)
}

/// A pass moving, or a file moving.
pub(crate) fn moving() -> coven::SyncState {
    coven::SyncState::Syncing
}

/// No storage location recorded on this device.
pub(crate) fn disconnected() -> coven::SyncState {
    coven::SyncState::NotSynced
}

/// The state coven's sync state names `pass` by, as `named` says.
pub(crate) fn ended(pass: coven::LastPass, named: Names) -> coven::SyncState {
    named(coven::SyncCause::Pass(pass))
}

/// The last pass blocked at a slot for `reason`, with the error that
/// blocked it, named by the state `named`.
pub(crate) fn blocked(
    reason: coven::PendingReason,
    error: Option<&str>,
    named: Names,
) -> coven::SyncState {
    ended(
        coven::LastPass::Blocked { slot: coven::Slot(3), reason, error: error.map(str::to_string) },
        named,
    )
}

/// The text coven keeps for an S3 request that never reached the provider:
/// its class, and each layer of its cause.
const UNREACHABLE_S3: &str = "S3: Network: S3 request failed: dispatch failure: io error: \
     client error (Connect): invalid peer certificate: UnknownIssuer";

pub(crate) fn offline() -> coven::SyncState {
    ended(
        coven::LastPass::Offline {
            failure: coven::StorageFailure::Network,
            error: UNREACHABLE_S3.into(),
        },
        coven::SyncState::Offline,
    )
}

/// The last pass failed as a whole with `failure`, named by the state
/// `named`.
pub(crate) fn failed_status(failure: coven::StatusFailure, named: Names) -> coven::SyncState {
    ended(coven::LastPass::Failed { failure, error: "the pass failed".into() }, named)
}

/// A failed start the status keeps beside coven's row survives the
/// pending records read with it.
#[test]
fn reading_pending_items_does_not_clear_a_start_failure() {
    let status = read(
        disconnected(),
        vec![record(coven::PendingReason::NoStorage)],
        coven::RetryEffect::Pass,
        Some(UiError::internal("connection failed")),
    );
    assert!(status.error.is_some());
}

#[test]
fn pending_files_remain_waiting_after_the_sequence_is_synced() {
    let status =
        snapshot(synced(), vec![record(coven::PendingReason::NoStorage)], coven::RetryEffect::Pass);
    assert_eq!(status.pending.len(), 1);
    assert_eq!(status.shown().status, SyncStatusWord::Synced);
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
    assert_eq!(pending_action(&record, None), PendingAction::SignIn(coven::CloudProvider::Dropbox));
}

/// A pin's failed download offers pinning the release again; with no release
/// file naming it, only its details.
#[test]
fn a_failed_pins_download_offers_pinning_its_release_again() {
    let record = coven::PendingRecord::new(
        coven::PendingSubject::Download {
            device: coven::DeviceId(2),
            object: coven::ObjectId(uuid::Uuid::from_u128(9)),
        },
        coven::PendingReason::Storage {
            provider: coven::CloudProvider::S3,
            failure: coven::StorageFailure::Network,
            answered: true,
        },
    );
    assert_eq!(
        pending_action(&record, Some("release")),
        PendingAction::RetryDownload { release_id: "release".into() }
    );
    assert_eq!(pending_action(&record, None), PendingAction::Review);
}

/// A move whose original went missing waits for it to be put back; one whose
/// original changed failed, and offers cancelling the release's upload.
#[test]
fn a_moves_original_offers_restoring_it_or_cancelling_the_upload() {
    let original = |failure| {
        coven::PendingRecord::new(
            coven::PendingSubject::Move { id: coven::MoveId(4) },
            coven::PendingReason::Original { path: "/music/a.flac".into(), failure },
        )
    };
    assert_eq!(
        pending_action(&original(coven::FileSourceFailure::Missing), Some("release")),
        PendingAction::RestoreFile
    );
    assert_eq!(
        pending_action(&original(coven::FileSourceFailure::Changed), Some("release")),
        PendingAction::CancelUpload { id: coven::MoveId(4) }
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
    assert_eq!(pending_action(&record, None), PendingAction::SignIn(coven::CloudProvider::Dropbox));
}

#[test]
fn a_failed_pass_with_no_pending_records_does_not_claim_synced() {
    let status = alone(failed_status(
        coven::StatusFailure::Storage(coven::StorageFailure::PermissionDenied),
        coven::SyncState::NoAccess,
    ));
    assert_eq!(status.shown().status, SyncStatusWord::NoAccess);
    assert!(matches!(
        status.failure(),
        Some(UiError::Diagnostic {
            category: bae_diagnostics::UiErrorCategory::Storage(
                coven::StorageFailure::PermissionDenied
            ),
            ..
        })
    ));
}

/// Only coven's syncing state is syncing: a finished, waiting, offline,
/// blocked, failed, stopped or absent sync is not, however long its loop
/// runs.
#[test]
fn only_a_moving_pass_is_syncing() {
    let syncing = |state| alone(state).syncing();
    assert!(syncing(moving()));
    assert!(!syncing(synced()));
    assert!(!syncing(coven::SyncState::WaitingToSync(coven::SyncWait::Queued)));
    assert!(!syncing(offline()));
    assert!(!syncing(blocked(coven::PendingReason::Removed, None, coven::SyncState::Removed)));
    assert!(!syncing(failed_status(coven::StatusFailure::Removed, coven::SyncState::Removed)));
    assert!(!snapshot(synced(), Vec::new(), coven::RetryEffect::NotSyncing).syncing());
    assert!(!syncing(disconnected()));
}

/// While coven's sync state is syncing, the status shows no failure: the
/// state names none.
#[test]
fn a_moving_pass_shows_no_earlier_failure() {
    let status = alone(moving());
    assert_eq!(status.shown().status, SyncStatusWord::Syncing);
    assert_eq!(status.failure(), None);
}

/// Reconnect is offered where a failed start left sync not running with
/// storage set up, and nowhere else: not without a failed start, not while
/// a loop runs, and not with no storage on this device to start sync with.
#[test]
fn only_a_failed_start_with_storage_set_up_offers_reconnect() {
    let start_failed = || Some(UiError::internal("the store key is missing"));
    let offered = |state, effect, error| read(state, Vec::new(), effect, error).reconnect_offered();
    let stopped = coven::RetryEffect::NotSyncing;
    assert!(offered(synced(), stopped, start_failed()));
    assert!(offered(offline(), stopped, start_failed()));
    assert!(!offered(synced(), stopped, None));
    assert!(!offered(disconnected(), stopped, start_failed()));
    for running in [coven::RetryEffect::Pass, coven::RetryEffect::Failed] {
        assert!(!offered(moving(), running, start_failed()));
        assert!(!offered(offline(), running, start_failed()));
        assert!(!offered(synced(), running, start_failed()));
    }
}

pub(crate) fn record(reason: coven::PendingReason) -> coven::PendingRecord {
    coven::PendingRecord::new(coven::PendingSubject::Connection, reason)
}

fn failed(failure: coven::StatusFailure, named: Names) -> SyncStatusSnapshot {
    alone(failed_status(failure, named))
}

fn category(snapshot: &SyncStatusSnapshot) -> bae_diagnostics::UiErrorCategory {
    let Some(UiError::Diagnostic { category, .. }) = snapshot.failure() else {
        panic!("the failed pass is the status's failure");
    };
    category
}

/// A removed device says it was removed, and nothing it offers would
/// register it again: its record is only reviewed, never reset.
#[test]
fn a_removed_device_says_so_and_offers_nothing_that_registers_it() {
    assert_eq!(pending_action(&record(coven::PendingReason::Removed), None), PendingAction::Review);
    assert_eq!(
        category(&failed(coven::StatusFailure::Removed, coven::SyncState::Removed)),
        bae_diagnostics::UiErrorCategory::DeviceRemoved
    );
}

/// Keys replaced by another of the person's devices are restored with the
/// person's current code, not by resetting this device.
#[test]
fn replaced_member_keys_offer_restoring_from_the_code() {
    let member = coven::MemberId(uuid::Uuid::from_u128(7));
    assert_eq!(
        pending_action(&record(coven::PendingReason::MemberKeysReplaced { member }), None),
        PendingAction::RestoreFromCode
    );
}

/// Another copy of this device is the one sync failure a reset answers.
#[test]
fn another_copy_of_this_device_is_answered_by_a_reset() {
    assert_eq!(
        category(&failed(
            coven::StatusFailure::DeviceIdInUse(coven::DeviceIdEvidence::Replaced),
            coven::SyncState::NeedsReset,
        )),
        bae_diagnostics::UiErrorCategory::DeviceResetRequired
    );
}

/// An S3 request that never reached the provider, as coven reports it to
/// the retry that waited on it: its class, and a cause whose message names
/// each layer below it.
fn unreachable_s3() -> std::sync::Arc<coven::StorageError> {
    std::sync::Arc::new(coven::StorageError::Provider {
        provider: coven::CloudProvider::S3,
        failure: coven::StorageFailure::Network,
        source: Box::new(std::io::Error::other(
            "S3 request failed: dispatch failure: io error: client error (Connect): \
             invalid peer certificate: UnknownIssuer",
        )),
    })
}

fn connection_record(failure: coven::StorageFailure) -> coven::PendingRecord {
    record(coven::PendingReason::Storage {
        provider: coven::CloudProvider::S3,
        answered: failure != coven::StorageFailure::Network,
        failure,
    })
}

/// A retry that didn't sync answers with what the status shows for the
/// same failure: an offline pass is the network's, every layer of its
/// cause kept, and a removed device says so.
#[test]
fn a_failed_retry_answers_with_the_failure_the_status_shows() {
    use bae_diagnostics::UiErrorCategory as C;
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
            SyncRetryOutcome::Failed(UiError::Diagnostic { category: C::DeviceRemoved, .. })
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
    assert_eq!(
        alone(offline()).failure(),
        Some(UiError::Diagnostic {
            category: bae_diagnostics::UiErrorCategory::Network,
            detail: UNREACHABLE_S3.to_string(),
        })
    );
}

/// The connection's record keeps only the failure's class; while the
/// status keeps the error behind it, the record carries that error.
#[test]
fn the_connection_record_carries_the_error_behind_it() {
    let cause = |status| {
        snapshot(
            status,
            vec![connection_record(coven::StorageFailure::Network)],
            coven::RetryEffect::Pass,
        )
        .pending[0]
            .wait
            .cause
            .clone()
    };
    assert_eq!(cause(offline()).as_deref(), Some(UNREACHABLE_S3));
    assert_eq!(
        cause(ended(
            coven::LastPass::Failed {
                failure: coven::StatusFailure::Storage(coven::StorageFailure::Network),
                error: "refused".into(),
            },
            coven::SyncState::Failed,
        ))
        .as_deref(),
        Some("refused")
    );
    assert_eq!(cause(moving()), None);
}

/// A record of another failure names nothing more than the record does.
#[test]
fn the_connection_record_names_only_a_matching_error() {
    let other = snapshot(
        offline(),
        vec![connection_record(coven::StorageFailure::Authentication)],
        coven::RetryEffect::Pass,
    );
    assert!(other.pending[0].wait.cause.is_none());
}

/// A failure is logged once, with its whole cause, when it appears or
/// changes; the same failure read again, or a change to anything else,
/// logs nothing more.
#[test]
fn a_new_sync_failure_is_logged_once() {
    let syncing = alone(moving());
    let failed = alone(offline());
    let logged = failed.failure_to_log(Some(&syncing)).expect("the new failure is logged");
    let UiError::Diagnostic { category, detail } = &logged;
    assert_eq!(*category, bae_diagnostics::UiErrorCategory::Network);
    assert_eq!(detail, UNREACHABLE_S3);
    assert_eq!(failed.failure_to_log(Some(&failed)), None);
    let listed = snapshot(
        offline(),
        vec![connection_record(coven::StorageFailure::Network)],
        coven::RetryEffect::Pass,
    );
    assert_eq!(listed.failure_to_log(Some(&failed)), None);
    assert_eq!(syncing.failure_to_log(Some(&failed)), None);
    assert!(failed.failure_to_log(None).is_some());
}

/// A blocked slot that names coven's sync state surfaces the error that
/// blocked it, in the class of its reason; a device that opened already
/// blocked has no error to show.
#[test]
fn a_blocked_slot_surfaces_the_error_that_blocked_it() {
    let blocked = |error: Option<&str>| {
        alone(blocked(
            coven::PendingReason::Storage {
                provider: coven::CloudProvider::S3,
                failure: coven::StorageFailure::QuotaExceeded,
                answered: true,
            },
            error,
            coven::SyncState::StorageFull,
        ))
    };
    assert_eq!(
        blocked(Some(UNREACHABLE_S3)).failure(),
        Some(UiError::Diagnostic {
            category: bae_diagnostics::UiErrorCategory::Storage(
                coven::StorageFailure::QuotaExceeded
            ),
            detail: UNREACHABLE_S3.to_string(),
        })
    );
    assert_eq!(blocked(None).failure(), None);
}

/// A state a pending record names shows no failure of its own beside the
/// records: the record shows it.
#[test]
fn a_state_a_record_names_shows_no_failure_of_its_own() {
    let removed = record(coven::PendingReason::Removed);
    let status = snapshot(
        coven::SyncState::Removed(coven::SyncCause::Pending(removed.clone())),
        vec![removed],
        coven::RetryEffect::Pass,
    );
    assert_eq!(status.failure(), None);
    assert_eq!(status.shown().status, SyncStatusWord::RemovedFromTheLibrary);
}

/// coven logs the error that blocked a slot itself, so it isn't logged
/// again, though the status shows it.
#[test]
fn a_blocked_slot_is_not_logged_again() {
    let blocked = alone(blocked(
        coven::PendingReason::Removed,
        Some(UNREACHABLE_S3),
        coven::SyncState::Removed,
    ));
    assert_eq!(blocked.failure_to_log(Some(&alone(moving()))), None);
    assert!(blocked.failure().is_some());
}

/// coven keeps the error behind every record but the connection's; naming
/// the connection's from the status leaves the others' as coven kept them.
#[test]
fn a_record_keeps_the_error_coven_keeps_for_it() {
    let mut record = coven::PendingRecord::new(
        coven::PendingSubject::Audience(coven::Audience::Store),
        coven::PendingReason::Failed(coven::LocalFailure::Disk),
    );
    record.cause = Some("disk full".into());
    let status = snapshot(offline(), vec![record], coven::RetryEffect::Pass);
    assert_eq!(status.pending[0].wait.cause.as_deref(), Some("disk full"));
}

fn storage(failure: coven::StorageFailure) -> coven::PendingReason {
    coven::PendingReason::Storage { provider: coven::CloudProvider::S3, failure, answered: true }
}

/// A pass whose retry runs a pass, beside `records`.
fn retried(state: coven::SyncState, records: Vec<coven::PendingRecord>) -> bool {
    snapshot(state, records, coven::RetryEffect::Pass).retry_offered()
}

fn file(reason: coven::PendingReason) -> coven::PendingRecord {
    coven::PendingRecord::new(
        coven::PendingSubject::Upload {
            device: coven::DeviceId(2),
            object: coven::ObjectId(uuid::Uuid::from_u128(9)),
        },
        reason,
    )
}

pub(crate) fn synced() -> coven::SyncState {
    coven::SyncState::Synced
}

/// Only a retry coven would run a pass for is offered: never while a
/// provider's cooldown, a failure only the person can clear or stopped sync
/// would answer it at once, whatever failed.
#[test]
fn retry_now_is_offered_only_when_coven_would_run_a_pass() {
    assert!(snapshot(offline(), Vec::new(), coven::RetryEffect::Pass).retry_offered());
    for effect in [
        coven::RetryEffect::CoolingDown {
            until: std::time::UNIX_EPOCH + std::time::Duration::from_secs(600),
        },
        coven::RetryEffect::Failed,
        coven::RetryEffect::NotSyncing,
    ] {
        assert!(!snapshot(offline(), Vec::new(), effect).retry_offered(), "{effect:?}");
    }
}

/// The pass's own failure is what its next pass retries: storage
/// unreachable or failing, or a local failure.
#[test]
fn a_failed_pass_offers_retry() {
    assert!(retried(
        failed_status(
            coven::StatusFailure::Storage(coven::StorageFailure::Network),
            coven::SyncState::Failed,
        ),
        Vec::new(),
    ));
    assert!(retried(
        failed_status(coven::StatusFailure::Other, coven::SyncState::Failed),
        Vec::new()
    ));
}

/// A failure only an update, a reset or nothing clears offers no retry.
#[test]
fn a_failure_no_retry_clears_offers_none() {
    let cases: [(coven::StatusFailure, Names); 3] = [
        (coven::StatusFailure::Removed, coven::SyncState::Removed),
        (
            coven::StatusFailure::DeviceIdInUse(coven::DeviceIdEvidence::Replaced),
            coven::SyncState::NeedsReset,
        ),
        (
            coven::StatusFailure::UpdateRequired(coven::RequiredUpdate::AppSchema { version: 9 }),
            coven::SyncState::UpdateNeeded,
        ),
    ];
    for (failure, named) in cases {
        assert!(!retried(failed_status(failure, named), Vec::new()));
    }
}

/// A slot blocked by a failure coven retries offers Retry now; one blocked
/// for good, or waiting on what another device hasn't sent, doesn't. A
/// block whose reason names no state leaves coven's sync state synced, and
/// its slot's record says whether it retries.
#[test]
fn a_blocked_slot_offers_retry_only_for_a_retried_failure() {
    let slot = |reason: coven::PendingReason| {
        coven::PendingRecord::new(coven::PendingSubject::Slot(coven::Slot(3)), reason)
    };
    let named = |reason: coven::PendingReason, named: Names| {
        retried(blocked(reason.clone(), None, named), vec![slot(reason)])
    };
    let unnamed = |reason: coven::PendingReason| retried(synced(), vec![slot(reason)]);
    assert!(unnamed(storage(coven::StorageFailure::Network)));
    assert!(named(
        coven::PendingReason::Failed(coven::LocalFailure::Disk),
        coven::SyncState::Failed
    ));
    assert!(!named(coven::PendingReason::Removed, coven::SyncState::Removed));
    assert!(!named(storage(coven::StorageFailure::Protocol), coven::SyncState::Failed));
    assert!(!unnamed(coven::PendingReason::Waits(coven::Prerequisite::SequenceCatchUp)));
}

/// coven's own operation that failed its last attempt offers Retry now when
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
        let named: Names = match operation.reason {
            coven::PendingReason::Storage {
                failure: coven::StorageFailure::Authentication,
                ..
            } => |cause| coven::SyncState::SignedOut {
                provider: coven::CloudProvider::S3,
                cause: Some(cause),
            },
            _ => coven::SyncState::Failed,
        };
        retried(
            failed_status(coven::StatusFailure::Operation(operation.clone()), named),
            vec![operation],
        )
    };
    assert!(failed(storage(coven::StorageFailure::Network)));
    assert!(!failed(storage(coven::StorageFailure::Authentication)));
}

/// With the pass synced, a record of a failure coven retries by itself,
/// such as an upload a server error refused, offers Retry now; a record
/// waiting for the person or for other work doesn't, nor does nothing
/// pending.
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
    assert!(!with(vec![coven::PendingReason::Waits(coven::Prerequisite::SequenceCatchUp)]));
    assert!(!with(Vec::new()));
}
