mod metadata_edits;
// The pane's own controls, from the handle down to the next read.
//
// Every one of these writes a row and returns; nothing is handed back to the
// caller. So what each test asserts is what the pane would draw next — the
// `candidate_pane` read — because that is the only path the UI has to what it
// just wrote.

fn cover_jpeg() -> Vec<u8> {
    let image = image::RgbImage::from_pixel(16, 16, image::Rgb([90, 30, 160]));
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image)
        .write_to(&mut bytes, image::ImageFormat::Jpeg)
        .expect("encode cover jpeg");
    bytes.into_inner()
}

/// A watched root holding one folder of real audio and one image, scanned into
/// the tables and picked as its own tags.
///
/// The folder is stored rather than scanned for: the pane's writes are what is
/// under test, and a real scan would only make when they run less certain.
async fn picked_candidate(
    manager: &LibraryManager,
    tmp: &TempDir,
) -> (FolderCandidate, String, String) {
    use crate::import::folder_scanner::{
        CandidateFile, CategorizedFiles, FileRole, ReleaseFileScope, ScannedFile,
    };

    let root = tmp.path().join("watched");
    let folder = root.join("Album");
    std::fs::create_dir_all(&folder).unwrap();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/flac");
    let mut files = Vec::new();
    for (index, name) in ["01 Test Track 1.flac", "02 Test Track 2.flac"]
        .into_iter()
        .enumerate()
    {
        let relative_path = format!("{:02} Track.flac", index + 1);
        let path = folder.join(&relative_path);
        std::fs::copy(fixtures.join(name), &path).unwrap();
        let metadata = std::fs::metadata(&path).unwrap();
        let size = metadata.len();
        let modified_at_ns =
            crate::import::folder_scanner::file_modified_at_ns(&path, &metadata).unwrap();
        files.push(CandidateFile {
            proposed_audio: true,
            file: ScannedFile::new(path.clone(), relative_path, size, modified_at_ns)
                .with_test_flac_audio(),
            role: FileRole::Audio,
        });
    }
    let cover_path = folder.join("cover.jpg");
    std::fs::write(&cover_path, cover_jpeg()).unwrap();
    let cover_metadata = std::fs::metadata(&cover_path).unwrap();
    let cover_modified_at_ns =
        crate::import::folder_scanner::file_modified_at_ns(&cover_path, &cover_metadata).unwrap();
    files.push(CandidateFile {
        proposed_audio: false,
        file: ScannedFile::new(
            cover_path.clone(),
            "cover.jpg".to_string(),
            cover_metadata.len(),
            cover_modified_at_ns,
        ),
        role: FileRole::Artwork,
    });

    let candidate = FolderCandidate {
        path: folder.clone(),
        file_root: folder.clone(),
        name: "Album".to_string(),
        files: CategorizedFiles {
            files,
        },
        watched_folder_path: root.to_string_lossy().into_owned(),
        scope: ReleaseFileScope::Recursive,
        file_edit_revision: 0,
        display_path: "Album".to_string(),
        resolved_boundaries: Vec::new(),
        combine_ancestor_key: None,
    };

    let root = root.to_string_lossy().into_owned();
    manager.add_watched_import_folder(&root).await.unwrap();
    let generation = manager.begin_folder_scan(&root).await.unwrap();
    manager
        .save_folder_scan_item(
            &root,
            generation,
            &crate::import::folder_scanner::ScanItem::Valid(candidate.clone()),
        )
        .await
        .unwrap();
    manager
        .finish_folder_scan(&root, generation, None)
        .await
        .unwrap();

    let key = folder.to_string_lossy().into_owned();
    let hash = candidate.files.content_hash();
    (candidate, key, hash)
}

/// The handle, the key its controls address, and the hash its rows are stored
/// under — with the folder's own tags already picked, which is what draws the
/// edit form and the mapping table.
async fn pane_fixture() -> (ImportServiceHandle, TempDir, String, String) {
    let (manager, tmp) = setup_test_manager().await;
    let (_candidate, key, hash) = picked_candidate(&manager, &tmp).await;
    let handle = manager
        .start_import_service(tokio::runtime::Handle::current())
        .await
        .unwrap();
    handle
        .preview_file_tags_for_folder(key.clone())
        .await
        .unwrap();
    let revision = handle
        .select_candidate_metadata_provenance(
            key.clone(),
            crate::import::MetadataProvenance::FileTags,
        )
        .await
        .unwrap();
    assert_eq!(revision, 1);
    assert_eq!(pane(&handle, &key).await.metadata_revision, revision);
    (handle, tmp, key, hash)
}

