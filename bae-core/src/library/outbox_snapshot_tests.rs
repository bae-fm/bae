use super::*;
use crate::db::{DbOutboxDelete, DbOutboxUpload};

const RELEASE: &str = "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e";
const SMALL_FILE: &str = "00415c7f-b363-4ed9-8aad-422b93e974e9";
const LARGE_FILE: &str = "357d9eb4-a021-4555-8713-0bc652d83c65";
const OTHER_RELEASE: &str = "e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b";
const OTHER_FILE: &str = "36ebe9b3-749f-4638-82b2-57cba256ff68";

fn row_blob(
    table: &str,
    row_id: &str,
    namespace: &str,
    blob_id: &str,
    plaintext_size: u64,
) -> coven::RowBlobRef {
    coven::RowBlobRef::new(
        table.to_string(),
        row_id.to_string(),
        "0000000001000-0000-device-a".to_string(),
        "blob_id".to_string(),
        coven::BlobRef {
            namespace: namespace.to_string(),
            id: blob_id.to_string(),
            scope: coven::BlobScope::Master,
            cloud_path: None,
            provenance: coven::Provenance::HostProvided,
            fill: coven::CacheFill::CacheEager,
        },
        plaintext_size,
        coven::ObjectHash::digest(blob_id.as_bytes()),
        coven::RowBlobAuthority::Local,
        None,
    )
    .expect("valid queued test blob")
}

fn upload_id(namespace: &str, blob_id: &str) -> String {
    format!("{namespace}:{blob_id}")
}

#[test]
fn activity_ranks_the_real_upload_journey() {
    let mut progress = UploadProgress::default();
    assert_eq!(progress.activity(), None);
    for (activity, set) in [
        (UploadActivity::Queued, 0),
        (UploadActivity::Prepared, 1),
        (UploadActivity::Retrying, 2),
        (UploadActivity::Preparing, 3),
        (UploadActivity::Uploading, 4),
        (UploadActivity::Publishing, 5),
        (UploadActivity::Cancelling, 6),
    ] {
        match set {
            0 => progress.queued = 1,
            1 => progress.prepared = 1,
            2 => progress.retrying = 1,
            3 => progress.preparing = 1,
            4 => progress.uploading = 1,
            5 => progress.publishing = 1,
            6 => progress.cancelling = 1,
            _ => unreachable!(),
        }
        assert_eq!(progress.activity(), Some(activity));
    }
}

#[test]
#[should_panic(expected = "provider byte progress overflow")]
fn aggregate_progress_cannot_wrap_its_byte_counters() {
    let mut total = UploadProgress {
        upload_bytes_done: u64::MAX,
        ..Default::default()
    };
    let next = UploadProgress {
        upload_bytes_done: 1,
        ..Default::default()
    };

    total.add_progress(&next);
}

/// One release with two queued uploads (100 and 1000 bytes), as coven's
/// queue plus bae's context report them.
fn two_queued_uploads() -> DbOutboxQueue {
    DbOutboxQueue {
        uploads: vec![
            queued_upload(SMALL_FILE, "01 Track Title.flac", 100),
            queued_upload(LARGE_FILE, "02 Track Title.flac", 1000),
        ],
        deletes: Vec::new(),
        make_remotes: Vec::new(),
    }
}

fn queued_upload(file_id: &str, file_name: &str, file_size: u64) -> DbOutboxUpload {
    DbOutboxUpload {
        release_id: RELEASE.to_string(),
        blob: row_blob(
            crate::sync::RELEASE_FILES_NAMESPACE,
            file_id,
            crate::sync::RELEASE_FILES_NAMESPACE,
            file_id,
            file_size,
        ),
        phase: coven::QueuedUploadPhase::Pending,
        provider_bytes_total: None,
        attempt_count: 0,
        last_failure: None,
        created_at: 1_700_000_000_000,
        label: UploadFileLabel::Filename(file_name.to_string()),
        album_title: "Album Title".to_string(),
    }
}

fn build(
    queue: DbOutboxQueue,
    transient: &HashMap<UploadBlobKey, TransientUploadState>,
) -> OutboxSnapshot {
    build_outbox_snapshot(queue, transient, &UploadThroughput::new(), false)
}

