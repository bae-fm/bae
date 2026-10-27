use super::*;
use crate::db::DbOutboxUpload;
use std::time::Duration;

/// File references are issued by coven; the tests supply its public progress values.
async fn queued_files() -> (
    coven::CovenHandle,
    tempfile::TempDir,
    Vec<coven::QueuedUpload>,
) {
    let directory = tempfile::tempdir().unwrap();
    let installation = coven::TestCoven::new();
    let tables = ["release_files", "covers", "artist_images"]
        .map(|table| {
            coven::SyncedTable::new(table, coven::RowIdentity::SharedKey).carries_files(
                coven::FileDecl::new(table, 1024 * 1024, coven::Provenance::AppProvided),
            )
        })
        .to_vec();
    let schema =
        "CREATE TABLE release_files(id TEXT PRIMARY KEY NOT NULL,size INTEGER,hash BLOB,location TEXT);\n\
        CREATE TABLE covers(id TEXT PRIMARY KEY NOT NULL,size INTEGER,hash BLOB,location TEXT);\n\
        CREATE TABLE artist_images(id TEXT PRIMARY KEY NOT NULL,size INTEGER,hash BLOB,location TEXT);";
    let handle = installation
        .builder(
            crate::config::AppDir::at(directory.path())
                .unwrap()
                .store_layout(),
        )
        .synced_tables(tables)
        .migrations(vec![coven::Migration::addition(
            1,
            "upload fixtures",
            schema,
        )])
        .create_store("Upload Test")
        .await
        .unwrap();
    handle.write_with_files(|batch| {
        for (table, id, size) in [("release_files", "first", 100), ("release_files", "second", 1000), ("covers", "first", 20), ("artist_images", "portrait", 30)] {
            batch.put_file(table, id, vec![7; size]);
        }
        Ok(())
    }, |sql| {
        sql.execute_batch("INSERT INTO release_files(id) VALUES('first'),('second'); INSERT INTO covers(id) VALUES('first'); INSERT INTO artist_images(id) VALUES('portrait');")?;
        Ok(())
    }).await.unwrap();
    let mut files = handle.uploads().await.unwrap().files;
    files.sort_by_key(|upload| {
        (
            upload.file.table().to_string(),
            format!("{:?}", upload.file.key()),
        )
    });
    (handle, directory, files)
}

fn queued(upload: coven::QueuedUpload, subject: UploadSubject) -> DbOutboxUpload {
    DbOutboxUpload { upload, subject }
}

/// One of `release`'s own files, named `name`.
fn release_file(release: &str, name: &str) -> UploadSubject {
    UploadSubject::ReleaseFile {
        release_id: release.into(),
        name: name.into(),
    }
}

/// Mark `upload` as being sent now, with these provider-confirmed bytes.
fn sending(upload: &mut coven::QueuedUpload, bytes_confirmed: u64, bytes_total: u64) {
    upload.phase = UploadState::Uploading;
    upload.progress = Some(coven::UploadProgress {
        bytes_confirmed,
        bytes_total,
    });
}

/// The queue and transfers `uploads` hold at `rates`, unpaused.
fn built(
    uploads: Vec<DbOutboxUpload>,
    rates: &HashMap<coven::FileId, u64>,
) -> (OutboxQueue, OutboxTransfers) {
    OutboxQueue::from_queue(
        DbOutboxQueue {
            paused: false,
            uploads,
        },
        rates,
    )
}

fn release_transfer(transfers: &OutboxTransfers, release: &str) -> UploadTransfer {
    transfers.keyed[&UploadTransferKey::Release(release.into())]
}

