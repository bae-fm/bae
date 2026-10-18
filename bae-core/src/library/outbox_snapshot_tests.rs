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
                coven::FileDecl::new(
                    table,
                    1024 * 1024,
                    coven::Provenance::AppProvided,
                    coven::CacheFill::CacheLazy,
                ),
            )
        })
        .to_vec();
    let schema =
        "CREATE TABLE release_files(id TEXT PRIMARY KEY NOT NULL,size INTEGER,hash BLOB,location TEXT);\n\
        CREATE TABLE covers(id TEXT PRIMARY KEY NOT NULL,size INTEGER,hash BLOB,location TEXT);\n\
        CREATE TABLE artist_images(id TEXT PRIMARY KEY NOT NULL,size INTEGER,hash BLOB,location TEXT);";
    let handle = installation
        .builder(coven::StoreLayout::new(directory.path().into()))
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

fn labelled(
    upload: coven::QueuedUpload,
    release: Option<&str>,
    label: UploadFileLabel,
) -> DbOutboxUpload {
    DbOutboxUpload {
        upload,
        release_id: release.map(str::to_string),
        label,
    }
}

#[test]
fn active_uploads_take_precedence_over_waiting_files() {
    let mut progress = UploadProgress::default();
    assert_eq!(progress.activity(), None);
    progress.add(UploadState::Waiting);
    assert_eq!(progress.activity(), Some(UploadActivity::Queued));
    assert_eq!(progress.bar(), None);
    progress.add(UploadState::Uploading {
        bytes_confirmed: 40,
        bytes_total: 116,
    });
    assert_eq!(progress.activity(), Some(UploadActivity::Uploading));
    assert_eq!(
        progress.bar(),
        Some(UploadBar {
            bytes_done: 40,
            bytes_total: 116
        })
    );
}

#[test]
#[should_panic(expected = "upload bytes overflow")]
fn aggregate_progress_cannot_wrap_its_counters() {
    let mut progress = UploadProgress::default();
    progress.add(UploadState::Uploading {
        bytes_confirmed: u64::MAX,
        bytes_total: u64::MAX,
    });
    progress.add(UploadState::Uploading {
        bytes_confirmed: 1,
        bytes_total: 1,
    });
}

#[test]
#[should_panic(expected = "provider progress exceeds file size")]
fn progress_cannot_exceed_its_denominator() {
    UploadProgress::default().add(UploadState::Uploading {
        bytes_confirmed: 117,
        bytes_total: 116,
    });
}

#[tokio::test]
async fn files_group_by_release_and_sort_naturally_with_artwork_first() {
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
        uploads.push(labelled(
            file,
            Some("release"),
            UploadFileLabel::Filename(name.into()),
        ));
    }
    uploads.push(labelled(
        files
            .iter()
            .find(|file| file.file.table() == "covers")
            .unwrap()
            .clone(),
        Some("release"),
        UploadFileLabel::Cover,
    ));
    let snapshot = OutboxSnapshot::from_queue(
        DbOutboxQueue {
            paused: false,
            uploads,
        },
        &HashMap::new(),
    );
    assert_eq!(snapshot.upload_groups.len(), 1);
    assert_eq!(
        snapshot.upload_groups[0]
            .files
            .iter()
            .map(|file| file.label.clone())
            .collect::<Vec<_>>(),
        vec![
            UploadFileLabel::Cover,
            UploadFileLabel::Filename("disc 1/05 Track.flac".into()),
            UploadFileLabel::Filename("Disc 1/18 Track.flac".into())
        ]
    );
    assert_eq!(snapshot.total.queued, 3);
    assert_eq!(snapshot.total.upload_bytes_total, 0);
    assert_eq!(snapshot.total.bar(), None);
    handle.close().await.unwrap();
}

#[tokio::test]
async fn provider_bytes_and_file_bytes_stay_distinct() {
    let (handle, _directory, files) = queued_files().await;
    let mut audio = files
        .into_iter()
        .filter(|file| file.file.table() == "release_files")
        .collect::<Vec<_>>();
    audio[1].phase = UploadState::Uploading {
        bytes_confirmed: 250,
        bytes_total: 1016,
    };
    let uploads = audio
        .into_iter()
        .map(|file| {
            labelled(
                file,
                Some("release"),
                UploadFileLabel::Filename("Track.flac".into()),
            )
        })
        .collect();
    let snapshot = OutboxSnapshot::from_queue(
        DbOutboxQueue {
            paused: false,
            uploads,
        },
        &HashMap::new(),
    );
    assert_eq!(snapshot.total.queued, 1);
    assert_eq!(snapshot.total.uploading, 1);
    assert_eq!(snapshot.total.upload_bytes_done, 250);
    assert_eq!(snapshot.total.upload_bytes_total, 1016);
    let active = snapshot.upload_groups[0]
        .files
        .iter()
        .find(|file| matches!(file.upload.phase, UploadState::Uploading { .. }))
        .unwrap();
    assert_eq!(active.upload.file.plaintext_size(), 1000);
    assert_eq!(
        active.bar(),
        Some(UploadBar {
            bytes_done: 250,
            bytes_total: 1016
        })
    );
    assert_eq!(snapshot.eta_seconds, None);
    handle.close().await.unwrap();
}