/// The durable queue query and the in-process callback stream are independent
/// feeds: a provider callback can arrive before the requery delivers the
/// Prepared row, and a failed attempt's rollback can land while its last
/// callback is still in flight. A provider-transfer transient paired with a
/// durable Pending row refines nothing — the row renders from its durable
/// state alone.
#[test]
fn a_provider_transient_on_a_pending_row_is_a_stale_straggler() {
    for straggler in [
        TransientUploadState::UploadStarted,
        TransientUploadState::Uploading {
            bytes_done: 250,
            bytes_total: 1016,
        },
    ] {
        let transient = HashMap::from([(
            UploadBlobKey::new(crate::sync::RELEASE_FILES_NAMESPACE, LARGE_FILE),
            straggler,
        )]);

        // No recorded failure: the durable row still reads Queued.
        let snapshot = build(two_queued_uploads(), &transient);
        assert_eq!(
            snapshot.upload_groups[0].files[1].state,
            UploadState::Queued
        );
        assert_eq!(snapshot.total.upload_bytes_done, 0);

        // A recorded failure: the durable row reads its retry state.
        let mut queue = two_queued_uploads();
        queue.uploads[1].attempt_count = 1;
        queue.uploads[1].last_failure = Some(coven::OutboxFailure::other("provider write failed"));
        let snapshot = build(queue, &transient);
        assert_eq!(
            snapshot.upload_groups[0].files[1].state,
            UploadState::RetryingPreparation {
                last_error: "provider write failed".to_string(),
            }
        );
    }
}

/// The queue enqueues in whatever order the walk produced. A release's files
/// are listed by name instead, naturally and ignoring case, with the release's
/// own artwork ahead of them — core cannot sort the artwork's name, which each
/// platform localizes.
#[test]
fn a_releases_files_are_listed_by_name_with_the_cover_first() {
    let names = [
        "disc 2/09 Track.flac",
        "Disc 1/18 Track.flac",
        "Disc 1/05 Track.flac",
    ];
    let mut uploads: Vec<DbOutboxUpload> = names
        .iter()
        .enumerate()
        .map(|(i, name)| queued_upload(&format!("file-{i}"), name, 100))
        .collect();
    uploads.push(DbOutboxUpload {
        blob: row_blob(
            crate::sync::COVERS_NAMESPACE,
            "cover-row",
            crate::sync::COVERS_NAMESPACE,
            "cover-blob",
            20,
        ),
        label: UploadFileLabel::Cover,
        ..queued_upload("cover-row", "unused", 0)
    });
    let snapshot = build(
        DbOutboxQueue {
            uploads,
            deletes: Vec::new(),
            make_remotes: Vec::new(),
        },
        &HashMap::new(),
    );

    assert_eq!(
        snapshot.upload_groups[0]
            .files
            .iter()
            .map(|file| file.label.clone())
            .collect::<Vec<_>>(),
        vec![
            UploadFileLabel::Cover,
            UploadFileLabel::Filename("Disc 1/05 Track.flac".to_string()),
            UploadFileLabel::Filename("Disc 1/18 Track.flac".to_string()),
            UploadFileLabel::Filename("disc 2/09 Track.flac".to_string()),
        ]
    );
}

#[test]
fn upload_groups_group_a_releases_files_with_aggregate_progress() {
    let snapshot = build(two_queued_uploads(), &HashMap::new());

    // The release's two files collapse to one group carrying both.
    assert_eq!(snapshot.upload_groups.len(), 1);
    let group = &snapshot.upload_groups[0];
    assert_eq!(group.release_id, RELEASE);
    assert_eq!(group.display_title, "Album Title");
    assert_eq!(group.files.len(), 2);
    assert_eq!(
        group.files[0].label,
        UploadFileLabel::Filename("01 Track Title.flac".to_string())
    );
    assert_eq!(group.files[0].state, UploadState::Queued);
    // Aggregate progress: both queued, summed bytes (100 + 1000).
    assert_eq!(group.progress.queued, 2);
    assert_eq!(group.progress.uploading, 0);
    assert_eq!(group.progress.preparation_bytes_total, 1100);
    assert!(group.progress.can_cancel());
}

#[test]
fn source_unavailable_failure_projects_its_path_for_the_release() {
    let mut queue = two_queued_uploads();
    let source_path = std::path::PathBuf::from("/Volumes/Library/Album/01 Track.flac");
    queue.uploads[0].attempt_count = 1;
    queue.uploads[0].last_failure = Some(coven::OutboxFailure::source_unavailable(
        source_path.clone(),
        "source file is unavailable",
    ));

    let snapshot = build(queue, &HashMap::new());

    assert_eq!(
        snapshot.upload_groups[0].progress.issue,
        Some(UploadIssue::SourceUnavailable {
            paths: vec![source_path]
        })
    );
}

