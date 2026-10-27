use super::*;
use crate::library::output_snapshot::{OutputKind, OutputRequest};
use crate::library::release_queue::{ReleaseQueueOp, ReleaseQueueRow, ReleaseQueueState};
use crate::library::ReleaseQueueEntries;
use crate::library::UploadProgress;

fn row<Extra>(payload: Extra, release_id: &str) -> ReleaseQueueRow<Extra, ()> {
    ReleaseQueueRow {
        op: ReleaseQueueOp {
            release_id: release_id.to_string(),
            created_at: 1,
            payload,
            state: ReleaseQueueState::Queued,
        },
        release: None,
    }
}

fn export() -> OutputRequest {
    OutputRequest {
        target_dir: "/exports".into(),
        kind: OutputKind::Export,
    }
}

fn waiting() -> ReleaseUpload {
    ReleaseUpload {
        progress: UploadProgress {
            queued: 1,
            uploading: 0,
        },
        bar: None,
    }
}

#[test]
fn each_queue_shows_the_selected_releases_entry_with_its_own_pause_state() {
    let transfers = storage_inspector_transfers(
        "selected",
        &ReleaseQueueEntries {
            download: Some(row((), "selected")),
            downloads_paused: false,
            output: Some(row(export(), "selected")),
            outputs_paused: true,
        },
        Some(waiting()),
        false,
    );
    assert_eq!(
        transfers
            .iter()
            .map(|transfer| match transfer {
                StorageInspectorTransfer::Download { operation, paused } => {
                    ("download", operation.op.release_id.as_str(), *paused)
                }
                StorageInspectorTransfer::Output { operation, paused } => {
                    ("output", operation.op.release_id.as_str(), *paused)
                }
                StorageInspectorTransfer::Upload {
                    release_id, paused, ..
                } => ("upload", release_id.as_str(), *paused),
            })
            .collect::<Vec<_>>(),
        vec![
            ("download", "selected", false),
            ("output", "selected", true),
            ("upload", "selected", false),
        ]
    );
}

#[test]
fn a_release_no_queue_holds_has_no_transfers() {
    assert!(storage_inspector_transfers(
        "selected",
        &ReleaseQueueEntries {
            download: None,
            downloads_paused: false,
            output: None,
            outputs_paused: false,
        },
        None,
        false,
    )
    .is_empty());
}
