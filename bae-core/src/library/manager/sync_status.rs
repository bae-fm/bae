//! bae's flat sync-status banner state, folded from coven's sync-loop status.
//!
//! coven reports a cycle's outcome as a `SyncLoopStatus`; bae keeps the parts a
//! banner renders — the fault of a failed cycle, the last successful sync time,
//! and the durable operations a completed cycle left waiting on a person. Each
//! of those operations failed once with a fault that running it again cannot
//! change, so later cycles skip it and it runs again only when the person asks;
//! the front-ends render it as a kind they localize, an id they hand back to
//! `LibraryManager::retry_blocked_sync_operation`, and a description and error
//! they show as detail.

use coven::SyncLoopStatus;
use tracing::warn;

use super::LibraryError;
use crate::db::Database;
use crate::ui::{UiError, UiErrorCategory};

/// The banner state bae maintains across cycles. Each field holds what the last
/// status with a verdict on it said.
#[derive(Debug, Clone)]
pub(super) struct SyncStatusState {
    pub(super) error: Option<UiError>,
    pub(super) blocked: Vec<BlockedSyncOperation>,
    pub(super) last_sync_time_raw: Option<String>,
    pub(super) last_sync_time: Option<i64>,
    pub(super) syncing: bool,
}

impl SyncStatusState {
    pub(super) fn initial(database: &Database) -> Self {
        Self {
            error: None,
            blocked: Vec::new(),
            last_sync_time_raw: None,
            last_sync_time: None,
            syncing: database.is_syncing(),
        }
    }
}

/// What one sync-loop status says about the banner state.
///
/// A field is `None` when the status has no verdict on it: a cycle in progress
/// and an offline loop say nothing at all, and a failed cycle says nothing about
/// the blocked operations because it never got as far as reading them — they are
/// durable, so the list stands until a cycle completes and enumerates it again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SyncStatusUpdate {
    /// `Some(None)` clears the banner, `Some(Some(fault))` sets it.
    pub(super) error: Option<Option<UiError>>,
    /// The RFC 3339 time of a completed cycle.
    pub(super) last_sync_time: Option<String>,
    pub(super) blocked: Option<Vec<BlockedSyncOperation>>,
}

impl SyncStatusUpdate {
    pub(super) fn from_loop_status(status: &SyncLoopStatus) -> Self {
        match status {
            SyncLoopStatus::CheckingStorage
            | SyncLoopStatus::Publishing
            | SyncLoopStatus::Offline => Self {
                error: None,
                last_sync_time: None,
                blocked: None,
            },
            SyncLoopStatus::Synchronized(success) => Self {
                error: Some(held_sync_error(&success.alerts.held_positions)),
                last_sync_time: Some(success.last_sync_time.clone()),
                blocked: Some(Vec::new()),
            },
            SyncLoopStatus::Blocked {
                success,
                operations,
            } => Self {
                error: Some(held_sync_error(&success.alerts.held_positions)),
                last_sync_time: Some(success.last_sync_time.clone()),
                blocked: Some(
                    operations
                        .iter()
                        .map(BlockedSyncOperation::from_coven)
                        .collect(),
                ),
            },
            SyncLoopStatus::Failed { error } => {
                // The loop itself never logs its fault (it only ships it into
                // the status watch), and the UI localizes it down to a generic
                // line — without this record a failing cycle is invisible in
                // the log.
                warn!("sync loop failed: {error}");
                Self {
                    error: Some(Some(UiError::internal(error))),
                    last_sync_time: None,
                    blocked: None,
                }
            }
        }
    }
}

/// A completed network cycle may still have refused remote updates. Keep those
/// reasons visible; completion alone does not mean the library is synchronized.
fn held_sync_error(positions: &[coven::HeldStorePosition]) -> Option<UiError> {
    if positions.is_empty() {
        return None;
    }
    let category = if positions.iter().any(|position| {
        matches!(
            position.reason,
            coven::HeldStorePositionReason::NewerSchema { .. }
        )
    }) {
        UiErrorCategory::SyncUpdateRequired
    } else {
        UiErrorCategory::Internal
    };
    Some(UiError::diagnostic(
        category,
        format!("Sync held remote updates: {positions:?}"),
    ))
}

/// One durable sync operation that cannot proceed until a person acts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockedSyncOperation {
    /// Names this operation to `retry_blocked_sync_operation`. Opaque to the
    /// front-ends, which only carry it back.
    pub id: String,
    pub kind: BlockedSyncOperationKind,
    /// Which operation this is: the rows a write touches, the circle an
    /// operation belongs to, what a reclaim was going to delete. coven's own
    /// vocabulary, shown under the localized kind and never translated.
    pub description: String,
    /// coven's reason the operation stopped. Never translated.
    pub error: String,
}

