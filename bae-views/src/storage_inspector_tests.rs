//! The Storage Manager inspector's read of one release.

use super::*;
use bae_records::test_fixtures::aggregate_ordering::{
    insert_files_out_of_natural_order, release_detail_db, FILES_IN_NATURAL_ORDER,
};
use bae_records::test_fixtures::*;
use bae_store::test_fixtures::*;
use bae_transfers::{DownloadRequest, ReleaseQueueEntries};
use bae_transfers::{OutputKind, OutputRequest, ReleaseQueueEntry};
use bae_transfers::{ReleaseQueueOp, ReleaseQueueRow, ReleaseQueueState};

fn row<Extra>(
    payload: Extra,
    release_id: &str,
    state: ReleaseQueueState<()>,
) -> ReleaseQueueRow<Extra, ()> {
    ReleaseQueueRow {
        op: ReleaseQueueOp {
            release_id: release_id.to_string(),
            created_at: 1,
            file_count: 0,
            total_size: 0,
            payload,
            state,
        },
        release: None,
    }
}

fn export() -> OutputRequest {
    OutputRequest { target_dir: "/exports".into(), kind: OutputKind::Export }
}

fn waiting() -> bae_records::DbReleaseUpload {
    bae_records::DbReleaseUpload { waiting: 1, sending: 0, stored: 0, missing: None }
}

fn file(id: &str) -> crate::DbStorageInspectorFile {
    crate::DbStorageInspectorFile {
        file: bae_records::DbFile {
            id: id.to_string(),
            release_id: "selected".to_string(),
            original_filename: format!("{id}.flac"),
            file_size: 10,
            content_type: bae_util::content_type::ContentType::Flac,
            role: bae_records::ReleaseFileRole::Other,
            source_audio: None,
            file: coven::FileId(uuid::Uuid::nil()),
            stored: false,
            created_at: time::UtcDateTime::UNIX_EPOCH,
        },
        download: None,
        upload: None,
        move_record: None,
    }
}

/// Each count of a summary, by its label's key.
fn counts(summary: &[bae_transfers::TransferCount]) -> Vec<(&str, u64)> {
    summary.iter().map(|count| (count.label.key.as_str(), count.label.count)).collect()
}

fn shown(
    files: Vec<crate::DbStorageInspectorFile>,
    entries: &ReleaseQueueEntries,
    pins: Option<crate::DbPinCounts>,
    upload: Option<bae_records::DbReleaseUpload>,
) -> StorageInspector {
    StorageInspector::read(crate::DbStorageInspector {
        release_id: "selected".to_string(),
        release: crate::DbInspectorRelease {
            title: "Album Title".to_string(),
            file_count: 9,
            total_size: 356_000_000,
        },
        files,
        pins,
        upload,
        failed_move: None,
        entries: entries.clone(),
    })
}

#[test]
fn each_queue_shows_the_selected_releases_entry() {
    let inspector = shown(
        Vec::new(),
        &ReleaseQueueEntries {
            download: Some(row(DownloadRequest::Pin, "selected", ReleaseQueueState::Queued)),
            output: Some(row(export(), "selected", ReleaseQueueState::Queued)),
        },
        None,
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
                StorageInspectorTransfer::Output { operation } => {
                    ("output", operation.op.release_id.as_str())
                }
                StorageInspectorTransfer::Upload { release_id, .. } => {
                    ("upload", release_id.as_str())
                }
                StorageInspectorTransfer::MoveFailed { release_id, .. } => {
                    ("move failed", release_id.as_str())
                }
            })
            .collect::<Vec<_>>(),
        vec![("download", "selected"), ("output", "selected"), ("upload", "selected"),]
    );
}

/// An upload names its release's album and its files' count and size, which
/// its row shows while it waits for a missing original.
#[test]
fn an_upload_names_its_release() {
    let inspector = shown(
        Vec::new(),
        &ReleaseQueueEntries { download: None, output: None },
        None,
        Some(waiting()),
    );
    let [StorageInspectorTransfer::Upload { release, .. }] = inspector.transfers.as_slice() else {
        panic!("only the upload is queued: {:?}", inspector.transfers);
    };
    assert_eq!(
        release,
        &crate::DbInspectorRelease {
            title: "Album Title".to_string(),
            file_count: 9,
            total_size: 356_000_000,
        }
    );
}