#[test]
fn live_bytes_ride_the_active_file_and_the_totals() {
    // The large file is uploading right now (250 of 1000 bytes done); the
    // small file is still queued.
    let mut queue = two_queued_uploads();
    queue.uploads[1].phase = coven::QueuedUploadPhase::Prepared;
    queue.uploads[1].provider_bytes_total = Some(1016);
    let transient = HashMap::from([(
        UploadBlobKey::new(crate::sync::RELEASE_FILES_NAMESPACE, LARGE_FILE),
        TransientUploadState::Uploading {
            bytes_done: 250,
            bytes_total: 1016,
        },
    )]);
    let snapshot = build(queue, &transient);

    assert_eq!(snapshot.total.preparation_bytes_total, 1100);
    assert_eq!(snapshot.total.upload_bytes_total, 1016);
    // bytes_done is the in-flight file's live progress.
    assert_eq!(snapshot.total.upload_bytes_done, 250);
    assert_eq!(snapshot.total.uploading, 1);
    assert_eq!(snapshot.total.queued, 1);
    assert!(
        !snapshot.total.upload_bytes_total_complete,
        "the queued file's provider size is not known yet"
    );
    let group = &snapshot.upload_groups[0];
    let active = group
        .files
        .iter()
        .find(|f| f.file_id == upload_id(crate::sync::RELEASE_FILES_NAMESPACE, LARGE_FILE))
        .expect("active file listed");
    assert_eq!(
        active.state,
        UploadState::Uploading {
            bytes_done: 250,
            bytes_total: 1016
        }
    );
}

#[test]
fn pause_state_is_absolute_while_transfer_work_is_active() {
    let mut queue = two_queued_uploads();
    queue.uploads[1].phase = coven::QueuedUploadPhase::Prepared;
    queue.uploads[1].provider_bytes_total = Some(1016);
    let transient = HashMap::from([(
        UploadBlobKey::new(crate::sync::RELEASE_FILES_NAMESPACE, LARGE_FILE),
        TransientUploadState::Uploading {
            bytes_done: 250,
            bytes_total: 1016,
        },
    )]);

    let paused_during_upload =
        build_outbox_snapshot(queue.clone(), &transient, &UploadThroughput::new(), true);
    assert_eq!(paused_during_upload.pause_state, OutboxPauseState::Paused);

    let paused = build_outbox_snapshot(queue, &HashMap::new(), &UploadThroughput::new(), true);
    assert_eq!(paused.pause_state, OutboxPauseState::Paused);

    let preparing = HashMap::from([(
        UploadBlobKey::new(crate::sync::RELEASE_FILES_NAMESPACE, SMALL_FILE),
        TransientUploadState::Preparing {
            bytes_done: 20,
            bytes_total: 100,
        },
    )]);
    let paused_during_preparation = build_outbox_snapshot(
        two_queued_uploads(),
        &preparing,
        &UploadThroughput::new(),
        true,
    );
    assert_eq!(
        paused_during_preparation.pause_state,
        OutboxPauseState::Paused
    );

    let running = build(two_queued_uploads(), &HashMap::new());
    assert_eq!(running.pause_state, OutboxPauseState::Running);
}