/// Which of coven's durable operation journals a blocked operation came from.
/// The front-ends localize the name; all three retry through the same call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockedSyncOperationKind {
    /// A row write stopped by a semantic publication fault.
    Write,
    /// A circle operation whose author lost its Store write authority or the
    /// stream position it was prepared against.
    CircleOperation,
    /// A reclaim — the deletion of one stored object — whose last run failed.
    Reclaim,
}

/// The tag each encoded id opens with, naming which retry path it takes.
const WRITE_TAG: &str = "write";
const CIRCLE_OPERATION_TAG: &str = "circle-operation";
const RECLAIM_TAG: &str = "reclaim";

impl BlockedSyncOperation {
    pub(crate) fn from_coven(operation: &coven::BlockedOperation) -> Self {
        let id = encode_id(&operation.id());
        match operation {
            coven::BlockedOperation::Write(write) => Self {
                id,
                kind: BlockedSyncOperationKind::Write,
                description: write_description(write),
                error: write_error(write),
            },
            coven::BlockedOperation::CircleOperation(circle_operation) => Self {
                id,
                kind: BlockedSyncOperationKind::CircleOperation,
                description: circle_operation_description(circle_operation),
                error: circle_operation_error(circle_operation),
            },
            coven::BlockedOperation::Reclaim(reclaim) => Self {
                id,
                kind: BlockedSyncOperationKind::Reclaim,
                description: reclaim_description(&reclaim.target).to_string(),
                // Which object, in front of coven's reason. The reason names
                // what went wrong but not what it went wrong on, and a person
                // reporting this needs both.
                error: format!(
                    "object {}: {}",
                    reclaim.target.object().slot().logical_key(),
                    reclaim.error
                ),
            },
        }
    }
}

/// Render one blocked operation's identity as the single string the bridge
/// carries to the front-ends and back. The leading tag says which retry path
/// `decode_id` reconstructs; the rest is the kind's own id, which may itself
/// contain `:`.
fn encode_id(id: &coven::BlockedOperationId) -> String {
    match id {
        coven::BlockedOperationId::Write(write_id) => format!("{WRITE_TAG}:{write_id}"),
        coven::BlockedOperationId::CircleOperation(operation_id) => {
            format!("{CIRCLE_OPERATION_TAG}:{}", operation_id.as_str())
        }
        coven::BlockedOperationId::Reclaim(operation_id) => {
            format!("{RECLAIM_TAG}:{operation_id}")
        }
    }
}

/// Read back an id this crate encoded. A string that is not one is a front-end
/// sending something it never received, which the caller sees as an error rather
/// than a silently skipped retry.
pub(crate) fn decode_id(value: &str) -> Result<coven::BlockedOperationId, LibraryError> {
    let Some((tag, id)) = value.split_once(':') else {
        return Err(LibraryError::Internal(format!(
            "blocked sync operation id {value:?} has no kind tag"
        )));
    };
    match tag {
        WRITE_TAG => Ok(coven::BlockedOperationId::Write(
            coven::WriteId::from_generated(id.to_string()),
        )),
        CIRCLE_OPERATION_TAG => Ok(coven::BlockedOperationId::CircleOperation(
            coven::CircleOperationId::from_write_id(coven::WriteId::from_generated(id.to_string())),
        )),
        RECLAIM_TAG => id
            .parse()
            .map(coven::BlockedOperationId::Reclaim)
            .map_err(|error| {
                LibraryError::Internal(format!("blocked sync operation id {value:?}: {error}"))
            }),
        _ => Err(LibraryError::Internal(format!(
            "blocked sync operation id {value:?} names no operation kind"
        ))),
    }
}

/// The rows a blocked write would have published, or the write's own id when it
/// names none.
fn write_description(write: &coven::PendingWrite) -> String {
    if write.affected_rows.is_empty() {
        return format!("write {}", write.write_id);
    }
    write
        .affected_rows
        .iter()
        .map(|row| format!("{}/{}", row.table, row.primary_key))
        .collect::<Vec<_>>()
        .join(", ")
}

fn write_error(write: &coven::PendingWrite) -> String {
    match &write.status {
        coven::WriteStatus::Blocked(block) => write_block(block),
        status => {
            warn!("coven reported a blocked write whose status is {status:?}");
            format!("the write is not blocked: {status:?}")
        }
    }
}

