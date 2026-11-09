//! Exports and saves: the output queue, the bytes and folder names an export
//! writes, a failed export, a save's preset captured at enqueue, and on Windows
//! a name holding an unpaired surrogate.

use super::*;
use crate::manager::tests::*;
use bae_records::{DbRelease, NewReleaseFile};
use chrono::Utc;

#[test]
fn replace_output_dir_restores_prior_export_when_staging_rename_fails() {
    let temp = tempfile::tempdir().expect("create temp dir");
    let final_dir = temp.path().join("album");
    std::fs::create_dir(&final_dir).expect("create prior export dir");
    std::fs::write(final_dir.join(OUTPUT_MARKER_FILE), b"release-1").expect("write prior marker");
    std::fs::write(final_dir.join("prior.txt"), b"prior").expect("write prior export");

    let missing_staging = temp.path().join("missing-staging");
    replace_output_dir(&missing_staging, &final_dir, "album", "release-1")
        .expect_err("missing staging fails");

    assert_eq!(std::fs::read(final_dir.join("prior.txt")).expect("read prior export"), b"prior");
    assert!(!temp.path().join(".album.replace-release-1").exists());
}

#[test]
fn replace_output_dir_refuses_to_replace_unmarked_directory() {
    let temp = tempfile::tempdir().expect("create temp dir");
    let final_dir = temp.path().join("album");
    std::fs::create_dir(&final_dir).expect("create existing dir");
    std::fs::write(final_dir.join("user-file.txt"), b"user data").expect("write user file");

    let staging_dir = temp.path().join("staging");
    std::fs::create_dir(&staging_dir).expect("create staging dir");
    std::fs::write(staging_dir.join(OUTPUT_MARKER_FILE), b"release-1").expect("write marker");
    std::fs::write(staging_dir.join("export.txt"), b"export").expect("write export");

    let error = replace_output_dir(&staging_dir, &final_dir, "album", "release-1")
        .expect_err("unmarked existing target must be refused");

    assert!(error.to_string().contains("is not a prior bae output"), "unexpected error: {error}",);
    assert_eq!(
        std::fs::read(final_dir.join("user-file.txt")).expect("read user file"),
        b"user data",
    );
    assert!(staging_dir.exists());
    assert!(!temp.path().join(".album.replace-release-1").exists());
}

#[test]
fn replace_output_dir_replaces_marked_directory() {
    let temp = tempfile::tempdir().expect("create temp dir");
    let final_dir = temp.path().join("album");
    std::fs::create_dir(&final_dir).expect("create prior export dir");
    std::fs::write(final_dir.join(OUTPUT_MARKER_FILE), b"release-1").expect("write prior marker");
    std::fs::write(final_dir.join("prior.txt"), b"prior").expect("write prior export");

    let staging_dir = temp.path().join("staging");
    std::fs::create_dir(&staging_dir).expect("create staging dir");
    std::fs::write(staging_dir.join(OUTPUT_MARKER_FILE), b"release-1").expect("write marker");
    std::fs::write(staging_dir.join("export.txt"), b"export").expect("write export");

    replace_output_dir(&staging_dir, &final_dir, "album", "release-1")
        .expect("marked prior export can be replaced");

    assert_eq!(std::fs::read(final_dir.join("export.txt")).expect("read export"), b"export",);
    assert!(!final_dir.join("prior.txt").exists());
    assert!(!temp.path().join(".album.replace-release-1").exists());
}

/// Create a Remote, pinned, exportable release: a Local release with
/// known-byte source files on disk (device-local paths) and a
/// `source_folder_name`, made Remote with pin so its files stay readable from
/// the offline cache. Returns its id. The manager must already be connected via
/// [`connect_test_cloud`].
async fn make_exportable_release(
    manager: &LibraryManager,
    source_dir: &std::path::Path,
    folder_name: &str,
    files: &[(&str, &[u8])],
) -> String {
    let (release, _) = insert_export_release_rows(manager, source_dir, folder_name, files).await;
    manager.upload_release(&release.id, true).await.unwrap();
    // The uploads this fixture's tests leave running drain the queue, so
    // wait for the upload to finish rather than counting a drain pass this
    // test does not own.
    wait_for_landed_upload(manager, &release.id).await;
    release.id
}