#[tokio::test]
async fn rates_belong_to_exact_files_and_pausing_removes_rates_and_eta() {
    let (handle, _directory, files) = queued_files().await;
    let mut uploads = Vec::new();
    let mut rates = HashMap::new();
    for (index, mut file) in files.into_iter().enumerate() {
        rates.insert(file.file.id(), 10 * (index as u64 + 1));
        file.phase = UploadState::Uploading {
            bytes_confirmed: 100,
            bytes_total: 111,
        };
        let (release, label) = match file.file.table() {
            "covers" => (Some("first"), UploadFileLabel::Cover),
            "artist_images" => (None, UploadFileLabel::ArtistImage),
            _ => (
                Some("second"),
                UploadFileLabel::Filename("Track.flac".into()),
            ),
        };
        uploads.push(labelled(file, release, label));
    }
    for paused in [false, true] {
        let snapshot = OutboxSnapshot::from_queue(
            DbOutboxQueue {
                paused,
                uploads: uploads.clone(),
            },
            &rates,
        );
        assert_eq!(snapshot.other_files.len(), 1);
        assert_eq!(snapshot.upload_groups.len(), 2);
        assert_eq!(snapshot.total.uploading, 4, "artist images also count");
        assert_eq!(
            snapshot.pause_state,
            if paused {
                OutboxPauseState::Paused
            } else {
                OutboxPauseState::Running
            }
        );
        assert_eq!(snapshot.throughput_bps, if paused { 0 } else { 100 });
        assert_eq!(snapshot.eta_seconds, if paused { None } else { Some(1) });
        let summed = snapshot
            .upload_groups
            .iter()
            .map(|group| group.throughput_bps)
            .sum::<u64>()
            + snapshot
                .other_files
                .iter()
                .map(|file| file.throughput_bps)
                .sum::<u64>();
        assert_eq!(summed, snapshot.throughput_bps);
    }
    handle.close().await.unwrap();
}

#[tokio::test]
async fn completed_files_leave_no_empty_release_groups_or_stale_rates() {
    let (handle, _directory, files) = queued_files().await;
    let rates = files.iter().map(|file| (file.file.id(), 100)).collect();
    let snapshot = OutboxSnapshot::from_queue(DbOutboxQueue::default(), &rates);
    assert!(snapshot.upload_groups.is_empty());
    assert!(snapshot.per_release_progress().is_empty());
    assert_eq!(snapshot.throughput_bps, 0);
    assert_eq!(snapshot.eta_seconds, None);
    assert_eq!(snapshot.total.activity(), None);
    handle.close().await.unwrap();
}

#[tokio::test]
async fn display_names_join_by_table_and_row_while_rates_use_object_identity() {
    let (handle, _directory, _initial_files) = queued_files().await;
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
    let snapshot = OutboxSnapshot::from_queue(queue, &HashMap::new());
    let release = snapshot
        .upload_groups
        .iter()
        .find(|group| group.release_id == "release")
        .unwrap();
    assert_eq!(release.files.len(), 3);
    assert_eq!(
        release.files[2].label,
        UploadFileLabel::Filename("Renamed.flac".into())
    );
    let cover = snapshot
        .upload_groups
        .iter()
        .find(|group| group.release_id == "first")
        .unwrap();
    assert_eq!(cover.files[0].label, UploadFileLabel::Cover);
    assert_ne!(
        cover.files[0].upload.file.id(),
        release.files[0].upload.file.id()
    );
    let rows = crate::library::storage_inspector::storage_inspector_release_files(
        "release",
        vec![
            ("first".into(), "First.flac"),
            ("second".into(), "Renamed.flac"),
        ],
        snapshot
            .upload_groups
            .into_iter()
            .map(|group| {
                (
                    group.release_id,
                    group
                        .files
                        .into_iter()
                        .map(|upload| (upload.release_file_key().cloned(), upload))
                        .collect(),
                )
            })
            .collect(),
    );
    let crate::library::storage_inspector::StorageInspectorFile::ReleaseFile { uploads, .. } =
        &rows[0]
    else {
        panic!("the release file retains its place")
    };
    assert_eq!(
        uploads.len(),
        2,
        "both committed uploads join their release-file row"
    );
    assert_ne!(uploads[0].upload.file.id(), uploads[1].upload.file.id());
    handle.close().await.unwrap();
}