fn original_record(failure: coven::FileSourceFailure, cause: Option<&str>) -> coven::PendingRecord {
    let mut record = coven::PendingRecord::new(
        coven::PendingSubject::Connection,
        coven::PendingReason::Original { failure, path: "/music/a.flac".into() },
    );
    record.cause = cause.map(str::to_string);
    record
}

/// A file whose move found its original missing, or changed, shows it as its
/// problem; a file with no such record shows none.
#[test]
fn a_file_shows_what_its_moves_record_says_of_its_original() {
    let problems = |failure| {
        let mut stopped = file("stopped");
        stopped.move_record = Some(original_record(failure, None));
        let whole = file("whole");
        shown(
            vec![stopped, whole],
            &ReleaseQueueEntries { download: None, output: None },
            None,
            None,
        )
        .files
        .into_iter()
        .map(|file| file.problem)
        .collect::<Vec<_>>()
    };
    assert_eq!(
        problems(coven::FileSourceFailure::Missing),
        vec![Some(FileProblem::OriginalMissing), None]
    );
    assert_eq!(
        problems(coven::FileSourceFailure::Changed),
        vec![Some(FileProblem::OriginalChanged), None]
    );
}

/// A release whose move failed shows the failed move, naming the file and
/// carrying the text of the error behind it, with its album and its files'
/// count and size.
#[test]
fn a_failed_move_names_its_file_and_the_error_behind_it() {
    let mut changed = file("changed");
    changed.move_record =
        Some(original_record(coven::FileSourceFailure::Changed, Some("size differs")));
    let inspector = StorageInspector::read(crate::DbStorageInspector {
        release_id: "selected".to_string(),
        release: crate::DbInspectorRelease {
            title: "Album Title".to_string(),
            file_count: 9,
            total_size: 356_000_000,
        },
        files: vec![file("other"), changed],
        pins: None,
        upload: None,
        failed_move: Some(crate::DbFailedMove {
            file_name: "changed.flac".to_string(),
            cause: Some("size differs".to_string()),
        }),
        entries: ReleaseQueueEntries { download: None, output: None },
    });
    let [StorageInspectorTransfer::MoveFailed { file_name, cause, release, .. }] =
        inspector.transfers.as_slice()
    else {
        panic!("only the failed move is shown: {:?}", inspector.transfers);
    };
    assert_eq!(file_name, "changed.flac");
    assert_eq!(cause.as_deref(), Some("size differs"));
    assert_eq!(release.title, "Album Title");
}

#[test]
fn a_release_no_queue_holds_has_no_transfers() {
    let inspector =
        shown(vec![file("a")], &ReleaseQueueEntries { download: None, output: None }, None, None);
    assert!(inspector.transfers.is_empty());
}

/// The download of a pinning release counts its files by pin state and
/// leaves out each count that is zero, and counts nothing for a release that
/// is not pinning.
#[test]
fn a_pinning_releases_download_counts_its_files() {
    let entries = ReleaseQueueEntries {
        download: Some(row(DownloadRequest::Pin, "selected", ReleaseQueueState::Queued)),
        output: None,
    };
    let inspector = shown(
        Vec::new(),
        &entries,
        Some(crate::DbPinCounts { downloading: 1, waiting: 2, kept: 0 }),
        None,
    );
    let [StorageInspectorTransfer::Download { summary: Some(summary), .. }] =
        inspector.transfers.as_slice()
    else {
        panic!("a pinning release's download counts its files");
    };
    assert_eq!(counts(summary), vec![("core.queue.pinning", 1), ("core.queue.waiting", 2)]);

    let resting = shown(Vec::new(), &entries, None, None);
    assert!(matches!(
        resting.transfers.as_slice(),
        [StorageInspectorTransfer::Download { summary: None, .. }]
    ));
}