fn write_block(block: &coven::WriteBlock) -> String {
    match block {
        coven::WriteBlock::InvalidPackage { reason } => format!("invalid package: {reason}"),
        coven::WriteBlock::InvalidProtocolState { reason } => {
            format!("invalid protocol state: {reason}")
        }
        coven::WriteBlock::MissingBlob { namespace, id } => {
            format!("blob {namespace}/{id} is missing")
        }
        coven::WriteBlock::LocalUserBlob { namespace, id } => {
            format!("blob {namespace}/{id} is a local user file and cannot be published")
        }
        coven::WriteBlock::RotationRequired {
            circle_id,
            removed_members,
        } => format!(
            "circle {circle_id} must rotate its key after removing {}",
            removed_members.join(", ")
        ),
    }
}

/// What a reclaim was going to delete from the cloud, named rather than
/// located — the object's storage key is technical enough to belong with the
/// error, but the row above it should say what the thing is.
fn reclaim_description(target: &coven::ReclaimTarget) -> &'static str {
    match target {
        coven::ReclaimTarget::StorePackage(_) => "a published batch of library changes",
        coven::ReclaimTarget::CirclePackage(_) => "a published batch of circle changes",
        coven::ReclaimTarget::CircleBootstrapImage(_) => "a circle's starting image",
        coven::ReclaimTarget::CircleSnapshotImage(_) => "a circle's snapshot image",
        coven::ReclaimTarget::StoreMembershipRollup(_) => "the library's membership record",
        coven::ReclaimTarget::AudienceBlob(_) => "a stored file",
    }
}

fn circle_operation_description(operation: &coven::CircleOperationInfo) -> String {
    let kind = match operation.kind {
        coven::CircleOperationKind::Create => "create",
        coven::CircleOperationKind::Rename => "rename",
        coven::CircleOperationKind::AddMember => "add member",
        coven::CircleOperationKind::RemoveMember => "remove member",
        coven::CircleOperationKind::ResolveControl => "resolve control",
        coven::CircleOperationKind::Delete => "delete",
    };
    format!("{kind} on circle {}", operation.circle_id)
}