async fn insert_export_release_rows(
    manager: &LibraryManager,
    source_dir: &std::path::Path,
    folder_name: &str,
    files: &[(&str, &[u8])],
) -> (DbRelease, Vec<NewReleaseFile>) {
    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.source_folder_name = Some(folder_name.to_string());
    manager.records.insert_album(&album).await.unwrap();
    insert_release(manager, &release).await;
    std::fs::create_dir_all(source_dir).unwrap();
    let created_at = Utc::now();
    let mut inserted_files = Vec::with_capacity(files.len());
    for (index, (name, bytes)) in files.iter().enumerate() {
        let file = NewReleaseFile::new(
            &release.id,
            name,
            bae_util::content_type::ContentType::Flac,
            bae_util::test_ids::test_uuid(&format!("{}-export-file-{index}", release.id)),
            created_at,
        );
        let path = source_dir.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, bytes).unwrap();
        manager.add_external_file_for_test(&file, &path).await.unwrap();
        inserted_files.push(file);
    }
    (release, inserted_files)
}

/// A Local release with one readable source file, whose stored name is then
/// overwritten with `name` — the shape a row pulled from another device can
/// have. coven applies a pulled changeset straight into SQLite, so bae's own
/// row-write never sees it: the export copy-out and audio detachment build each
/// path from the names as they read them.
async fn insert_local_export_release_named(
    manager: &LibraryManager,
    source_dir: &std::path::Path,
    bytes: &[u8],
    name: PeerName<'_>,
) -> String {
    let (release, db_files) =
        insert_export_release_rows(manager, source_dir, "Album Title", &[("track.flac", bytes)])
            .await;
    match name {
        PeerName::OriginalFilename(value) => {
            manager.records.set_original_filename_for_test(&db_files[0].id, value).await.unwrap()
        }
        PeerName::SourceFolderName(value) => {
            manager.records.set_source_folder_name_for_test(&release.id, value).await.unwrap()
        }
    }
    release.id
}

enum PeerName<'a> {
    OriginalFilename(&'a str),
    SourceFolderName(&'a str),
}

/// Pausing before the first enqueue parks the worker, so the queue's
/// state (enqueue, dedup, target_dir, cancel) is observable deterministically
/// without the export path racing the assertions.
#[tokio::test]
async fn output_queue_enqueue_dedups_and_cancels_while_paused() {
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let release_id = insert_pinnable_release(&test).await;

    manager.set_outputs_paused(true).await.unwrap();

    let target = temp_dir.path().join("export-out");
    manager.enqueue_export(&release_id, target.clone()).await.unwrap();
    let snap = output_queue_now(manager).await;
    assert_eq!(snap.total.queued, 1);
    assert_eq!(snap.rows.len(), 1);
    assert_eq!(snap.rows[0].op.payload.target_dir, target);
    assert_eq!(snap.rows[0].op.state, bae_transfers::OutputPhase::Queued);
    assert!(snap.paused);

    // Re-enqueuing the same release is a no-op: still one entry.
    manager.enqueue_export(&release_id, target.clone()).await.unwrap();
    assert_eq!(output_queue_now(manager).await.rows.len(), 1);

    // Cancel drops the entry.
    manager.cancel_output(&release_id).await.unwrap();
    assert!(output_queue_now(manager).await.rows.is_empty());
}

/// The verbatim copy-out: exported bytes equal the source bytes, laid out at
/// `<target>/<source_folder_name>/<original_filename>` (including nested
/// subfolders), and the export changes no release state — it stays Remote with
/// no new cloud-outbox rows.
#[tokio::test]
async fn export_writes_exact_bytes_in_source_folder_and_leaves_release_remote() {
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    connect_test_cloud_uploading(&test).await;

    let source_dir = temp_dir.path().join("source");
    let files: &[(&str, &[u8])] =
        &[("cover.jpg", b"cover-bytes-abc"), ("CD1/track.flac", b"flac-bytes-0123456789")];
    let release_id =
        make_exportable_release(manager, &source_dir, "Album Title (2020)", files).await;

    // Precondition: Remote, no pending uploads after the drain.
    assert!(release_files(manager, &release_id).await.iter().all(|file| file.stored));
    assert!(!has_pending_uploads(manager, &release_id).await);

    let target = temp_dir.path().join("export-out");
    manager.enqueue_export(&release_id, target.clone()).await.unwrap();

    // Success removes the entry from the queue.
    output_queue_until(manager, |queue| queue.rows.is_empty()).await;

    // Byte-accuracy + folder layout.
    for (name, bytes) in files {
        let written = target.join("Album Title (2020)").join(name);
        let got = std::fs::read(&written).unwrap_or_else(|e| panic!("read exported {name}: {e}"));
        assert_eq!(&got, bytes, "exported bytes for {name} match the source");
    }

    // The staging directory was renamed into place, leaving nothing behind it.
    let leftover = std::fs::read_dir(&target)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        leftover,
        vec!["Album Title (2020)".to_string()],
        "only the final export folder remains under the target; no staging dir"
    );

    // Export changed no release state: still Remote, no new outbox rows.
    assert!(release_files(manager, &release_id).await.iter().all(|file| file.stored));
    assert!(!has_pending_uploads(manager, &release_id).await, "export enqueues no cloud uploads");
}