/// Row ids do not identify immutable bytes. A cover whose row id equals an
/// audio file's id must not make that audio blob active or completed.
#[test]
fn upload_state_uses_the_blob_bearing_table_and_row() {
    let shared_row_id = SMALL_FILE;
    let cover_blob_id = "8ff02583-dd77-47e0-9db5-8be5a7295729";
    let mut queue = DbOutboxQueue {
        uploads: vec![
            queued_upload(shared_row_id, "01 Track Title.flac", 100),
            DbOutboxUpload {
                blob: row_blob(
                    crate::sync::COVERS_NAMESPACE,
                    shared_row_id,
                    crate::sync::COVERS_NAMESPACE,
                    cover_blob_id,
                    20,
                ),
                label: UploadFileLabel::Cover,
                ..queued_upload(shared_row_id, "unused", 0)
            },
        ],
        deletes: Vec::new(),
        make_remotes: Vec::new(),
    };
    queue.uploads[1].release_id = RELEASE.to_string();
    let transient = HashMap::from([(
        UploadBlobKey::new(crate::sync::COVERS_NAMESPACE, cover_blob_id),
        TransientUploadState::Preparing {
            bytes_done: 10,
            bytes_total: 20,
        },
    )]);

    let snapshot = build(queue, &transient);

    let audio = snapshot.upload_groups[0]
        .files
        .iter()
        .find(|upload| {
            upload.file_id == upload_id(crate::sync::RELEASE_FILES_NAMESPACE, shared_row_id)
        })
        .expect("audio upload");
    assert_eq!(audio.state, UploadState::Queued);
    let cover = snapshot.upload_groups[0]
        .files
        .iter()
        .find(|upload| upload.file_id == upload_id(crate::sync::COVERS_NAMESPACE, cover_blob_id))
        .expect("cover upload");
    assert_eq!(
        cover.state,
        UploadState::Preparing {
            bytes_done: 10,
            bytes_total: 20
        }
    );
}

/// A queue entry coven has recorded a failed attempt on derives as retrying,
/// so the release badge reads "Retrying" rather than "Queued".
#[test]
fn a_recorded_failure_derives_retrying_with_its_error() {
    let mut queue = two_queued_uploads();
    queue.uploads[1].attempt_count = 1;
    queue.uploads[1].last_failure = Some(coven::OutboxFailure::other("boom"));

    let snapshot = build(queue, &HashMap::new());

    assert_eq!(snapshot.total.retrying, 1);
    assert_eq!(snapshot.total.queued, 1);
    assert_eq!(
        snapshot.total.activity(),
        Some(UploadActivity::Retrying),
        "a failure awaiting retry outranks the file still only queued"
    );
    assert!(snapshot
        .summary_parts()
        .iter()
        .any(|part| part.key == "core.outbox.retrying" && part.count == 1));
    let failed = snapshot.upload_groups[0]
        .files
        .iter()
        .find(|f| f.file_id == upload_id(crate::sync::RELEASE_FILES_NAMESPACE, LARGE_FILE))
        .expect("failed file listed");
    assert_eq!(
        failed.state,
        UploadState::RetryingPreparation {
            last_error: "boom".to_string()
        }
    );
}

/// Pending tombstones carry into the snapshot and into the summary line even
/// when nothing is uploading — the queue pane still has work to show.
#[test]
fn pending_deletes_survive_an_otherwise_empty_queue() {
    let queue = DbOutboxQueue {
        uploads: Vec::new(),
        deletes: vec![DbOutboxDelete {
            namespace: "release_files".to_string(),
            blob_id: SMALL_FILE.to_string(),
            created_at: 1_700_000_000_000,
        }],
        make_remotes: Vec::new(),
    };

    let snapshot = build(queue, &HashMap::new());

    assert_eq!(snapshot.pending_delete_count(), 1);
    assert_eq!(snapshot.deletes[0].namespace, "release_files");
    assert_eq!(snapshot.deletes[0].blob_id, SMALL_FILE);
    assert!(snapshot.upload_groups.is_empty());
    assert_eq!(
        snapshot
            .summary_parts()
            .iter()
            .map(|part| part.key.as_str())
            .collect::<Vec<_>>(),
        vec!["core.outbox.pending_deletes"]
    );
}

/// Created is the durable handoff after upload and before publication; it
/// must never reappear as queued while the release gate is still Local.
#[test]
fn created_file_with_lingering_entry_derives_uploaded() {
    let mut queue = two_queued_uploads();
    queue.uploads[0].phase = coven::QueuedUploadPhase::Created;
    queue.uploads[0].provider_bytes_total = Some(116);
    let snapshot = build(queue, &HashMap::new());

    let group = &snapshot.upload_groups[0];
    assert_eq!(group.files.len(), 2, "done file represented exactly once");
    let done = group
        .files
        .iter()
        .find(|f| f.file_id == upload_id(crate::sync::RELEASE_FILES_NAMESPACE, SMALL_FILE))
        .expect("done file listed");
    assert_eq!(done.state, UploadState::Uploaded { bytes_total: 116 });
    assert_eq!(group.progress.uploaded, 1);
    assert_eq!(group.progress.queued, 1);
    assert_eq!(group.progress.preparation_bytes_done, 100);
    assert_eq!(group.progress.upload_bytes_done, 116);
    assert_eq!(group.progress.upload_bytes_total, 116);
    assert_eq!(group.progress.preparation_bytes_total, 1100);
}

