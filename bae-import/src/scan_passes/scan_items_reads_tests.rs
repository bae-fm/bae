//! What storing a batch of scan items reads of the store: the same few reads
//! for one item as for many, nothing of an entry the batch keeps as it
//! stands, and an entry that changed after it was read, read again.

use super::*;
use crate::file_tag_snapshot::StoredCover;
use crate::scan_passes::tests::*;
use crate::service::tests::*;
use bae_util::test_channels::HANG_GUARD;

/// The bytes of the picture each track of a pictured root embeds.
const PICTURE_BYTES: usize = 1024 * 1024;

/// Store `items` again under a new generation of `root`, and count the reads
/// of the store it made.
async fn reads_to_store(
    test: &TestService,
    root: &Path,
    items: Vec<ScanItem>,
) -> Result<u64, crate::ImportError> {
    let (scan, _events) = test.scan();
    let services = &scan.services.services;
    let generation = services.tables.begin_folder_scan(root, crate::VolumeKind::Local).await?;
    let reads = Arc::new(crate::folder_scanner::FileReads::new(Arc::new(
        crate::file_tag_reader::LoftyFileTagReader,
    )));
    let before = test.db.reads_asked();
    let stored = store_scan_items(
        root,
        generation,
        items.into_iter().map(FolderEntry::Release).collect(),
        &reads,
        services,
        &mut Vec::new(),
        &mut Vec::new(),
    )
    .await?;
    assert!(stored, "the new generation is the root's");
    Ok(test.db.reads_asked() - before)
}

/// Each kind of read a batch needs — the file decisions, what the store holds
/// of the items' files, and the recheck of the decisions' revisions — is made
/// once for the batch, not once for each item.
#[tokio::test]
async fn a_batch_of_scan_items_reads_the_store_as_often_as_one_item() {
    let test = setup_import_service().await;
    let root = test.temp.path().join("watched");
    for albums in 1..=8 {
        write_disc(&root.join(format!("Album {albums}")), albums);
    }
    let root = crate::tables::watched_path::canonical_absolute_root(&root).unwrap();
    test.tables().add_watched_import_folder(&root).await.unwrap();
    let (scan, _events) = test.scan();
    scan.rescan(&root).await.unwrap();
    let items = test.tables().load_folder_scan_items(&root).await.unwrap();
    assert_eq!(items.len(), 8, "every album was scanned");

    let one = reads_to_store(&test, &root, items[..1].to_vec()).await.unwrap();
    let eight = reads_to_store(&test, &root, items).await.unwrap();

    assert_eq!(eight, one, "a batch of eight reads the store as often as one item");
    assert_eq!(eight, 3, "the decisions, what the files seed, and the revisions' recheck");
}

/// Reads each track's tags from the file, and answers as if every track
/// embedded a picture of `bytes` bytes: what a library of tagged albums
/// stores for each release.
struct PictureTagReader {
    bytes: usize,
}

impl crate::file_tag_reader::FileTagReader for PictureTagReader {
    fn read(
        &self,
        path: &Path,
    ) -> Result<crate::file_tag_reader::FileTagRead, crate::file_tag_reader::TagReadFailure> {
        let mut read = crate::file_tag_reader::LoftyFileTagReader.read(path)?;
        read.embedded_cover =
            Some((vec![0xD8; self.bytes], bae_util::content_type::ContentType::Jpeg));
        Ok(read)
    }
}

/// What a pass over a watched root of `RELEASES` releases nothing in has
/// changed costs, each release's tracks embedding a picture of
/// [`PICTURE_BYTES`]: the time it takes and the statements it runs. A
/// measurement, not a check. Run it in release with
///
/// ```sh
/// scripts/cargo.sh test --release -p bae-import --features test-utils --lib -- \
///     unchanged_rescan_cost --ignored --nocapture --test-threads 1
/// ```
#[tokio::test(flavor = "multi_thread")]
#[ignore = "measurement: run in release with --ignored --nocapture"]
async fn unchanged_rescan_cost() {
    const RELEASES: usize = 200;
    const RUNS: usize = 5;
    let test = setup_import_service().await;
    let (root, scan) = scanned_pictured_root(&test, RELEASES).await;
    let counter = test.db.track_work_for_test().await.unwrap();
    let mut times = Vec::new();
    for _ in 0..RUNS {
        counter.take();
        let started = std::time::Instant::now();
        scan.rescan(&root).await.unwrap();
        times.push(started.elapsed());
        let work = counter.take();
        eprintln!(
            "UNCHANGED_RESCAN releases={RELEASES} ms={:.1} read_statements={} read_steps={} \
             write_steps={} commits={}",
            started.elapsed().as_secs_f64() * 1e3,
            work.app_read_statements,
            work.app_read_steps,
            work.app_write_steps,
            work.commits,
        );
    }
    times.sort();
    eprintln!("UNCHANGED_RESCAN median_ms={:.1}", times[RUNS / 2].as_secs_f64() * 1e3);
}