/// A file named `..` is an ordinary name: coven writes it as `.．`, so the
/// export writes the file inside its folder and nothing outside the target.
#[tokio::test]
async fn export_writes_a_parent_name_inside_its_folder() {
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let release_id = insert_local_export_release_named(
        manager,
        &temp_dir.path().join("source"),
        b"escape-bytes",
        PeerName::OriginalFilename("../escape.flac"),
    )
    .await;

    let target = temp_dir.path().join("export-out");
    manager
        .export_release(&release_id, &target, bae_transfers::OutputKind::Export)
        .await
        .expect("a parent name exports");

    assert_eq!(
        std::fs::read(target.join("Album Title").join(".．").join("escape.flac")).unwrap(),
        b"escape-bytes"
    );
    assert!(!target.join("escape.flac").exists());
    assert!(!temp_dir.path().join("escape.flac").exists());
}

/// An absolute path's leading `/` gives an empty name, which names no file:
/// the export writes nothing.
#[tokio::test]
async fn export_refuses_an_absolute_original_filename() {
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let release_id = insert_local_export_release_named(
        manager,
        &temp_dir.path().join("source"),
        b"escape-bytes",
        PeerName::OriginalFilename("/absolute-escape.flac"),
    )
    .await;

    let target = temp_dir.path().join("export-out");
    let error = manager
        .export_release(&release_id, &target, bae_transfers::OutputKind::Export)
        .await
        .expect_err("an absolute original_filename is refused");

    assert!(
        matches!(
            &error,
            LibraryError::Database(bae_store::DbError::Unreadable { stored, .. })
                if stored.contains("/absolute-escape.flac")
        ),
        "a name that names no file is a damaged row: {error}"
    );
    assert!(!target.join("Album Title").exists());
}

/// A folder name is one name, `/` included: coven writes `../escape-folder`
/// as `..／escape-folder`, one folder inside the target.
#[tokio::test]
async fn export_writes_a_folder_name_holding_a_slash_as_one_folder() {
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let release_id = insert_local_export_release_named(
        manager,
        &temp_dir.path().join("source"),
        b"track-bytes",
        PeerName::SourceFolderName("../escape-folder"),
    )
    .await;

    let target = temp_dir.path().join("export-out");
    manager
        .export_release(&release_id, &target, bae_transfers::OutputKind::Export)
        .await
        .expect("a folder name holding a slash exports");

    assert_eq!(
        std::fs::read(target.join("..／escape-folder").join("track.flac")).unwrap(),
        b"track-bytes"
    );
    assert!(!temp_dir.path().join("escape-folder").exists());
}

/// A write error (an unwritable target) marks the export `Failed` with a message
/// and keeps it in the queue until it is cancelled.
#[tokio::test]
async fn export_write_error_marks_failed_until_cancelled() {
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    connect_test_cloud_uploading(&test).await;

    let source_dir = temp_dir.path().join("source");
    let release_id = make_exportable_release(
        manager,
        &source_dir,
        "Album Title",
        &[("track.flac", b"track-bytes")],
    )
    .await;

    // Target a path that is actually a file, so creating the release subfolder
    // under it fails with an I/O error (the read succeeds; the write doesn't).
    let blocker = temp_dir.path().join("blocker");
    std::fs::write(&blocker, b"a file, not a directory").unwrap();

    manager.enqueue_export(&release_id, blocker.clone()).await.unwrap();

    // An unwritable target marks the export Failed.
    let failed = output_queue_until(manager, |queue| {
        matches!(
            queue.rows.first().map(|row| &row.op.state),
            Some(bae_transfers::OutputPhase::Failed { .. })
        )
    })
    .await;
    assert_eq!(failed.total.failed, 1);

    manager.cancel_output(&release_id).await.unwrap();
    assert!(output_queue_now(manager).await.rows.is_empty(), "cancel removes the entry");
}

