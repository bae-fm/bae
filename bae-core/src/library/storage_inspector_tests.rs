use super::*;
use crate::album_detail::{FileKind, FilePin, FilePlace};
use crate::library::output_snapshot::{OutputKind, OutputRequest};
use crate::library::release_queue::{ReleaseQueueOp, ReleaseQueueRow, ReleaseQueueState};
use crate::library::ReleaseQueueEntries;
use crate::library::UploadProgress;

fn row<Extra>(
    payload: Extra,
    release_id: &str,
    state: ReleaseQueueState<()>,
) -> ReleaseQueueRow<Extra, ()> {
    ReleaseQueueRow {
        op: ReleaseQueueOp {
            release_id: release_id.to_string(),
            created_at: 1,
            payload,
            state,
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
        stored: 0,
        bar: None,
    }
}

fn file(id: &str, pin: Option<FilePin>) -> FileDetail {
    FileDetail {
        id: id.to_string(),
        original_filename: format!("{id}.flac"),
        place: FilePlace::of(&format!("{id}.flac")),
        kind: FileKind::Audio {
            format: "FLAC".into(),
        },
        file_size: 10,
        is_image: false,
        content_type: "audio/flac".into(),
        source_audio: None,
        upload: None,
        pin,
    }
}

fn shown(
    files: Vec<FileDetail>,
    entries: &ReleaseQueueEntries,
    upload: Option<ReleaseUpload>,
) -> StorageInspector {
    StorageInspector::of(
        "selected",
        files,
        entries,
        upload,
        &coven::PendingCauses::default(),
    )
}

#[test]
fn each_queue_shows_the_selected_releases_entry() {
    let inspector = shown(
        Vec::new(),
        &ReleaseQueueEntries {
            download: Some(row((), "selected", ReleaseQueueState::Queued)),
            output: Some(row(export(), "selected", ReleaseQueueState::Queued)),
        },
        Some(waiting()),
    );
    assert_eq!(
        inspector
            .transfers
            .iter()
            .map(|transfer| match transfer {
                StorageInspectorTransfer::Download { operation, .. } => {
                    ("download", operation.op.release_id.as_str())
                }
                StorageInspectorTransfer::PinFetch { .. } => ("pin fetch", "selected"),
                StorageInspectorTransfer::Output { operation } => {
                    ("output", operation.op.release_id.as_str())
                }
                StorageInspectorTransfer::Upload { release_id, .. } => {
                    ("upload", release_id.as_str())
                }
            })
            .collect::<Vec<_>>(),
        vec![
            ("download", "selected"),
            ("output", "selected"),
            ("upload", "selected"),
        ]
    );
}

#[test]
fn a_release_no_queue_holds_has_no_transfers() {
    let inspector = shown(
        vec![file("a", Some(FilePin::Kept))],
        &ReleaseQueueEntries {
            download: None,
            output: None,
        },
        None,
    );
    assert!(inspector.transfers.is_empty());
    assert_eq!(
        inspector.files[0].pin, None,
        "no pin shows out of the queue"
    );
}

/// While its download waits or runs, each file says whether it is kept and
/// the download counts them; a failed download shows neither, and leaves a
/// pin waiting for its copy to coven's fetch.
#[test]
fn a_release_pinning_shows_each_files_pin_and_counts_them() {
    let files = || {
        vec![
            file("a", Some(FilePin::Kept)),
            file("b", Some(FilePin::Waiting)),
            file("c", None),
        ]
    };
    for state in [
        ReleaseQueueState::Queued,
        ReleaseQueueState::Active { progress: () },
    ] {
        let inspector = shown(
            files(),
            &ReleaseQueueEntries {
                download: Some(row((), "selected", state)),
                output: None,
            },
            None,
        );
        assert_eq!(
            inspector
                .files
                .iter()
                .map(|file| file.pin)
                .collect::<Vec<_>>(),
            vec![
                Some(FilePin::Kept),
                Some(FilePin::Waiting),
                Some(FilePin::Waiting)
            ]
        );
        assert!(matches!(
            inspector.transfers.as_slice(),
            [StorageInspectorTransfer::Download {
                files: Some(PinFiles {
                    kept: 1,
                    waiting: 2
                }),
                ..
            }]
        ));
    }

    let failed = shown(
        files(),
        &ReleaseQueueEntries {
            download: Some(row(
                (),
                "selected",
                ReleaseQueueState::Failed {
                    error: "the network connection was lost".into(),
                },
            )),
            output: None,
        },
        None,
    );
    assert_eq!(
        failed.files.iter().map(|file| file.pin).collect::<Vec<_>>(),
        vec![Some(FilePin::Kept), Some(FilePin::Waiting), None],
        "coven still fetches the copy the pin waits for"
    );
    assert!(matches!(
        failed.transfers.as_slice(),
        [
            StorageInspectorTransfer::Download { files: None, .. },
            StorageInspectorTransfer::PinFetch {
                files: PinFiles {
                    kept: 1,
                    waiting: 1
                }
            }
        ]
    ));
}

/// Out of the download queue, a release whose pins wait for coven to fetch
/// their copies is pinning: each pinned file says whether it is kept, a file
/// without a pin shows none, and coven's fetch counts them. Once no pin
/// waits, nothing pins it.
#[test]
fn a_release_whose_pins_wait_for_their_copies_shows_coven_fetching_them() {
    let no_queue = ReleaseQueueEntries {
        download: None,
        output: None,
    };
    let fetching = shown(
        vec![
            file("a", Some(FilePin::Kept)),
            file("b", Some(FilePin::Waiting)),
            file("c", None),
        ],
        &no_queue,
        None,
    );
    assert_eq!(
        fetching
            .files
            .iter()
            .map(|file| file.pin)
            .collect::<Vec<_>>(),
        vec![Some(FilePin::Kept), Some(FilePin::Waiting), None]
    );
    assert_eq!(
        fetching.transfers,
        vec![StorageInspectorTransfer::PinFetch {
            files: PinFiles {
                kept: 1,
                waiting: 1
            }
        }]
    );

    let kept = shown(
        vec![
            file("a", Some(FilePin::Kept)),
            file("b", Some(FilePin::Kept)),
        ],
        &no_queue,
        None,
    );
    assert!(kept.transfers.is_empty());
    assert!(kept.files.iter().all(|file| file.pin.is_none()));
}