/// A watched root of `releases` releases of two tracks each, every track
/// embedding a picture of `PICTURE_BYTES`, scanned once; and the scan.
async fn scanned_pictured_root(test: &TestService, releases: usize) -> (PathBuf, TestScan) {
    let root = test.temp.path().join("watched");
    for release in 0..releases {
        write_disc(&root.join(format!("Album {release:04}")), 2);
    }
    let root = crate::tables::watched_path::canonical_absolute_root(&root).unwrap();
    test.tables().add_watched_import_folder(&root).await.unwrap();
    let (scan, _events) = test.scan_with(
        Arc::new(PictureTagReader { bytes: PICTURE_BYTES }),
        Arc::new(crate::folder_scanner::OsDirectoryReader),
    );
    scan.rescan(&root).await.unwrap();
    (root, scan)
}

/// The bytes of the pictures `readings` hold.
fn picture_bytes(readings: &[crate::tables::ScanItemReadings]) -> usize {
    readings
        .iter()
        .map(|readings| match readings {
            crate::tables::ScanItemReadings::Stored(Some(stored)) => match &stored.cover {
                StoredCover::Picture(cover) => cover.data.len(),
                StoredCover::Absent | StoredCover::Replaced { .. } => 0,
            },
            _ => 0,
        })
        .sum()
}

/// A pass over releases nothing in has changed seeds none of them and loads
/// none of the pictures their stored readings hold: each entry is already
/// what the pass read, so its write keeps it. An entry known to have changed
/// since is read for its seed, picture and all.
#[tokio::test]
async fn an_unchanged_release_is_not_seeded_and_its_picture_is_not_loaded() {
    let test = setup_import_service().await;
    let (root, scan) = scanned_pictured_root(&test, 3).await;
    let services = &scan.services.services;
    let tables = &services.tables;
    let items = tables.load_folder_scan_items(&root).await.unwrap();
    assert_eq!(items.len(), 3, "every release was scanned");
    let generation = tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    let reads = Arc::new(tables.load_file_reads(&root, services.file_tags.clone()).await.unwrap());

    let readings = tables.load_scan_batch_readings(&items, &HashSet::new()).await.unwrap();
    let seeds = scan_item_seeds(
        tables,
        &services.diagnostics,
        &items,
        generation,
        reads.clone(),
        &HashSet::new(),
    )
    .await
    .unwrap();

    assert_eq!(picture_bytes(&readings), 0, "no stored picture is loaded");
    let seeded =
        seeds.iter().filter(|seed| !matches!(seed, crate::tables::ScanItemSeed::Unchanged)).count();
    assert_eq!(seeded, 0, "no release is seeded");

    let moved = HashSet::from([items[0].key()]);
    let readings = tables.load_scan_batch_readings(&items, &moved).await.unwrap();
    assert_eq!(picture_bytes(&readings), PICTURE_BYTES, "the moved entry's picture is loaded");
}