/// A failure partway through the export (a read error on a later file, after an
/// earlier file has already been written to staging) leaves NO output at the
/// final `<target>/<source_folder_name>/` path — the staging directory is
/// removed, so the export is all-or-nothing.
#[tokio::test]
async fn export_mid_failure_leaves_no_partial_output_at_final_path() {
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();

    // A Local release whose two source files live on disk as device-local paths
    // (UserProvided reads straight from the user's own file). Keeping it Local —
    // never made Remote — means the export reads these files directly, so removing
    // one forces a read error on a later file mid-export.
    let source_dir = temp_dir.path().join("source");
    std::fs::create_dir_all(&source_dir).unwrap();
    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.source_folder_name = Some("Album Title".to_string());
    manager.records.insert_album(&album).await.unwrap();
    insert_release(manager, &release).await;

    let files: &[(&str, &[u8])] = &[("01.flac", b"first-ok"), ("02.flac", b"second-fails")];
    for (name, bytes) in files {
        std::fs::write(source_dir.join(name), bytes).unwrap();
        let file = NewReleaseFile::new(
            &release.id,
            name,
            bae_util::content_type::ContentType::Flac,
            coven::IdSource::new_id(&coven::UuidIds).to_string(),
            Utc::now(),
        );
        manager.add_external_file_for_test(&file, &source_dir.join(name)).await.unwrap();
    }

    // Pause so the source file can be deleted before the worker runs, making the
    // later read fail deterministically rather than racing the copy.
    manager.set_outputs_paused(true).await.unwrap();
    let target = temp_dir.path().join("export-out");
    std::fs::create_dir_all(&target).unwrap();
    manager.enqueue_export(&release.id, target.clone()).await.unwrap();
    std::fs::remove_file(source_dir.join("02.flac")).unwrap();
    manager.set_outputs_paused(false).await.unwrap();

    // A read error on a later file marks the export Failed.
    output_queue_until(manager, |queue| {
        matches!(
            queue.rows.first().map(|row| &row.op.state),
            Some(bae_transfers::OutputPhase::Failed { .. })
        )
    })
    .await;

    // The all-or-nothing guarantee: nothing at the final path, and the staging
    // directory was removed — the target holds no export output at all.
    assert!(!target.join("Album Title").exists(), "no partial output at the final export path");
    assert_eq!(
        std::fs::read_dir(&target).unwrap().count(),
        0,
        "staging directory cleaned up; target left empty"
    );
}

/// The preset is captured whole at enqueue: editing (or deleting) it afterward
/// can't change or break the already-queued save.
#[tokio::test]
async fn enqueue_release_save_captures_the_preset() {
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let release_id = insert_pinnable_release(&test).await;
    manager.set_outputs_paused(true).await.unwrap();

    let target = temp_dir.path().join("save-out");
    manager.enqueue_release_save(&release_id, target, "flac").await.unwrap();

    // Rename the "flac" preset after enqueue; the queued save keeps the old one.
    let edited: Vec<_> = manager
        .save_presets()
        .await
        .unwrap()
        .into_iter()
        .map(|mut preset| {
            if preset.id == "flac" {
                preset.name = "FLAC EDITED".to_string();
            }
            preset
        })
        .collect();
    manager.set_save_presets(edited).await.unwrap();

    let snap = output_queue_now(manager).await;
    let bae_transfers::OutputKind::Save { preset } = &snap.rows[0].op.payload.kind else {
        panic!("expected a queued save op");
    };
    assert_eq!(
        preset.name, "FLAC",
        "the queued save uses the preset captured at enqueue, not the edited one"
    );
}

/// The export queue as its table holds it now.
#[cfg(desktop)]
async fn output_queue_now(manager: &LibraryManager) -> bae_transfers::OutputPhaseSnapshot {
    manager.outputs.snapshot().await.unwrap()
}