#[tokio::test]
async fn pending_reasons_belong_to_the_exact_uploading_device_and_file() {
    let (handle, _directory, files) = queued_files().await;
    let upload = files
        .into_iter()
        .find(|file| file.file.table() == "release_files")
        .unwrap();
    let record = coven::PendingRecord {
        subject: coven::PendingSubject::File {
            device: upload.file.uploader(),
            file: upload.file.id(),
        },
        reason: coven::PendingReason::Paused,
    };
    let wrong_device = coven::PendingRecord {
        subject: coven::PendingSubject::File {
            device: coven::DeviceId(upload.file.uploader().0 ^ 1),
            file: upload.file.id(),
        },
        reason: coven::PendingReason::NoStorage,
    };
    let snapshot = OutboxSnapshot::from_queue(
        DbOutboxQueue {
            paused: true,
            uploads: vec![labelled(
                upload,
                Some("release"),
                UploadFileLabel::Filename("Track.flac".into()),
            )],
        },
        &HashMap::new(),
    )
    .with_pending(vec![wrong_device, record.clone()]);
    let pending = snapshot.upload_groups[0].files[0]
        .pending
        .as_ref()
        .expect("the file's own record reaches it");
    assert_eq!(pending.reason, record.reason);
    assert_eq!(pending.action, crate::library::PendingAction::ResumeUploads);
    handle.close().await.unwrap();
}

/// Uploading release files with these confirmed and total provider bytes and
/// these rates, as one release's queue.
async fn uploading(
    progress: &[(u64, u64, u64)],
) -> (coven::CovenHandle, tempfile::TempDir, OutboxSnapshot) {
    let (handle, directory, files) = queued_files().await;
    let mut rates = HashMap::new();
    let uploads = files
        .into_iter()
        .filter(|file| file.file.table() == "release_files")
        .zip(progress)
        .map(|(mut file, &(confirmed, total, rate))| {
            file.phase = UploadState::Uploading {
                bytes_confirmed: confirmed,
                bytes_total: total,
            };
            rates.insert(file.file.id(), rate);
            labelled(
                file,
                Some("release"),
                UploadFileLabel::Filename("Track.flac".into()),
            )
        })
        .collect();
    let snapshot = OutboxSnapshot::from_queue(
        DbOutboxQueue {
            paused: false,
            uploads,
        },
        &rates,
    );
    (handle, directory, snapshot)
}

#[tokio::test]
async fn eta_never_reports_zero_while_provider_bytes_remain() {
    let (handle, _directory, snapshot) = uploading(&[(115, 116, 10)]).await;
    assert_eq!(snapshot.eta_seconds, Some(1));
    handle.close().await.unwrap();
    let (handle, _directory, snapshot) = uploading(&[(105, 116, 10)]).await;
    assert_eq!(snapshot.eta_seconds, Some(2));
    handle.close().await.unwrap();
}

#[tokio::test]
async fn queue_eta_counts_every_uploading_files_remaining_bytes() {
    let (handle, _directory, snapshot) = uploading(&[(16, 116, 20), (216, 1016, 30)]).await;
    assert_eq!(snapshot.total.upload_bytes_done, 232);
    assert_eq!(snapshot.total.upload_bytes_total, 1132);
    // 900 bytes left at 50 B/s.
    assert_eq!(snapshot.eta_seconds, Some(18));
    handle.close().await.unwrap();
}

#[tokio::test]
async fn queue_eta_is_hidden_while_a_file_waits_for_its_provider_size() {
    let (handle, _directory, files) = queued_files().await;
    let mut rates = HashMap::new();
    let uploads = files
        .into_iter()
        .filter(|file| file.file.table() == "release_files")
        .enumerate()
        .map(|(index, mut file)| {
            if index == 0 {
                file.phase = UploadState::Uploading {
                    bytes_confirmed: 16,
                    bytes_total: 116,
                };
                rates.insert(file.file.id(), 20);
            }
            labelled(
                file,
                Some("release"),
                UploadFileLabel::Filename("Track.flac".into()),
            )
        })
        .collect();
    let snapshot = OutboxSnapshot::from_queue(
        DbOutboxQueue {
            paused: false,
            uploads,
        },
        &rates,
    );
    assert_eq!(snapshot.throughput_bps, 20);
    assert_eq!(
        snapshot.eta_seconds, None,
        "a waiting file's provider bytes are not known yet"
    );
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
    let at = |confirmed| UploadState::Uploading {
        bytes_confirmed: confirmed,
        bytes_total: 1016,
    };
    let queue = |files: &[coven::QueuedUpload], paused| coven::UploadQueue {
        paused,
        files: files.to_vec(),
    };
    let (first, second) = (audio[0].file.id(), audio[1].file.id());
    let mut tracker = crate::library::upload_throughput::UploadThroughput::default();
    audio[0].phase = at(0);
    audio[1].phase = at(0);
    tracker.observe(&queue(&audio, false), Duration::ZERO);
    audio[0].phase = at(100);
    audio[1].phase = at(200);
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
    audio[0].phase = at(400);
    tracker.observe(&queue(&audio[..1], false), Duration::from_secs(4));
    assert_eq!(
        tracker.rates(Duration::from_secs(4))[&first],
        0,
        "bytes confirmed before the pause are not counted after it"
    );
    handle.close().await.unwrap();
}