#[tokio::test]
async fn active_uploads_take_precedence_over_waiting_files() {
    let (handle, _directory, mut files) = queued_files().await;
    let mut progress = UploadProgress::default();
    let mut sum = TransferSum::default();
    assert_eq!(progress.activity(), None);
    progress.add(files[0].phase);
    sum.add(&files[0], 0);
    assert_eq!(progress.activity(), Some(UploadActivity::Queued));
    assert_eq!(sum.transfer().bar, None);
    sending(&mut files[1], 40, 116);
    progress.add(files[1].phase);
    sum.add(&files[1], 0);
    assert_eq!(progress.activity(), Some(UploadActivity::Uploading));
    assert_eq!(
        sum.transfer().bar,
        Some(UploadBar {
            bytes_done: 40,
            bytes_total: 116
        })
    );
    handle.close().await.unwrap();
}

/// A file sent in one request is being sent, with no provider bytes to
/// count.
#[tokio::test]
async fn a_file_sent_in_one_request_counts_as_sending_without_bytes() {
    let (handle, _directory, mut files) = queued_files().await;
    let mut rates = HashMap::new();
    for file in &mut files {
        rates.insert(file.file.id(), 10);
        sending(file, 10, 116);
    }
    files[0].progress = None;
    let one_request = files[0].file.id();
    let uploads = files
        .into_iter()
        .map(|file| queued(file, release_file("release", "Track.flac")))
        .collect();
    let (snapshot, transfers) = built(uploads, &rates);
    assert_eq!(snapshot.total.uploading, 4);
    assert_eq!(
        transfers.total.bar,
        Some(UploadBar {
            bytes_done: 30,
            bytes_total: 348
        })
    );
    assert_eq!(
        transfers.keyed[&UploadTransferKey::File(one_request)].bar,
        None
    );
    handle.close().await.unwrap();
}

#[tokio::test]
#[should_panic(expected = "upload bytes overflow")]
async fn aggregate_progress_cannot_wrap_its_counters() {
    let (_handle, _directory, mut files) = queued_files().await;
    let mut sum = TransferSum::default();
    sending(&mut files[0], u64::MAX, u64::MAX);
    sending(&mut files[1], 1, 1);
    sum.add(&files[0], 0);
    sum.add(&files[1], 0);
}

#[tokio::test]
#[should_panic(expected = "provider progress exceeds file size")]
async fn progress_cannot_exceed_its_denominator() {
    let (_handle, _directory, mut files) = queued_files().await;
    sending(&mut files[0], 117, 116);
    TransferSum::default().add(&files[0], 0);
}

/// A release's own files group under it, sorted naturally; its cover, which
/// uploads whether or not the release does, waits among the other files.
#[tokio::test]
async fn release_files_group_by_release_and_its_cover_waits_apart() {
    let (handle, _directory, files) = queued_files().await;
    let mut uploads = Vec::new();
    let audio = files
        .iter()
        .filter(|file| file.file.table() == "release_files")
        .cloned()
        .collect::<Vec<_>>();
    for (file, name) in audio
        .into_iter()
        .zip(["Disc 1/18 Track.flac", "disc 1/05 Track.flac"])
    {
        uploads.push(queued(file, release_file("release", name)));
    }
    uploads.push(queued(
        files
            .iter()
            .find(|file| file.file.table() == "covers")
            .unwrap()
            .clone(),
        UploadSubject::Cover {
            release_id: "release".into(),
        },
    ));
    let (snapshot, transfers) = built(uploads, &HashMap::new());
    assert_eq!(snapshot.upload_groups.len(), 1);
    assert_eq!(
        snapshot.upload_groups[0]
            .files
            .iter()
            .map(|file| file.label.clone())
            .collect::<Vec<_>>(),
        vec![
            UploadFileLabel::Filename("disc 1/05 Track.flac".into()),
            UploadFileLabel::Filename("Disc 1/18 Track.flac".into())
        ]
    );
    assert_eq!(snapshot.upload_groups[0].progress.queued, 2);
    assert_eq!(
        snapshot
            .other_files
            .iter()
            .map(|file| file.label.clone())
            .collect::<Vec<_>>(),
        vec![UploadFileLabel::Cover]
    );
    assert_eq!(
        snapshot.total.queued, 3,
        "the cover still counts in the queue"
    );
    assert_eq!(transfers.total.bar, None);
    assert_eq!(release_transfer(&transfers, "release").bar, None);
    handle.close().await.unwrap();
}