/// Wait until the export queue's live read holds what `done` looks for.
#[cfg(desktop)]
async fn output_queue_until(
    manager: &LibraryManager,
    done: impl Fn(&bae_transfers::OutputPhaseSnapshot) -> bool,
) -> bae_transfers::OutputPhaseSnapshot {
    let mut values = manager.subscribe_output_snapshot();
    tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
        loop {
            let queue = values.next().await.expect("the query stays open").unwrap();
            if done(&queue) {
                return queue;
            }
        }
    })
    .await
    .expect("the export queue reaches the state waited for")
}

#[cfg(windows)]
mod imported_names {
    use super::*;
    use std::os::windows::ffi::OsStringExt;

    /// What the import `import_id` reported: the release it created, or the
    /// error it failed with.
    async fn import_outcome(
        events: &mut bae_import::ImportEvents,
        import_id: &str,
    ) -> Result<String, String> {
        loop {
            let event = tokio::time::timeout(bae_util::test_channels::HANG_GUARD, events.recv())
                .await
                .expect("the import reports its result")
                .expect("the import event stream remains open");
            let bae_import::ImportEvent::ImportProgress { progress, .. } = event else {
                continue;
            };
            match progress {
                bae_import::ImportProgress::Complete { import_id: completed, id, .. }
                    if completed == import_id =>
                {
                    return Ok(id)
                }
                bae_import::ImportProgress::Failed { import_id: failed, error }
                    if failed == import_id =>
                {
                    return Err(error)
                }
                _ => {}
            }
        }
    }

    /// Windows takes a name holding an unpaired UTF-16 surrogate, which isn't
    /// Unicode, so the release stores it as escaped text (`Caf\xED\xA0\x80.flac`),
    /// whose `\` Windows reads as a separator. A stored name is a label: the track
    /// imports under that text, and its export writes the name coven writes for
    /// it, the same on every platform.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_track_named_with_an_unpaired_surrogate_imports_and_exports() {
        let (test, temp) = setup_test_manager().await;
        let manager = test.library.manager();
        let root = temp.path().join("watched");
        let folder = root.join("Album");
        std::fs::create_dir_all(&folder).unwrap();
        let flac =
            std::fs::read(bae_import::test_fixtures::flac_dir().join("01 Test Track 1.flac"))
                .unwrap();
        let name: Vec<u16> =
            "Caf".encode_utf16().chain([0xD800]).chain(".flac".encode_utf16()).collect();
        std::fs::write(folder.join(std::ffi::OsString::from_wide(&name)), &flac).unwrap();
        let import = test.library.start_import().await.unwrap();
        import.add_watched_folder(root.clone()).await.unwrap();
        import.refresh_watched_folder(root.clone()).await.unwrap();
        let key = bae_import::CandidateKey::folder(&folder);
        import.select_candidate_file_tags(key.clone()).await.unwrap();
        import
            .set_candidate_edit_field(
                &key,
                bae_import::DraftFieldEdit::Text {
                    field: bae_import::CandidateEditField::AlbumTitle,
                    value: "Album Title".into(),
                },
            )
            .await
            .unwrap();
        import
            .set_candidate_album_artists(
                &key,
                vec![bae_import::Credited::plain(bae_import::ArtistAssignment::named(
                    "Artist Name",
                ))],
            )
            .await
            .unwrap();
        let mut events = import.every_event();
        let import_id = import.start_import(&key).await.unwrap().expect_queued();
        let release_id = import_outcome(&mut events, &import_id)
            .await
            .unwrap_or_else(|error| panic!("the track imports: {error}"));

        let files = release_files(&manager, &release_id).await;
        let names: Vec<&str> = files.iter().map(|file| file.original_filename.as_str()).collect();
        assert_eq!(names, [r"Caf\xED\xA0\x80.flac"]);

        let target = temp.path().join("export");
        manager
            .export_release(&release_id, &target, bae_transfers::OutputKind::Export)
            .await
            .expect("the release exports");
        assert_eq!(
            std::fs::read(target.join("Album").join("Caf＼xED＼xA0＼x80.flac")).unwrap(),
            flac
        );
        bae_util::test_blocking::on_blocking_pool(move || import.stop_and_join()).await;
    }
}