fn download(release_id: &str) -> ReleaseQueueEntry<DownloadRequest> {
    ReleaseQueueEntry {
        release_id: release_id.to_string(),
        created_at: 7,
        payload: DownloadRequest::Pin,
    }
}

/// The inspector's read of one release holds its entry in each queue and
/// nothing of another release's, and reads no release the library does not
/// hold.
#[cfg(desktop)]
#[tokio::test]
async fn the_inspectors_read_holds_the_releases_own_entries() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    seed_artist_album_release(&db).await;
    bae_transfers::Downloads::new(db.clone())
        .enqueue_all(vec![download(RELEASE_A), download(RELEASE_B)])
        .await
        .unwrap();
    bae_transfers::Outputs::new(db.clone())
        .enqueue_all(vec![ReleaseQueueEntry {
            release_id: RELEASE_B.to_string(),
            created_at: 3,
            payload: OutputRequest {
                target_dir: "/Users/ana/Music/Copies".into(),
                kind: OutputKind::Export,
            },
        }])
        .await
        .unwrap();

    let mut live = crate::LibraryViews::new(db.clone())
        .subscribe_storage_inspector(Some(RELEASE_B.to_string()));
    let read = live.next_inline().await.expect("the query stays open").result.unwrap();
    assert_eq!(read, None, "the library holds no RELEASE_B");

    live.requests().set(Some(RELEASE_A.to_string())).expect("the query is open");
    let read = loop {
        let event = live.next_inline().await.expect("the query stays open");
        if event.request.as_deref() == Some(RELEASE_A) {
            break event.result.unwrap().expect("the library holds RELEASE_A");
        }
    };
    let [StorageInspectorTransfer::Download { operation, .. }] = read.transfers.as_slice() else {
        panic!("only RELEASE_A's download is queued: {:?}", read.transfers);
    };
    assert_eq!(operation.op.release_id, RELEASE_A);
}

/// The inspector's file details read the one file they show by its id, and
/// read nothing for an id the library holds no file for.
#[tokio::test]
async fn the_file_details_read_the_file_by_its_id() {
    let (db, tmp) = release_detail_db(crate::TEST_SCHEMA).await;
    insert_files_out_of_natural_order(&db, &tmp).await;
    let cover = bae_util::test_ids::test_uuid("ordering-file-1");

    let mut live = crate::LibraryViews::new(db.clone()).subscribe_storage_file(Some(cover.clone()));
    let read = live.next_inline().await.expect("the query stays open").result.unwrap();
    assert_eq!(read.map(|file| file.original_filename), Some("cover.jpg".to_string()));

    live.requests().set(Some("no-such-file".to_string())).expect("the query is open");
    let read = loop {
        let event = live.next_inline().await.expect("the query stays open");
        if event.request.as_deref() == Some("no-such-file") {
            break event.result.unwrap();
        }
    };
    assert_eq!(read, None, "the library holds no such file");
}

/// A queued move to this device marks each file of the selected release.
#[tokio::test]
async fn a_queued_move_to_this_device_marks_each_file() {
    let (db, tmp) = release_detail_db(crate::TEST_SCHEMA).await;
    insert_files_out_of_natural_order(&db, &tmp).await;
    bae_transfers::Downloads::new(db.clone())
        .enqueue_all(vec![ReleaseQueueEntry {
            payload: DownloadRequest::MoveToThisDevice { target_dir: tmp.path().to_owned() },
            ..download(RELEASE_A)
        }])
        .await
        .unwrap();

    let mut live = crate::LibraryViews::new(db).subscribe_storage_inspector(Some(RELEASE_A.into()));
    let inspector = live.next_inline().await.unwrap().result.unwrap().unwrap();
    assert_eq!(
        inspector.files.iter().map(|file| file.download).collect::<Vec<_>>(),
        vec![
            Some(FileDownload::Waiting { kind: DownloadKind::MoveToThisDevice });
            FILES_IN_NATURAL_ORDER.len()
        ]
    );
}