#[test]
fn created_handoff_outranks_final_transient_progress() {
    let mut queue = two_queued_uploads();
    queue.uploads.truncate(1);
    queue.uploads[0].phase = coven::QueuedUploadPhase::Created;
    queue.uploads[0].provider_bytes_total = Some(116);
    let transient = HashMap::from([(
        UploadBlobKey::new(crate::sync::RELEASE_FILES_NAMESPACE, SMALL_FILE),
        TransientUploadState::Uploading {
            bytes_done: 116,
            bytes_total: 116,
        },
    )]);

    let snapshot = build(queue, &transient);

    assert_eq!(
        snapshot.upload_groups[0].files[0].state,
        UploadState::Uploaded { bytes_total: 116 }
    );
}

#[test]
fn created_file_with_a_terminalization_error_remains_retryable() {
    let mut queue = two_queued_uploads();
    queue.uploads.truncate(1);
    queue.uploads[0].phase = coven::QueuedUploadPhase::Created;
    queue.uploads[0].provider_bytes_total = Some(116);
    queue.uploads[0].attempt_count = 1;
    queue.uploads[0].last_failure = Some(coven::OutboxFailure::other("publication failed"));

    let snapshot = build(queue, &HashMap::new());

    assert_eq!(snapshot.total.retrying, 1);
    // The provider object exists; only publication failed, so the bar stands
    // at the end of the upload phase rather than resetting.
    assert_eq!(
        snapshot.total.bar(),
        Some(UploadBar {
            phase: UploadPhase::Uploading,
            bytes_done: 116,
            bytes_total: 116,
        })
    );
    assert_eq!(
        snapshot.upload_groups[0].files[0].state,
        UploadState::RetryingPublication {
            last_error: "publication failed".to_string(),
            bytes_total: 116,
        }
    );
}

/// A release with nothing left to ship stops being rendered: its group
/// leaves the snapshot while other releases keep uploading, and the totals
/// cover only the work still on screen.
#[test]
fn fully_done_group_is_dropped_while_queue_busy() {
    // Both of this release's files completed and their queue entries are
    // consumed; a second release still has a queued upload keeping the queue
    // busy.
    let queue = DbOutboxQueue {
        uploads: vec![DbOutboxUpload {
            release_id: OTHER_RELEASE.to_string(),
            ..queued_upload(OTHER_FILE, "03 Track Title.flac", 500)
        }],
        deletes: Vec::new(),
        make_remotes: Vec::new(),
    };
    let snapshot = build(queue, &HashMap::new());

    assert_eq!(snapshot.upload_groups.len(), 1);
    let group = &snapshot.upload_groups[0];
    assert_eq!(group.release_id, OTHER_RELEASE);
    assert_eq!(snapshot.total.preparation_bytes_total, 500);
    assert_eq!(snapshot.total.queued, 1);
    assert!(
        !snapshot.per_release_progress().contains_key(RELEASE),
        "a finished release must fall back to its resting storage badge"
    );
}

/// An idle durable queue is terminal; no transient callback can keep a
/// release visible after publication consumes the intent and upload rows.
#[test]
fn idle_durable_queue_is_terminal() {
    let transient = HashMap::from([(
        UploadBlobKey::new(crate::sync::RELEASE_FILES_NAMESPACE, SMALL_FILE),
        TransientUploadState::Uploading {
            bytes_done: 116,
            bytes_total: 116,
        },
    )]);
    let snapshot = build(DbOutboxQueue::default(), &transient);

    assert!(snapshot.upload_groups.is_empty());
}

#[test]
fn durable_and_streamed_upload_phases_never_collapse_back_to_queued() {
    let mut queue = two_queued_uploads();
    queue.uploads[0].phase = coven::QueuedUploadPhase::Prepared;
    queue.uploads[0].provider_bytes_total = Some(1016);
    queue.uploads[1].phase = coven::QueuedUploadPhase::Created;
    queue.uploads[1].provider_bytes_total = Some(1000);
    let transient = HashMap::from([(
        UploadBlobKey::new(crate::sync::RELEASE_FILES_NAMESPACE, SMALL_FILE),
        TransientUploadState::Uploading {
            bytes_done: 400,
            bytes_total: 1016,
        },
    )]);

    let snapshot = build_outbox_snapshot(queue, &transient, &UploadThroughput::new(), false);

    assert_eq!(snapshot.total.queued, 0);
    assert_eq!(snapshot.total.uploading, 1);
    assert_eq!(snapshot.total.uploaded, 1);
    assert_eq!(snapshot.total.preparation_bytes_done, 1100);
    assert_eq!(snapshot.total.upload_bytes_done, 1400);
    assert_eq!(snapshot.total.upload_bytes_total, 2016);
}

