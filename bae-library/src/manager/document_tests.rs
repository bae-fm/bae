//! Reading a library release's documents: the text of a document or a track
//! sheet, decoded, and a refusal for any other file or one the release no
//! longer holds.

use super::*;
use crate::manager::tests::*;
use bae_records::{NewReleaseFile, ReleaseFileRole};
use bae_util::content_type::ContentType;
use chrono::Utc;

/// Seed `release_id`'s file `name` in `role`, holding `bytes`, as an import
/// stages it on this device.
async fn seed(
    manager: &LibraryManager,
    dir: &std::path::Path,
    release_id: &str,
    name: &str,
    role: ReleaseFileRole,
    bytes: &[u8],
) -> String {
    let file = NewReleaseFile::new(
        release_id,
        name,
        ContentType::PlainText,
        role,
        coven::IdSource::new_id(&coven::UuidIds).to_string(),
        Utc::now(),
    );
    let path = dir.join(name);
    std::fs::write(&path, bytes).unwrap();
    manager.add_external_file_for_test(&file, &path).await.unwrap();
    file.id
}

#[tokio::test]
async fn a_document_written_in_latin1_reads_decoded() {
    let (test, temp_dir, _album, release) = manager_with_release().await;
    let manager = test.library.manager();
    let log = seed(
        manager,
        temp_dir.path(),
        &release.id,
        "rip.log",
        ReleaseFileRole::Document(bae_records::DocumentKind::RipLog),
        b"Caf\xE9",
    )
    .await;
    let sheet = seed(
        manager,
        temp_dir.path(),
        &release.id,
        "disc.cue",
        ReleaseFileRole::TrackSheet,
        b"FILE \"disc.flac\" WAVE",
    )
    .await;

    assert_eq!(manager.read_release_document(&release.id, &log).await.unwrap(), "Café");
    assert_eq!(
        manager.read_release_document(&release.id, &sheet).await.unwrap(),
        "FILE \"disc.flac\" WAVE"
    );
}

#[tokio::test]
async fn a_file_that_is_no_document_or_not_the_release_s_is_refused() {
    let (test, temp_dir, _album, release) = manager_with_release().await;
    let manager = test.library.manager();
    let nfo =
        seed(manager, temp_dir.path(), &release.id, "notes.nfo", ReleaseFileRole::Other, b"notes")
            .await;

    let other = manager.read_release_document(&release.id, &nfo).await;
    let missing = manager.read_release_document(&release.id, "absent-file-id").await;

    assert!(matches!(other, Err(LibraryError::Internal(_))), "{other:?}");
    assert!(matches!(missing, Err(LibraryError::FileMissing { .. })), "{missing:?}");
}
