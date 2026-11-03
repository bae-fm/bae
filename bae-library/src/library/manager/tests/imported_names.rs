//! Names a folder's files carry on Windows, imported into the library and
//! exported from it.

use super::*;
use std::os::windows::ffi::OsStringExt;

/// What the import `import_id` reported: the release it created, or the
/// error it failed with.
async fn import_outcome(
    events: &mut tokio::sync::mpsc::UnboundedReceiver<bae_import::ImportEvent>,
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
    let (manager, temp) = setup_test_manager().await;
    let root = temp.path().join("watched");
    let folder = root.join("Album");
    std::fs::create_dir_all(&folder).unwrap();
    let flac =
        std::fs::read(bae_import::test_fixtures::flac_dir().join("01 Test Track 1.flac")).unwrap();
    let name: Vec<u16> =
        "Caf".encode_utf16().chain([0xD800]).chain(".flac".encode_utf16()).collect();
    std::fs::write(folder.join(std::ffi::OsString::from_wide(&name)), &flac).unwrap();
    let import = manager.start_import().await.unwrap();
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
            vec![bae_import::Credited::plain(bae_import::ArtistAssignment::named("Artist Name"))],
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
    assert_eq!(std::fs::read(target.join("Album").join("Caf＼xED＼xA0＼x80.flac")).unwrap(), flac);
    tokio::task::spawn_blocking(move || import.stop_and_join()).await.unwrap();
}