#[test]
fn publishing_intent_keeps_the_release_visible_after_upload_rows_leave() {
    let queue = DbOutboxQueue {
        uploads: Vec::new(),
        deletes: Vec::new(),
        make_remotes: vec![crate::db::DbMakeRemote {
            transition: coven::QueuedMakeRemote {
                root_table: "releases".to_string(),
                root_id: RELEASE.to_string(),
                root_label: "Album Title".to_string(),
                retain_pinned: false,
                progress: coven::MakeRemoteProgress::Publishing,
            },
            album_title: "Album Title".to_string(),
        }],
    };

    let snapshot = build(queue, &HashMap::new());

    assert_eq!(snapshot.upload_groups.len(), 1);
    assert_eq!(snapshot.total.publishing, 1);
    assert_eq!(snapshot.total.activity(), Some(UploadActivity::Publishing));
    assert!(!snapshot.total.can_cancel());
}

#[test]
fn eta_never_reports_zero_while_provider_bytes_remain() {
    assert_eq!(upload_eta_seconds(1, 10), 1);
    assert_eq!(upload_eta_seconds(11, 10), 2);
}

#[test]
fn queue_eta_is_hidden_until_every_provider_denominator_is_known() {
    let mut queue = two_queued_uploads();
    queue.uploads[1].phase = coven::QueuedUploadPhase::Prepared;
    queue.uploads[1].provider_bytes_total = Some(1016);
    let transient = HashMap::from([(
        UploadBlobKey::new(crate::sync::RELEASE_FILES_NAMESPACE, LARGE_FILE),
        TransientUploadState::Uploading {
            bytes_done: 250,
            bytes_total: 1016,
        },
    )]);
    let throughput = UploadThroughput::with_window(std::time::Duration::from_secs(10));
    throughput.begin();
    throughput.record(250);

    let snapshot = build_outbox_snapshot(queue, &transient, &throughput, false);

    assert_eq!(snapshot.eta_seconds, None);
}

#[test]
fn queue_eta_uses_every_exact_provider_denominator_when_they_are_known() {
    let mut queue = two_queued_uploads();
    queue.uploads[0].phase = coven::QueuedUploadPhase::Prepared;
    queue.uploads[0].provider_bytes_total = Some(116);
    queue.uploads[1].phase = coven::QueuedUploadPhase::Prepared;
    queue.uploads[1].provider_bytes_total = Some(1016);
    let transient = HashMap::from([(
        UploadBlobKey::new(crate::sync::RELEASE_FILES_NAMESPACE, LARGE_FILE),
        TransientUploadState::Uploading {
            bytes_done: 250,
            bytes_total: 1016,
        },
    )]);
    let throughput = UploadThroughput::with_window(std::time::Duration::from_secs(10));
    throughput.begin();
    throughput.record(250);

    let snapshot = build_outbox_snapshot(queue, &transient, &throughput, false);

    assert_eq!(snapshot.total.upload_bytes_done, 250);
    assert_eq!(snapshot.total.upload_bytes_total, 1132);
    assert!(snapshot.eta_seconds.is_some());
}

#[test]
#[should_panic(expected = "preparation progress must use the source's exact plaintext total")]
fn preparation_progress_rejects_a_changed_denominator() {
    let mut progress = UploadProgress::default();
    progress.add_upload(
        &UploadState::Preparing {
            bytes_done: 40,
            bytes_total: 90,
        },
        100,
    );
}

#[test]
#[should_panic(expected = "provider progress cannot exceed its exact total")]
fn provider_progress_rejects_bytes_beyond_its_denominator() {
    let mut progress = UploadProgress::default();
    progress.add_upload(
        &UploadState::Uploading {
            bytes_done: 101,
            bytes_total: 100,
        },
        100,
    );
}