#[tokio::test]
async fn provider_bytes_and_file_bytes_stay_distinct() {
    let (handle, _directory, files) = queued_files().await;
    let mut audio = files
        .into_iter()
        .filter(|file| file.file.table() == "release_files")
        .collect::<Vec<_>>();
    sending(&mut audio[1], 250, 1016);
    let uploads = audio
        .into_iter()
        .map(|file| queued(file, release_file("release", "Track.flac")))
        .collect();
    let (snapshot, transfers) = built(uploads, &HashMap::new());
    assert_eq!(snapshot.total.queued, 1);
    assert_eq!(snapshot.total.uploading, 1);
    let bar = Some(UploadBar {
        bytes_done: 250,
        bytes_total: 1016,
    });
    assert_eq!(transfers.total.bar, bar);
    assert_eq!(release_transfer(&transfers, "release").bar, bar);
    let active = snapshot.upload_groups[0]
        .files
        .iter()
        .find(|file| file.phase == UploadState::Uploading)
        .unwrap();
    assert_eq!(active.file.plaintext_size(), 1000);
    assert_eq!(
        transfers.keyed[&UploadTransferKey::File(active.file.id())].bar,
        bar
    );
    handle.close().await.unwrap();
}

#[tokio::test]
async fn rates_belong_to_exact_files_and_pausing_removes_rates() {
    let (handle, _directory, files) = queued_files().await;
    let mut uploads = Vec::new();
    let mut rates = HashMap::new();
    for (index, mut file) in files.into_iter().enumerate() {
        rates.insert(file.file.id(), 10 * (index as u64 + 1));
        sending(&mut file, 100, 111);
        let subject = match file.file.table() {
            "covers" => UploadSubject::Cover {
                release_id: "first".into(),
            },
            "artist_images" => UploadSubject::ArtistImage,
            _ => release_file("second", "Track.flac"),
        };
        uploads.push(queued(file, subject));
    }
    for paused in [false, true] {
        let (snapshot, transfers) = OutboxQueue::from_queue(
            DbOutboxQueue {
                paused,
                uploads: uploads.clone(),
            },
            &rates,
        );
        assert_eq!(
            snapshot.other_files.len(),
            2,
            "the cover and the artist image"
        );
        assert_eq!(snapshot.upload_groups.len(), 1);
        assert_eq!(snapshot.total.uploading, 4, "artist images also count");
        assert_eq!(
            snapshot.pause_state,
            if paused {
                OutboxPauseState::Paused
            } else {
                OutboxPauseState::Running
            }
        );
        assert_eq!(transfers.total.throughput_bps, if paused { 0 } else { 100 });
        let files_summed = transfers
            .keyed
            .iter()
            .filter(|(key, _)| matches!(key, UploadTransferKey::File(_)))
            .map(|(_, transfer)| transfer.throughput_bps)
            .sum::<u64>();
        assert_eq!(files_summed, transfers.total.throughput_bps);
        let release_files = snapshot.upload_groups[0]
            .files
            .iter()
            .map(|file| transfers.keyed[&UploadTransferKey::File(file.file.id())].throughput_bps)
            .sum::<u64>();
        assert_eq!(
            release_transfer(&transfers, "second").throughput_bps,
            release_files
        );
    }
    handle.close().await.unwrap();
}

#[tokio::test]
async fn completed_files_leave_no_empty_release_groups_or_stale_rates() {
    let (handle, _directory, files) = queued_files().await;
    let rates = files.iter().map(|file| (file.file.id(), 100)).collect();
    let (snapshot, transfers) = OutboxQueue::from_queue(DbOutboxQueue::default(), &rates);
    assert!(snapshot.upload_groups.is_empty());
    assert!(snapshot.per_release_progress().is_empty());
    assert!(transfers.keyed.is_empty());
    assert_eq!(transfers.total.throughput_bps, 0);
    assert_eq!(snapshot.total.activity(), None);
    handle.close().await.unwrap();
}