async fn pane(handle: &ImportServiceHandle, key: &str) -> crate::import::ImportCandidateDetail {
    handle
        .candidate_pane(key)
        .await
        .unwrap()
        .expect("the picked candidate reads back")
}

async fn shut_down(handle: ImportServiceHandle) {
    tokio::task::spawn_blocking(move || handle.stop_and_join())
        .await
        .unwrap();
}

struct CountingFileTagReader {
    reads: std::sync::atomic::AtomicUsize,
    fail_on_read: Option<usize>,
    first_read: Option<(
        std::sync::mpsc::SyncSender<()>,
        std::sync::Arc<std::sync::Barrier>,
    )>,
    embedded_cover: Option<Vec<u8>>,
}

impl CountingFileTagReader {
    fn immediate() -> Self {
        Self {
            reads: std::sync::atomic::AtomicUsize::new(0),
            fail_on_read: None,
            first_read: None,
            embedded_cover: None,
        }
    }

    fn failing(fail_on_read: usize) -> Self {
        Self {
            reads: std::sync::atomic::AtomicUsize::new(0),
            fail_on_read: Some(fail_on_read),
            first_read: None,
            embedded_cover: None,
        }
    }

    fn blocking(
        entered: std::sync::mpsc::SyncSender<()>,
        resume: std::sync::Arc<std::sync::Barrier>,
    ) -> Self {
        Self {
            reads: std::sync::atomic::AtomicUsize::new(0),
            fail_on_read: None,
            first_read: Some((entered, resume)),
            embedded_cover: None,
        }
    }

    fn with_embedded_cover(data: Vec<u8>) -> Self {
        Self {
            reads: std::sync::atomic::AtomicUsize::new(0),
            fail_on_read: None,
            first_read: None,
            embedded_cover: Some(data),
        }
    }