#[test]
fn a_slice_counts_source_bytes_until_every_provider_size_is_exact() {
    // The large file is uploading; the small one has not been prepared, so no
    // exact provider denominator exists for the slice yet.
    let mut queue = two_queued_uploads();
    queue.uploads[1].phase = coven::QueuedUploadPhase::Prepared;
    queue.uploads[1].provider_bytes_total = Some(1016);
    let transient = HashMap::from([(
        UploadBlobKey::new(crate::sync::RELEASE_FILES_NAMESPACE, LARGE_FILE),
        TransientUploadState::Uploading {
            bytes_done: 250,
            bytes_total: 1016,
        },
    )]);

    let snapshot = build(queue, &transient);

    // Source bytes, against the source total — never 250 provider bytes under a
    // source denominator, and never the source size standing in for the
    // unknown provider one.
    assert_eq!(
        snapshot.total.bar(),
        Some(UploadBar {
            phase: UploadPhase::Preparing,
            bytes_done: 1000,
            bytes_total: 1100,
        })
    );
}

#[test]
fn a_slice_counts_provider_bytes_once_every_upload_is_prepared() {
    let mut queue = two_queued_uploads();
    for (upload, provider_total) in queue.uploads.iter_mut().zip([116, 1016]) {
        upload.phase = coven::QueuedUploadPhase::Prepared;
        upload.provider_bytes_total = Some(provider_total);
    }
    let transient = HashMap::from([(
        UploadBlobKey::new(crate::sync::RELEASE_FILES_NAMESPACE, LARGE_FILE),
        TransientUploadState::Uploading {
            bytes_done: 250,
            bytes_total: 1016,
        },
    )]);

    let snapshot = build(queue, &transient);

    assert_eq!(
        snapshot.total.bar(),
        Some(UploadBar {
            phase: UploadPhase::Uploading,
            bytes_done: 250,
            bytes_total: 1132,
        })
    );
}

#[test]
fn a_queued_slice_counts_the_source_bytes_it_has_yet_to_read() {
    let snapshot = build(two_queued_uploads(), &HashMap::new());

    assert_eq!(
        snapshot.total.bar(),
        Some(UploadBar {
            phase: UploadPhase::Preparing,
            bytes_done: 0,
            bytes_total: 1100,
        })
    );
}

#[test]
fn cancelling_and_publishing_slices_carry_no_byte_counters() {
    let cancelling = UploadProgress {
        cancelling: 1,
        preparation_bytes_done: 500,
        preparation_bytes_total: 1100,
        upload_bytes_total_complete: false,
        ..Default::default()
    };
    assert_eq!(cancelling.bar(), None);

    // A release down to its make-Remote transition has no upload rows left to
    // count.
    let publishing = UploadProgress {
        publishing: 1,
        ..Default::default()
    };
    assert_eq!(publishing.bar(), None);
}

#[test]
fn a_files_bar_counts_the_phase_that_file_is_in() {
    assert_eq!(
        UploadState::Preparing {
            bytes_done: 40,
            bytes_total: 100,
        }
        .bar(),
        Some(UploadBar {
            phase: UploadPhase::Preparing,
            bytes_done: 40,
            bytes_total: 100,
        })
    );
    assert_eq!(
        UploadState::Uploading {
            bytes_done: 40,
            bytes_total: 116,
        }
        .bar(),
        Some(UploadBar {
            phase: UploadPhase::Uploading,
            bytes_done: 40,
            bytes_total: 116,
        })
    );
    // Resting states render their state and size; a bar there would draw
    // motion that is not happening.
    for state in [
        UploadState::Queued,
        UploadState::Prepared { bytes_total: 116 },
        UploadState::RetryingPreparation {
            last_error: "read failed".to_string(),
        },
        UploadState::RetryingUpload {
            last_error: "provider refused".to_string(),
            bytes_total: 116,
        },
        UploadState::RetryingPublication {
            last_error: "publication failed".to_string(),
            bytes_total: 116,
        },
        UploadState::Uploaded { bytes_total: 116 },
    ] {
        assert_eq!(state.bar(), None, "{state:?} must not draw a bar");
    }
}

#[test]
#[should_panic(expected = "upload progress cannot exceed its exact total")]
fn a_bar_rejects_progress_beyond_its_denominator() {
    let progress = UploadProgress {
        uploaded: 1,
        preparation_bytes_done: 100,
        preparation_bytes_total: 100,
        upload_bytes_done: 200,
        upload_bytes_total: 116,
        upload_bytes_total_complete: true,
        ..Default::default()
    };

    progress.bar();
}