/// A part the provider confirms moves the transfers and leaves the queue as
/// it was, which is what lets the queue be sent only when it changes.
#[tokio::test]
async fn a_confirmed_part_moves_the_transfers_and_not_the_queue() {
    let (handle, _directory, files) = queued_files().await;
    let mut audio = files
        .into_iter()
        .filter(|file| file.file.table() == "release_files")
        .collect::<Vec<_>>();
    let queue = |audio: &[coven::QueuedUpload]| {
        built(
            audio
                .iter()
                .cloned()
                .map(|file| queued(file, release_file("release", "Track.flac")))
                .collect(),
            &HashMap::new(),
        )
    };
    sending(&mut audio[0], 100, 1016);
    let (before, before_transfers) = queue(&audio);
    sending(&mut audio[0], 200, 1016);
    let (after, after_transfers) = queue(&audio);
    assert_eq!(before, after);
    assert_ne!(before_transfers, after_transfers);

    audio[1].phase = UploadState::Uploading;
    let (started, _) = queue(&audio);
    assert_ne!(after, started, "a file starting to send changes the queue");
    handle.close().await.unwrap();
}

#[tokio::test]
async fn display_names_join_by_table_and_row_while_rates_use_object_identity() {
    let (handle, _directory, initial_files) = queued_files().await;
    let replaced = initial_files
        .iter()
        .find(|upload| {
            upload.file.table() == "release_files"
                && upload.file.key() == &coven::RowKey::from("first")
        })
        .unwrap()
        .file
        .id();
    handle
        .write_with_files(
            |batch| {
                batch.put_file("release_files", "first", vec![8; 101]);
                Ok(())
            },
            |sql| {
                sql.execute("UPDATE release_files SET size = 101 WHERE id = 'first'", [])?;
                Ok(())
            },
        )
        .await
        .unwrap();
    let files = handle.uploads().await.unwrap().files;
    let context = crate::db::OutboxDisplayContext {
        file_names: HashMap::from([
            ("first".into(), ("release".into(), "First.flac".into())),
            ("second".into(), ("release".into(), "Renamed.flac".into())),
        ]),
    };
    let queue = crate::db::Database::outbox_queue_from_context(
        coven::UploadQueue {
            paused: false,
            files,
        },
        context,
    )
    .unwrap();
    let (snapshot, _) = OutboxQueue::from_queue(queue, &HashMap::new());
    let release = snapshot
        .upload_groups
        .iter()
        .find(|group| group.release_id == "release")
        .unwrap();
    // Attaching again replaced the row's pending attachment: only the new
    // file is queued for it.
    assert_eq!(release.files.len(), 2);
    assert!(release
        .files
        .iter()
        .any(|file| file.label == UploadFileLabel::Filename("Renamed.flac".into())));
    assert!(
        !snapshot
            .upload_groups
            .iter()
            .any(|group| group.release_id == "first"),
        "a cover alone is no release uploading"
    );
    let cover = snapshot
        .other_files
        .iter()
        .find(|file| file.label == UploadFileLabel::Cover)
        .unwrap();
    assert_ne!(cover.file.id(), release.files[0].file.id());
    #[cfg(desktop)]
    {
        use crate::library::storage_inspector::{
            StorageInspector, StorageInspectorFile, StorageInspectorTransfer,
        };
        let file = |id: &str, name: &str| crate::album_detail::FileDetail {
            id: id.into(),
            original_filename: name.into(),
            file_size: 101,
            is_image: false,
            content_type: "audio/flac".into(),
            source_audio: None,
        };
        let group = snapshot
            .upload_groups
            .into_iter()
            .find(|group| group.release_id == "release")
            .unwrap();
        let inspector = StorageInspector::of(
            vec![file("first", "First.flac"), file("second", "Renamed.flac")],
            vec![StorageInspectorTransfer::Upload {
                group,
                paused: false,
            }],
        );
        let StorageInspectorFile::ReleaseFile {
            upload: Some(upload),
            ..
        } = &inspector.files[0]
        else {
            panic!("the release file retains its place with its upload")
        };
        assert_ne!(
            upload.file.id(),
            replaced,
            "the row's upload is the file attached last"
        );
    }
    handle.close().await.unwrap();
}