/// A release whose cover's source track changed is read from that track on
/// disk, so the picture its stored reading holds is not loaded; a release
/// whose other track changed still takes the stored picture. The stored result
/// has the picture the changed track embeds now.
#[tokio::test]
async fn a_changed_cover_source_track_does_not_load_the_stored_picture() {
    const NEW_PICTURE_BYTES: usize = 2048;
    let test = setup_import_service().await;
    let (root, scan) = scanned_pictured_root(&test, 1).await;
    let services = &scan.services.services;
    let tables = &services.tables;
    let items = tables.load_folder_scan_items(&root).await.unwrap();
    let [ScanItem::Valid(candidate)] = items.as_slice() else {
        panic!("one valid release was scanned: {items:?}")
    };
    // Every track embeds a picture, so the first track is the cover's source.
    let with_track_changed = |track: usize| {
        let mut changed = candidate.clone();
        let audio = changed
            .files
            .files
            .iter_mut()
            .filter(|entry| matches!(entry.role, crate::folder_scanner::FileRole::Audio))
            .nth(track)
            .expect("the release has the track");
        audio.file.version.modified_at_ns += 1;
        vec![ScanItem::Valid(changed)]
    };

    let source_changed =
        tables.load_scan_batch_readings(&with_track_changed(0), &HashSet::new()).await.unwrap();
    assert_eq!(picture_bytes(&source_changed), 0, "the changed source's picture is not loaded");
    let other_changed =
        tables.load_scan_batch_readings(&with_track_changed(1), &HashSet::new()).await.unwrap();
    assert_eq!(
        picture_bytes(&other_changed),
        PICTURE_BYTES,
        "the picture of a source that did not change is loaded"
    );

    let source = std::fs::File::options().write(true).open(candidate.path.join("01.flac")).unwrap();
    source.set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(60)).unwrap();
    drop(source);
    let (scan, _events) = test.scan_with(
        Arc::new(PictureTagReader { bytes: NEW_PICTURE_BYTES }),
        Arc::new(crate::folder_scanner::OsDirectoryReader),
    );
    scan.rescan(&root).await.unwrap();
    let snapshot = tables
        .load_candidate_file_tag_snapshot(&items[0].key())
        .await
        .unwrap()
        .expect("the release is stored")
        .snapshot
        .expect("its reading is stored");
    assert_eq!(
        snapshot.embedded_cover.map(|cover| cover.data.len()),
        Some(NEW_PICTURE_BYTES),
        "the picture is the changed track's, read from disk"
    );
}

/// An entry that changes after the pass read it as unchanged, before the
/// pass's write, refuses that write: the pass reads the entry again, seeds
/// the release from its tracks' stored reading, and stores it as it read it.
#[tokio::test]
async fn an_entry_changed_between_the_read_and_the_write_is_read_again() {
    let test = setup_import_service().await;
    let (root, scan) = scanned_pictured_root(&test, 1).await;
    let services = &scan.services.services;
    let tables = &services.tables;
    let items = tables.load_folder_scan_items(&root).await.unwrap();
    let [item] = items.as_slice() else { panic!("one release was scanned: {items:?}") };
    let ScanItem::Valid(candidate) = item else { panic!("the release is valid: {item:?}") };
    let renamed = ScanItem::Valid(crate::folder_scanner::FolderCandidate {
        name: "Another Name".to_string(),
        ..candidate.clone()
    });
    let generation = tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    let reads = Arc::new(tables.load_file_reads(&root, services.file_tags.clone()).await.unwrap());
    let held = services.folder_state_commit.lock("hold the pass's write back").await;

    let mut written = Vec::new();
    let mut displaced = Vec::new();
    let pass = store_scan_items(
        &root,
        generation,
        vec![FolderEntry::Release(item.clone())],
        &reads,
        services,
        &mut written,
        &mut displaced,
    );
    let meddle = async {
        // The pass has read and seeded its batch, and waits to write it.
        services.folder_state_commit.waiting_reaches(1).await;
        tables
            .save_folder_scan_item(&root, generation, &renamed)
            .await
            .unwrap()
            .expect("the root's generation takes the write");
        drop(held);
    };
    let (stored, ()) = tokio::join!(pass, meddle);

    assert!(stored.unwrap(), "the pass stores its batch");
    assert_eq!(tables.load_folder_scan_items(&root).await.unwrap(), vec![item.clone()]);
    assert_eq!(written, vec![item.relative_folder().clone()], "the release was written again");
    let snapshot = tables
        .load_candidate_file_tag_snapshot(&item.key())
        .await
        .unwrap()
        .expect("the release is stored")
        .snapshot
        .expect("its reading is stored");
    assert_eq!(snapshot.scan_generation, generation, "the reading is the pass's");
    assert_eq!(
        snapshot.embedded_cover.map(|cover| cover.data.len()),
        Some(PICTURE_BYTES),
        "the reading keeps its picture"
    );
}

