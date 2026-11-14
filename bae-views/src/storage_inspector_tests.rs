//! The Storage Manager inspector's read of one release.

use super::*;
use crate::album_detail::FilePin;
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
        op: ReleaseQueueOp { release_id: release_id.to_string(), created_at: 1, payload, state },
        release: None,
    }
}

fn export() -> OutputRequest {
    OutputRequest { target_dir: "/exports".into(), kind: OutputKind::Export }
}

fn waiting() -> bae_records::DbReleaseUpload {
    bae_records::DbReleaseUpload { waiting: 1, sending: 0, stored: 0, missing: None }
}

fn file(id: &str, pin: Option<FilePin>) -> crate::DbStorageInspectorFile {
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
            created_at: chrono::DateTime::UNIX_EPOCH,
        },
        pin,
        upload: None,
    }
}

/// Each count of a summary, by its label's key.
fn counts(summary: &[bae_transfers::TransferCount]) -> Vec<(&str, u32)> {
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
        files,
        pins,
        upload,
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
            })
            .collect::<Vec<_>>(),
        vec![("download", "selected"), ("output", "selected"), ("upload", "selected"),]
    );
}

#[test]
fn a_release_no_queue_holds_has_no_transfers() {
    let inspector = shown(
        vec![file("a", None)],
        &ReleaseQueueEntries { download: None, output: None },
        None,
        None,
    );
    assert!(inspector.transfers.is_empty());
}

/// The download of a pinning release counts its files by where their pins
/// stand, each count left out at zero, and counts nothing for a release that
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
    let read = live.next().await.expect("the query stays open").result.unwrap();
    assert_eq!(read, None, "the library holds no RELEASE_B");

    live.requests().set(Some(RELEASE_A.to_string())).expect("the query is open");
    let read = loop {
        let event = live.next().await.expect("the query stays open");
        if event.request.as_deref() == Some(RELEASE_A) {
            break event.result.unwrap().expect("the library holds RELEASE_A");
        }
    };
    let [StorageInspectorTransfer::Download { operation, .. }] = read.transfers.as_slice() else {
        panic!("only RELEASE_A's download is queued: {:?}", read.transfers);
    };
    assert_eq!(operation.op.release_id, RELEASE_A);
}
