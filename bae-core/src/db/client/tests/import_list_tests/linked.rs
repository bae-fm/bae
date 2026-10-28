//! A linked row as the list reads it: from the tables alone, whatever the
//! folder's files are doing.

use super::super::super::*;
use super::super::{candidate_with, folder_below};
use super::{musicbrainz_release, request, rows};
use crate::import::folder_scanner::{
    CandidateFile, CategorizedFiles, FileRole, ReleaseFileScope, ScanItem, ScannedFile,
};
use crate::import::TriageTab;
use std::os::unix::fs::OpenOptionsExt;
use std::time::Duration;

/// A row linked to a pressing is read from the tables: the list opens none of
/// the folder's files. Its rip log here is a pipe nobody writes, which holds
/// whoever opens it the way a volume that stops answering does, so a list
/// that read the log would never answer.
#[tokio::test]
async fn a_linked_row_is_read_without_opening_its_folders_files() {
    let (db, _tmp, root) = super::super::watched_root().await;
    let folder = folder_below(&root, "Album");
    std::fs::create_dir_all(&folder).unwrap();
    let log = folder.join("rip.log");
    let path = std::ffi::CString::new(log.to_str().unwrap()).unwrap();
    // SAFETY: `path` is a NUL-terminated string that outlives the call.
    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
    let candidate = candidate_with(
        &root,
        "Album",
        CategorizedFiles {
            files: vec![
                CandidateFile {
                    file: ScannedFile::new(folder.join("01.flac"), "01.flac".into(), 1_000, 1)
                        .with_test_flac_audio(),
                    role: FileRole::Audio,
                },
                CandidateFile {
                    file: ScannedFile::new(log.clone(), "rip.log".into(), 1_000, 1),
                    role: FileRole::Document,
                },
            ],
            parts: Vec::new(),
        },
        ReleaseFileScope::Recursive,
    );
    db.add_watched_import_folder(&root).await.unwrap();
    let generation = db
        .begin_folder_scan(&root, crate::import::VolumeKind::Local)
        .await
        .unwrap();
    db.save_folder_scan_item(&root, generation, &ScanItem::Valid(candidate.clone()))
        .await
        .unwrap();
    db.finish_folder_scan(&root, generation, None)
        .await
        .unwrap();
    let record = crate::import::MetadataRef::new(Catalog::MusicBrainz, "mb-picked");
    db.save_source_release(
        &crate::import::payloads::ReleasePayloads::for_test(
            record.clone(),
            musicbrainz_release("mb-picked", "Picked Album").to_string(),
            Vec::new(),
        )
        .extract()
        .unwrap(),
    )
    .await
    .unwrap();
    crate::import::CandidatePreparations::new(db.clone())
        .replace_link(
            &candidate.files.content_hash(),
            Some(&crate::import::ReleaseLink::Pressing(
                crate::import::PressingLink {
                    record,
                    partners: vec![],
                },
            )),
        )
        .await
        .unwrap();

    let read = tokio::time::timeout(
        Duration::from_secs(10),
        db.load_import_list_read(request(TriageTab::Pending).await),
    )
    .await;
    if read.is_err() {
        // Let the read held on the pipe go: a writer opening and closing it
        // ends it.
        drop(
            std::fs::OpenOptions::new()
                .write(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(&log),
        );
    }
    let projection = read
        .expect("the list read waited on the folder's rip log")
        .unwrap();
    assert_eq!(
        rows(&projection)[0].release_link,
        Some(crate::import::ReleaseLink::Pressing(
            crate::import::PressingLink {
                record: crate::import::MetadataRef::new(Catalog::MusicBrainz, "mb-picked"),
                partners: vec![],
            },
        )),
        "the row reads its link from the tables"
    );
}