fn circle_operation_error(operation: &coven::CircleOperationInfo) -> String {
    match &operation.state {
        coven::CircleOperationState::Blocked { block } => block.to_string(),
        state => {
            warn!("coven reported a blocked circle operation whose state is {state:?}");
            format!("the circle operation is not blocked: {state:?}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn success() -> coven::SyncLoopSuccess {
        coven::SyncLoopSuccess {
            last_sync_time: "2026-01-02T03:04:05Z".to_string(),
            device_count: 1,
            device_activity: Vec::new(),
            data_changed: false,
            row_changes: None,
            alerts: coven::SyncLoopAlerts {
                rotation_pending: None,
                held_positions: Vec::new(),
                local_blob_cleanup_pending: false,
            },
        }
    }

    #[test]
    fn a_newer_schema_is_not_reported_as_successful_sync() {
        let mut success = success();
        success
            .alerts
            .held_positions
            .push(coven::HeldStorePosition {
                coordinate: coven::HeldStoreCoordinate::Package {
                    device_id: "peer".to_string(),
                    seq: 90,
                    package_hash: coven::ObjectHash::digest(b"newer schema package"),
                },
                reason: coven::HeldStorePositionReason::NewerSchema {
                    local: 15,
                    required: 17,
                },
            });
        for status in [
            SyncLoopStatus::Synchronized(success.clone()),
            SyncLoopStatus::Blocked {
                success,
                operations: vec![blocked_write("write-1")],
            },
        ] {
            let update = SyncStatusUpdate::from_loop_status(&status);
            assert!(!update
                .error
                .as_ref()
                .unwrap()
                .as_ref()
                .unwrap()
                .can_reconnect_sync());
            assert!(
                matches!(
                    update.error,
                    Some(Some(UiError::Diagnostic {
                        category: UiErrorCategory::SyncUpdateRequired,
                        ..
                    }))
                ),
                "held updates must require an app update"
            );
        }
    }

    #[test]
    fn other_held_updates_report_their_reason_without_requesting_an_app_update() {
        let mut success = success();
        success
            .alerts
            .held_positions
            .push(coven::HeldStorePosition {
                coordinate: coven::HeldStoreCoordinate::Package {
                    device_id: "peer".to_string(),
                    seq: 90,
                    package_hash: coven::ObjectHash::digest(b"missing package"),
                },
                reason: coven::HeldStorePositionReason::MissingCommit,
            });
        let update = SyncStatusUpdate::from_loop_status(&SyncLoopStatus::Synchronized(success));
        let error = update.error.unwrap().expect("held updates remain visible");
        assert!(error.can_reconnect_sync());
        assert!(matches!(error, UiError::Diagnostic {
            category: UiErrorCategory::Internal,
            ref detail,
        } if detail.contains("MissingCommit")));
    }

    fn blocked_write(write_id: &str) -> coven::BlockedOperation {
        coven::BlockedOperation::Write(coven::PendingWrite {
            write_id: coven::WriteId::from_generated(write_id.to_string()),
            status: coven::WriteStatus::Blocked(coven::WriteBlock::MissingBlob {
                namespace: "release_files".to_string(),
                id: "file-7".to_string(),
            }),
            affected_rows: vec![coven::AffectedRow {
                table: "releases".to_string(),
                primary_key: "release-3".to_string(),
            }],
        })
    }

    /// A completed cycle that left an operation waiting reports it with the
    /// detail the person needs to decide: what it was, and why it stopped.
    #[test]
    fn a_blocked_cycle_reports_its_operations() {
        let update = SyncStatusUpdate::from_loop_status(&SyncLoopStatus::Blocked {
            success: success(),
            operations: vec![blocked_write("write-1")],
        });

        assert_eq!(update.error, Some(None), "a blocked cycle is not a failure");
        assert_eq!(
            update.last_sync_time.as_deref(),
            Some("2026-01-02T03:04:05Z")
        );
        assert_eq!(
            update.blocked,
            Some(vec![BlockedSyncOperation {
                id: "write:write-1".to_string(),
                kind: BlockedSyncOperationKind::Write,
                description: "releases/release-3".to_string(),
                error: "blob release_files/file-7 is missing".to_string(),
            }])
        );
    }

    /// A cycle that completes clean has enumerated the journals and found
    /// nothing waiting, so it clears the list rather than leaving the last
    /// cycle's operations on screen after they were resolved.
    #[test]
    fn a_clean_cycle_clears_the_blocked_list() {
        let update = SyncStatusUpdate::from_loop_status(&SyncLoopStatus::Synchronized(success()));

        assert_eq!(update.error, Some(None));
        assert_eq!(update.blocked, Some(Vec::new()));
    }

    /// A failed cycle records its fault whole, so a surface has something to
    /// render besides the category line — and says nothing about the blocked
    /// operations, because it never got as far as reading them. They are
    /// durable: the list from the last cycle that did read them still stands.
    #[test]
    fn a_failed_cycle_reports_its_fault_and_leaves_the_blocked_list_alone() {
        let update = SyncStatusUpdate::from_loop_status(&SyncLoopStatus::Failed {
            error: coven::SyncLoopFailure::Storage(std::sync::Arc::new(
                coven::StorageError::Storage("the bucket refused the request".to_string()),
            )),
        });

        assert_eq!(
            update.error,
            Some(Some(UiError::internal(
                "check sync storage: storage operation failed: the bucket refused the request"
            )))
        );
        assert_eq!(update.last_sync_time, None);
        assert_eq!(
            update.blocked, None,
            "a cycle that never enumerated the journals has no verdict on them"
        );
    }

    /// A cycle still running says nothing about either the fault or the blocked
    /// operations, so neither is disturbed mid-cycle.
    #[test]
    fn a_cycle_in_progress_has_no_verdict() {
        let update = SyncStatusUpdate::from_loop_status(&SyncLoopStatus::Publishing);

        assert_eq!(update.error, None);
        assert_eq!(update.last_sync_time, None);
        assert_eq!(update.blocked, None);
    }

    /// The id a blocked operation crosses the bridge with names the same
    /// operation when it comes back, whichever kind it is.
    #[test]
    fn an_encoded_id_names_the_same_operation() {
        for id in [
            coven::BlockedOperationId::Write(coven::WriteId::from_generated(
                "write-with:colon".to_string(),
            )),
            coven::BlockedOperationId::CircleOperation(coven::CircleOperationId::from_write_id(
                coven::WriteId::from_generated("operation-4".to_string()),
            )),
            coven::BlockedOperationId::Reclaim(coven::ObjectHash::digest(b"an object")),
        ] {
            assert_eq!(decode_id(&encode_id(&id)).unwrap(), id);
        }
    }

    /// An id the UI never received names no operation, so the retry is refused
    /// rather than dispatched at a guess.
    #[test]
    fn an_unknown_id_is_refused() {
        assert!(decode_id("reclaim:not-a-hash").is_err());
        assert!(decode_id("nonsense:1").is_err());
        assert!(decode_id("no-tag-at-all").is_err());
    }
}