    fn read_count(&self) -> usize {
        self.reads.load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl crate::import::file_tag_snapshot::FileTagReader for CountingFileTagReader {
    fn read(
        &self,
        _path: &std::path::Path,
    ) -> Result<crate::import::file_tag_snapshot::FileTagRead, crate::import::ImportError> {
        let index = self
            .reads
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if index == 0 {
            if let Some((entered, resume)) = &self.first_read {
                entered.send(()).unwrap();
                resume.wait();
            }
        }
        if self.fail_on_read == Some(index) {
            return Err(crate::import::ImportError::FileTags {
                detail: format!("fixture tag read {index} failed"),
            });
        }
        Ok(crate::import::file_tag_snapshot::FileTagRead {
            title: Some(format!("Track Title {}", index + 1)),
            track_artist: Some("Artist Name".to_string()),
            album_title: Some("Album Title".to_string()),
            album_artist: Some("Album Artist".to_string()),
            year: Some(2020),
            track_number: Some(u32::try_from(index + 1).unwrap()),
            disc_number: Some(1),
            embedded_cover: if index == 0 {
                self.embedded_cover.clone().map(|data| {
                    (data, crate::util::content_type::ContentType::Jpeg)
                })
            } else {
                None
            },
        })
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn one_unreadable_file_stores_no_partial_tag_snapshot() {
    let (manager, tmp) = setup_test_manager().await;
    let (candidate, key, _hash) = picked_candidate(&manager, &tmp).await;
    let handle = manager
        .start_import_service(tokio::runtime::Handle::current())
        .await
        .unwrap();
    let reader = std::sync::Arc::new(CountingFileTagReader::failing(1));

    let error = handle
        .file_tag_snapshot_with_reader(&key, reader.clone())
        .await
        .expect_err("the second unreadable audio file rejects the complete reading");
    let stored = handle
        .library_manager
        .load_candidate_file_tag_snapshot(&candidate.watched_folder_path, &key)
        .await
        .unwrap()
        .expect("the candidate remains stored");
    shut_down(handle).await;

    assert!(error.to_string().contains("fixture tag read 1 failed"));
    assert_eq!(reader.read_count(), 2);
    assert!(
        stored.snapshot.is_none(),
        "facts from the first file cannot land without the complete reading"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn matching_file_observations_reuse_the_stored_tag_snapshot() {
    let (manager, tmp) = setup_test_manager().await;
    let (_candidate, key, _hash) = picked_candidate(&manager, &tmp).await;
    let handle = manager
        .start_import_service(tokio::runtime::Handle::current())
        .await
        .unwrap();
    let reader = std::sync::Arc::new(CountingFileTagReader::immediate());

    let first = handle
        .file_tag_snapshot_with_reader(&key, reader.clone())
        .await
        .unwrap();
    assert_eq!(reader.read_count(), 2);
    let preview = handle
        .preview_file_tags_for_folder(key.clone())
        .await
        .unwrap();
    assert_eq!(preview.album_title, "Album Title");
    assert_eq!(
        preview
            .tracks
            .iter()
            .map(|track| track.title.as_str())
            .collect::<Vec<_>>(),
        vec!["Track Title 1", "Track Title 2"]
    );
    let second = handle
        .file_tag_snapshot_with_reader(&key, reader.clone())
        .await
        .unwrap();

    assert_eq!(second, first);
    assert_eq!(
        reader.read_count(),
        2,
        "an exact stored observation avoids opening the audio tags again"
    );
    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn changed_file_observations_replace_the_complete_tag_snapshot() {
    let (manager, tmp) = setup_test_manager().await;
    let (mut candidate, key, _hash) = picked_candidate(&manager, &tmp).await;
    let handle = manager
        .start_import_service(tokio::runtime::Handle::current())
        .await
        .unwrap();
    let reader = std::sync::Arc::new(CountingFileTagReader::immediate());

    handle
        .file_tag_snapshot_with_reader(&key, reader.clone())
        .await
        .unwrap();

    let first_audio = tmp.path().join("watched/Album/01 Track.flac");
    let audio = std::fs::OpenOptions::new()
        .write(true)
        .open(&first_audio)
        .unwrap();
    audio
        .set_times(
            std::fs::FileTimes::new().set_modified(
                std::time::SystemTime::now() + std::time::Duration::from_secs(5),
            ),
        )
        .unwrap();

    let after_modified_time = handle
        .file_tag_snapshot_with_reader(&key, reader.clone())
        .await
        .unwrap()
        .1;
    assert_eq!(reader.read_count(), 4);
    assert_eq!(
        after_modified_time
            .files
            .iter()
            .map(|fact| fact.title.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("Track Title 3"), Some("Track Title 4")]
    );

    let second_audio = tmp.path().join("watched/Album/02 Track.flac");
    let mut audio = std::fs::OpenOptions::new()
        .append(true)
        .open(&second_audio)
        .unwrap();
    std::io::Write::write_all(&mut audio, &[0]).unwrap();
    candidate
        .files
        .files
        .iter_mut()
        .find(|file| file.file.relative_path == "02 Track.flac")
        .unwrap()
        .file
        .size = std::fs::metadata(&second_audio).unwrap().len();
    let root = candidate.watched_folder_path.clone();
    let generation = manager.begin_folder_scan(&root).await.unwrap();
    manager
        .save_folder_scan_item(
            &root,
            generation,
            &crate::import::folder_scanner::ScanItem::Valid(candidate),
        )
        .await
        .unwrap();
    manager
        .finish_folder_scan(&root, generation, None)
        .await
        .unwrap();

    let after_size = handle
        .file_tag_snapshot_with_reader(&key, reader.clone())
        .await
        .unwrap()
        .1;
    assert_eq!(reader.read_count(), 6);
    assert_eq!(
        after_size
            .files
            .iter()
            .map(|fact| fact.title.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("Track Title 5"), Some("Track Title 6")]
    );
    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn changed_candidate_stamps_replace_the_complete_tag_snapshot() {
    let (manager, tmp) = setup_test_manager().await;
    let (candidate, key, _hash) = picked_candidate(&manager, &tmp).await;
    let handle = manager
        .start_import_service(tokio::runtime::Handle::current())
        .await
        .unwrap();
    let reader = std::sync::Arc::new(CountingFileTagReader::immediate());
    handle
        .file_tag_snapshot_with_reader(&key, reader.clone())
        .await
        .unwrap();

    let root = candidate.watched_folder_path.clone();
    let generation = manager.begin_folder_scan(&root).await.unwrap();
    manager
        .save_folder_scan_item(
            &root,
            generation,
            &crate::import::folder_scanner::ScanItem::Valid(candidate),
        )
        .await
        .unwrap();
    manager
        .finish_folder_scan(&root, generation, None)
        .await
        .unwrap();

    let after_generation = handle
        .file_tag_snapshot_with_reader(&key, reader.clone())
        .await
        .unwrap()
        .1;
    assert_eq!(reader.read_count(), 4);
    assert_eq!(
        after_generation
            .files
            .iter()
            .map(|fact| fact.title.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("Track Title 3"), Some("Track Title 4")]
    );
    shut_down(handle).await;

    let (manager, tmp) = setup_test_manager().await;
    let (_candidate, key, _hash) = picked_candidate(&manager, &tmp).await;
    let handle = manager
        .start_import_service(tokio::runtime::Handle::current())
        .await
        .unwrap();
    let reader = std::sync::Arc::new(CountingFileTagReader::immediate());
    handle
        .file_tag_snapshot_with_reader(&key, reader.clone())
        .await
        .unwrap();

    handle
        .set_file_role(
            key.clone(),
            "02 Track.flac".to_string(),
            crate::import::folder_scanner::FileRoleChoice::NotATrack,
        )
        .await
        .unwrap();

    let after_file_edit = handle
        .file_tag_snapshot_with_reader(&key, reader.clone())
        .await
        .unwrap()
        .1;
    assert_eq!(reader.read_count(), 3);
    assert_eq!(after_file_edit.files.len(), 1);
    assert_eq!(after_file_edit.files[0].title.as_deref(), Some("Track Title 3"));
    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn file_tags_cannot_restore_mappings_read_before_a_file_decision() {
    let (handle, tmp, key, hash) = pane_fixture().await;
    let stale = handle
        .library_manager
        .load_import_candidate_preparation(&hash)
        .await
        .unwrap()
        .expect("the candidate starts prepared");
    handle
        .set_file_role(
            key.clone(),
            "01 Track.flac".to_string(),
            crate::import::folder_scanner::FileRoleChoice::NotATrack,
        )
        .await
        .unwrap();
    let (_, snapshot) = handle
        .file_tag_snapshot_with_reader(
            &key,
            std::sync::Arc::new(CountingFileTagReader::immediate()),
        )
        .await
        .unwrap();
    let root = tmp.path().join("watched").to_string_lossy().into_owned();

    let error = handle
        .library_manager
        .replace_candidate_file_tags_metadata(
            &root,
            &key,
            &hash,
            stale.file_edit_revision,
            stale.metadata_revision,
            &snapshot,
            &stale.metadata_draft,
            &stale.track_mappings,
            stale.cover.as_ref(),
        )
        .await
        .expect_err("the new file snapshot cannot carry mappings from the old file revision");

    assert!(error.to_string().contains("candidate changed"), "{error}");
    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn file_role_changes_leave_complete_physical_mappings_for_the_settled_shape() {
    let (handle, _tmp, key, hash) = pane_fixture().await;

    handle
        .set_file_role(
            key.clone(),
            "02 Track.flac".to_string(),
            crate::import::folder_scanner::FileRoleChoice::NotATrack,
        )
        .await
        .unwrap();
    handle
        .set_file_role(
            key,
            "02 Track.flac".to_string(),
            crate::import::folder_scanner::FileRoleChoice::Audio,
        )
        .await
        .unwrap();

    let preparation = handle
        .library_manager
        .load_import_candidate_preparation(&hash)
        .await
        .unwrap()
        .expect("the candidate remains prepared");
    let active = crate::import::edits::apply_track_mappings_to_draft(
        preparation.metadata_draft,
        &preparation.track_mappings,
    )
    .expect("the settled candidate has one mapping per track");
    assert!(active.tracks.iter().all(|track| track.file.is_some()));

    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn import_refuses_audio_changed_after_the_file_tags_pane_was_read() {
    let (handle, tmp, key, _hash) = pane_fixture().await;
    let root = tmp.path().join("watched").to_string_lossy().into_owned();
    let before = handle
        .library_manager
        .load_candidate_file_tag_snapshot(&root, &key)
        .await
        .unwrap()
        .expect("the candidate remains stored")
        .snapshot
        .expect("the pane read stored a complete snapshot");

    let changed_audio = tmp.path().join("watched/Album/01 Track.flac");
    let audio = std::fs::OpenOptions::new()
        .write(true)
        .open(&changed_audio)
        .unwrap();
    audio
        .set_times(
            std::fs::FileTimes::new().set_modified(
                std::time::SystemTime::now() + std::time::Duration::from_secs(5),
            ),
        )
        .unwrap();

    let mut events = handle.subscribe_events();
    let import_id = handle
        .start_import(&key, crate::import::StorageMode::Local, false)
        .await
        .expect("the prepared candidate enters source validation");
    let error = loop {
        let event = tokio::time::timeout(std::time::Duration::from_secs(10), events.recv())
            .await
            .expect("the import reports its result")
            .expect("the import event stream remains open");
        match event {
            crate::import::handle::ImportEvent::ImportProgress {
                progress:
                    crate::import::ImportProgress::Failed {
                        error,
                        import_id: failed_import_id,
                    },
                ..
            } if failed_import_id == import_id => break error,
            _ => {}
        }
    };
    let after = handle
        .library_manager
        .load_candidate_file_tag_snapshot(&root, &key)
        .await
        .unwrap()
        .expect("the candidate remains stored")
        .snapshot
        .expect("a refused import preserves the pane snapshot");
    shut_down(handle).await;

    assert_eq!(
        after, before,
        "refusing import must not replace the snapshot the pane displayed"
    );
    assert!(
        error.contains("changed after its import candidate was scanned"),
        "source validation names the stale scan: {error}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_scan_that_moves_during_tag_reading_refuses_the_snapshot() {
    let (manager, tmp) = setup_test_manager().await;
    let (candidate, key, _hash) = picked_candidate(&manager, &tmp).await;
    let handle = manager
        .start_import_service(tokio::runtime::Handle::current())
        .await
        .unwrap();
    let (entered_tx, entered_rx) = std::sync::mpsc::sync_channel(1);
    let resume = std::sync::Arc::new(std::sync::Barrier::new(2));
    let reader = std::sync::Arc::new(CountingFileTagReader::blocking(
        entered_tx,
        resume.clone(),
    ));
    let operation = tokio::spawn({
        let handle = handle.clone();
        let key = key.clone();
        async move {
            handle
                .file_tag_snapshot_with_reader(&key, reader)
                .await
        }
    });
    entered_rx
        .recv_timeout(std::time::Duration::from_secs(2))
        .expect("the tag reader reached the first audio file");

    let root = candidate.watched_folder_path.clone();
    let generation = manager.begin_folder_scan(&root).await.unwrap();
    manager
        .save_folder_scan_item(
            &root,
            generation,
            &crate::import::folder_scanner::ScanItem::Valid(candidate),
        )
        .await
        .unwrap();
    manager
        .finish_folder_scan(&root, generation, None)
        .await
        .unwrap();
    resume.wait();

    let error = operation
        .await
        .unwrap()
        .expect_err("the earlier scan stamp cannot land after a newer scan");
    assert!(
        error.to_string().contains("changed while its file tags were being read"),
        "the refusal names the changed candidate: {error}"
    );
    shut_down(handle).await;
}

/// The rows of the table that become tracks, in order — what a person edits.
fn track_rows(table: &crate::import::MappingTable) -> Vec<crate::import::RawTrackEdit> {
    use crate::import::mapping::{MappingBecomes, MappingTrackGroup};
    table
        .track_groups
        .iter()
        .flat_map(MappingTrackGroup::units)
        .filter_map(|unit| match &unit.becomes {
            MappingBecomes::Track { track, .. } => Some(track.clone()),
            _ => None,
        })
        .collect()
}