#[tokio::test]
async fn pending_reasons_belong_to_the_exact_uploading_device_and_file() {
    let (handle, _directory, files) = queued_files().await;
    let upload = files
        .into_iter()
        .find(|file| file.file.table() == "release_files")
        .unwrap();
    let record = coven::PendingRecord::new(
        coven::PendingSubject::File {
            device: upload.file.uploader(),
            file: upload.file.id(),
        },
        coven::PendingReason::Paused,
    );
    let wrong_device = coven::PendingRecord::new(
        coven::PendingSubject::File {
            device: coven::DeviceId(upload.file.uploader().0 ^ 1),
            file: upload.file.id(),
        },
        coven::PendingReason::NoStorage,
    );
    let (snapshot, _) = OutboxQueue::from_queue(
        DbOutboxQueue {
            paused: true,
            uploads: vec![queued(upload, release_file("release", "Track.flac"))],
        },
        &HashMap::new(),
    );
    let snapshot = snapshot.with_pending(&[wrong_device, record.clone()]);
    let pending = snapshot.upload_groups[0].files[0]
        .pending
        .as_ref()
        .expect("the file's own record reaches it");
    assert_eq!(pending.reason, record.reason);
    assert_eq!(pending.action, crate::library::PendingAction::ResumeUploads);
    handle.close().await.unwrap();
}

/// The rate tracker follows the queue it observes: a file that left the
/// queue, or stopped uploading, has no rate, and pausing the queue drops
/// every measurement so resuming starts fresh.
#[tokio::test]
async fn observed_rates_follow_the_queue_and_pausing_starts_fresh() {
    let (handle, _directory, files) = queued_files().await;
    let mut audio = files
        .into_iter()
        .filter(|file| file.file.table() == "release_files")
        .collect::<Vec<_>>();
    let queue = |files: &[coven::QueuedUpload], paused| coven::UploadQueue {
        paused,
        files: files.to_vec(),
    };
    let (first, second) = (audio[0].file.id(), audio[1].file.id());
    let mut tracker = crate::library::upload_throughput::UploadThroughput::default();
    sending(&mut audio[0], 0, 1016);
    sending(&mut audio[1], 0, 1016);
    tracker.observe(&queue(&audio, false), Duration::ZERO);
    sending(&mut audio[0], 100, 1016);
    sending(&mut audio[1], 200, 1016);
    tracker.observe(&queue(&audio, false), Duration::from_secs(1));
    let rates = tracker.rates(Duration::from_secs(1));
    assert_eq!((rates[&first], rates[&second]), (100, 200));

    audio[1].phase = UploadState::Waiting;
    tracker.observe(&queue(&audio[..1], false), Duration::from_secs(2));
    let rates = tracker.rates(Duration::from_secs(2));
    assert_eq!(rates.len(), 1, "the file that left the queue has no rate");
    assert!(rates.contains_key(&first));

    tracker.observe(&queue(&audio[..1], true), Duration::from_secs(3));
    assert!(tracker.rates(Duration::from_secs(3)).is_empty());
    sending(&mut audio[0], 400, 1016);
    tracker.observe(&queue(&audio[..1], false), Duration::from_secs(4));
    assert_eq!(
        tracker.rates(Duration::from_secs(4))[&first],
        0,
        "bytes confirmed before the pause are not counted after it"
    );
    handle.close().await.unwrap();
}
