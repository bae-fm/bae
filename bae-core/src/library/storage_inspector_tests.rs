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

fn group(release_id: &str) -> UploadReleaseGroup {
    UploadReleaseGroup {
        release_id: release_id.to_string(),
        files: Vec::new(),
        progress: UploadProgress::default(),
    }
}

#[test]
fn each_queue_shows_the_selected_releases_entry_with_its_own_pause_state() {
    let groups = [group("other"), group("selected")];
    let transfers = storage_inspector_transfers(
        &ReleaseQueueEntries {
            download: Some(row((), "selected")),
            downloads_paused: false,
            output: Some(row(export(), "selected")),
            outputs_paused: true,
        },
        selected_upload_group("selected", &groups),
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
                StorageInspectorTransfer::Upload { group, paused } => {
                    ("upload", group.release_id.as_str(), *paused)
                }
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
    let groups = [group("other")];
    assert!(storage_inspector_transfers(
        &ReleaseQueueEntries {
            download: None,
            downloads_paused: false,
            output: None,
            outputs_paused: false,
        },
        selected_upload_group("selected", &groups),
        false,
    )
    .is_empty());
}

#[test]
fn joins_release_rows_and_keeps_unmatched_uploads_separate() {
    let rows = join_files(
        vec![
            ("audio".into(), "track.flac"),
            ("notes".into(), "notes.txt"),
        ],
        vec![(None, "unmatched"), (Some("audio".into()), "progress")],
    );
    assert_eq!(
        rows,
        vec![
            Joined::ReleaseFile {
                file: "track.flac",
                upload: Some("progress")
            },
            Joined::ReleaseFile {
                file: "notes.txt",
                upload: None
            },
            Joined::Upload {
                upload: "unmatched"
            },
        ]
    );
}

#[test]
fn finishing_uploads_keeps_the_contents_in_order() {
    let rows = join_files::<_, ()>(vec![("b".into(), "b.flac"), ("a".into(), "a.flac")], vec![]);
    assert_eq!(
        rows,
        vec![
            Joined::ReleaseFile {
                file: "b.flac",
                upload: None
            },
            Joined::ReleaseFile {
                file: "a.flac",
                upload: None
            },
        ]
    );
}

#[test]
fn uploads_survive_an_empty_file_set_in_queue_order() {
    let rows = join_files::<(), _>(
        vec![],
        vec![(None, "unmatched"), (Some("a".into()), "queued")],
    );
    assert_eq!(
        rows,
        vec![
            Joined::Upload {
                upload: "unmatched"
            },
            Joined::Upload { upload: "queued" },
        ]
    );
}