/// An entry a write before it in its own batch replaces was read as
/// unchanged, and the store before that write still holds it as it was: the
/// pass seeds it the next time rather than read it as unchanged again, and
/// stores the batch.
#[tokio::test]
async fn an_entry_an_earlier_write_of_its_batch_replaces_is_seeded_and_stored() {
    let test = setup_import_service().await;
    let root = test.temp.path().join("watched");
    // The candidates' one bound file each, which seeding reads.
    write_disc(&root.join("Album"), 1);
    write_disc(&root.join("Album").join("CD1"), 1);
    let root = crate::tables::watched_path::canonical_absolute_root(&root).unwrap();
    let tables = test.tables().clone();
    tables.add_watched_import_folder(&root).await.unwrap();
    // The album read whole reads the disc's files, and the disc reads its
    // own: each write of one replaces the other.
    let on_disk = |name: &str| {
        let mut candidate = crate::tables::tests::candidate(&root, name);
        for file in &mut candidate.files.files {
            let metadata = std::fs::metadata(&file.file.version.path).unwrap();
            file.file.version.size = metadata.len();
            file.file.version.modified_at_ns =
                crate::folder_scanner::file_modified_at_ns(&file.file.version.path, &metadata)
                    .unwrap();
        }
        ScanItem::Valid(candidate)
    };
    let album = on_disk("Album");
    let disc = on_disk("Album/CD1");
    let generation = tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    tables.save_folder_scan_item(&root, generation, &disc).await.unwrap().unwrap();
    let (scan, _events) = test.scan();
    let services = &scan.services.services;
    let generation = tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    let reads = Arc::new(crate::folder_scanner::FileReads::new(Arc::new(
        crate::file_tag_reader::LoftyFileTagReader,
    )));

    let stored = tokio::time::timeout(
        HANG_GUARD,
        store_scan_items(
            &root,
            generation,
            vec![FolderEntry::Release(album), FolderEntry::Release(disc.clone())],
            &reads,
            services,
            &mut Vec::new(),
            &mut Vec::new(),
        ),
    )
    .await
    .expect("the pass stores the batch rather than read the disc as unchanged again");

    assert!(stored.unwrap(), "the pass stores its batch");
    assert_eq!(tables.load_folder_scan_items(&root).await.unwrap(), vec![disc]);
}

/// Reads tags as a share that went away does: every read fails at the file
/// itself.
struct VanishedShareTagReader;

impl crate::file_tag_reader::FileTagReader for VanishedShareTagReader {
    fn read(
        &self,
        path: &Path,
    ) -> Result<crate::file_tag_reader::FileTagRead, crate::file_tag_reader::TagReadFailure> {
        Err(crate::file_tag_reader::TagReadFailure::Fault(format!(
            "failed to read tags from {}: the share went away",
            path.display()
        )))
    }
}

/// A folder whose tags can't be read when its draft is seeded fails, rather
/// than being stored with a blank draft that no later pass reads again: its
/// scan fails, and the next one seeds it.
#[tokio::test]
async fn a_folder_whose_tags_cannot_be_read_for_its_draft_fails_its_seed() {
    let test = setup_import_service().await;
    let root = test.temp.path().join("watched");
    write_disc(&root.join("Album"), 1);
    let root = crate::tables::watched_path::canonical_absolute_root(&root).unwrap();
    test.tables().add_watched_import_folder(&root).await.unwrap();
    let mut found = Vec::new();
    crate::folder_scanner::scan_for_candidates_with_decisions(
        root.clone(),
        &crate::folder_scanner::StoredCandidateEdits::none(),
        &crate::folder_scanner::FolderReleaseDecisions::default(),
        &coven::SequentialIds::new(),
        |item| {
            if let crate::folder_scanner::WalkItem::Entry(FolderEntry::Release(item)) = item {
                found.push(item);
            }
        },
    )
    .unwrap();
    // The walk finds the release tentative, then settled.
    let item = found.last().expect("the release was found");
    let generation =
        test.tables().begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();

    let seed = scan_item_seed(
        test.tables(),
        &bae_diagnostics::Diagnostics::noop(),
        item,
        generation,
        Arc::new(crate::folder_scanner::FileReads::new(Arc::new(VanishedShareTagReader))),
        &HashSet::new(),
    )
    .await;

    let Err(error) = seed else { panic!("the unread tags fail the seed") };
    assert!(error.to_string().contains("the share went away"), "{error}");
}
