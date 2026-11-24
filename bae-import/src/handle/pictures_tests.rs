//! Reading a candidate's pictures: a copy at the step a request covers, kept
//! with its candidate and dropped with it, one read of the file for requests
//! that share a picture, and a refusal for a file the import list does not
//! hold.

use crate::handle::tests::*;
use crate::tests::reads::{self, FileRead};
use crate::CoverImageSource;
use bae_pictures::Priority;

/// `fixture`'s folder holding a GIF named `name`, stored as a scan reads it,
/// and the picture that names it.
async fn with_image(
    fixture: &mut StoredCandidate,
    name: &str,
) -> (CoverImageSource, std::path::PathBuf) {
    fixture.tmp.write(fixture.candidate.path.join(name), bae_util::test_cover_art::SOLID_GIF);
    fixture.candidate.files = crate::folder_scanner::collect_release_candidate_files_with_scope(
        &fixture.candidate.path,
        fixture.candidate.scope,
        &crate::folder_scanner::StoredCandidateEdits::none(),
    )
    .unwrap();
    rescan_into(&fixture.handle.services.tables, fixture.candidate.clone()).await;
    let version = fixture
        .candidate
        .files
        .files
        .iter()
        .find(|entry| entry.file.relative_path == name)
        .expect("the scan reads the image")
        .file
        .version
        .clone();
    let path = version.path.clone();
    (CoverImageSource::Local { file: version }, path)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_picture_is_made_at_the_step_that_covers_the_request_and_read_once() {
    let mut fixture = stored_candidate().await;
    let (picture, path) = with_image(&mut fixture, "cover.gif").await;

    let first = fixture.handle.read_picture(picture.clone(), Some(100), Priority::Visible).await;
    let second = fixture.handle.read_picture(picture.clone(), Some(150), Priority::Visible).await;
    let (first, second) = (first.unwrap(), second.unwrap());
    let reads = reads::count(FileRead::PictureOriginal, &path);
    let owner = fixture.handle.services.tables.picture_owner(&picture).await.unwrap().unwrap();
    let kept = fixture.handle.services.tables.picture_copy_count(&owner).await.unwrap();
    shut_down(fixture.handle).await;

    // A GIF is not transparent here: a JPEG, which both requests (100 and 150
    // pixels, the 192 step) read from the one copy.
    assert_eq!(&first[..2], &[0xFF, 0xD8], "a JPEG");
    assert_eq!(first, second);
    assert_eq!(reads, 1, "the file was read once");
    assert_eq!(kept, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn two_requests_for_one_picture_and_size_read_the_file_once() {
    let mut fixture = stored_candidate().await;
    let (picture, path) = with_image(&mut fixture, "cover.gif").await;

    let (first, second) = tokio::join!(
        fixture.handle.read_picture(picture.clone(), Some(300), Priority::Visible),
        fixture.handle.read_picture(picture.clone(), Some(300), Priority::Prefetch),
    );
    let reads = reads::count(FileRead::PictureOriginal, &path);
    shut_down(fixture.handle).await;

    assert_eq!(first.unwrap(), second.unwrap());
    assert_eq!(reads, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_original_comes_back_as_the_files_own_bytes() {
    let mut fixture = stored_candidate().await;
    let (picture, _) = with_image(&mut fixture, "cover.gif").await;

    let original = fixture.handle.read_picture(picture, None, Priority::Visible).await;
    shut_down(fixture.handle).await;

    assert_eq!(original.unwrap(), bae_util::test_cover_art::SOLID_GIF);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_candidate_set_aside_keeps_no_copies() {
    let mut fixture = stored_candidate().await;
    let (picture, _) = with_image(&mut fixture, "cover.gif").await;
    fixture.handle.read_picture(picture.clone(), Some(100), Priority::Visible).await.unwrap();
    let tables = &fixture.handle.services.tables;
    let owner = tables.picture_owner(&picture).await.unwrap().unwrap();
    assert_eq!(tables.picture_copy_count(&owner).await.unwrap(), 1);

    tables.set_import_candidate_skipped(&fixture.candidate.path, true).await.unwrap();
    let kept = tables.picture_copy_count(&owner).await.unwrap();
    shut_down(fixture.handle).await;

    assert_eq!(kept, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_file_the_import_list_does_not_hold_is_refused() {
    let fixture = stored_candidate().await;
    let stray = fixture.tmp.write("stray.gif", bae_util::test_cover_art::SOLID_GIF);
    let picture = CoverImageSource::Local {
        file: crate::folder_scanner::FileVersion { path: stray, size: 51, modified_at_ns: 1 },
    };

    let refused = fixture.handle.read_picture(picture, Some(100), Priority::Visible).await;
    shut_down(fixture.handle).await;

    assert!(
        matches!(&refused, Err(crate::ImportError::Picture(error)) if matches!(**error, crate::PictureError::NotInImport(_))),
        "{refused:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_imported_candidate_keeps_no_copies() {
    let mut fixture = stored_candidate().await;
    let (picture, _) = with_image(&mut fixture, "cover.gif").await;
    let handle = &fixture.handle;
    handle.read_picture(picture.clone(), Some(100), Priority::Visible).await.unwrap();
    let owner = handle.services.tables.picture_owner(&picture).await.unwrap().unwrap();
    assert_eq!(handle.services.tables.picture_copy_count(&owner).await.unwrap(), 1);

    handle.select_candidate_file_tags(fixture.key.clone()).await.unwrap();
    handle
        .set_candidate_album_artists(
            &fixture.key,
            vec![crate::Credited::plain(crate::ArtistAssignment::named("Typed Artist"))],
        )
        .await
        .unwrap();
    let mut events = handle.every_event();
    let import_id = handle.start_import(&fixture.key).await.unwrap().expect_queued();
    await_import_outcome(&mut events, &import_id).await.expect("the import completes");
    let kept = handle.services.tables.picture_copy_count(&owner).await.unwrap();
    shut_down(fixture.handle).await;

    assert_eq!(kept, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_file_replaced_after_its_scan_makes_no_copy() {
    let mut fixture = stored_candidate().await;
    let (picture, path) = with_image(&mut fixture, "cover.gif").await;
    // The person saves another picture over the file after the scan read it.
    fixture.tmp.write(&path, bae_util::test_cover_art::SOLID_WEBP);

    let refused = fixture.handle.read_picture(picture.clone(), Some(100), Priority::Visible).await;
    let owner = fixture.handle.services.tables.picture_owner(&picture).await.unwrap().unwrap();
    let kept = fixture.handle.services.tables.picture_copy_count(&owner).await.unwrap();
    shut_down(fixture.handle).await;

    assert!(
        matches!(&refused, Err(crate::ImportError::Picture(error)) if matches!(**error, crate::PictureError::ChangedSinceScan(_))),
        "{refused:?}"
    );
    assert_eq!(kept, 0, "a copy of the new file would be kept under the old file's version");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_file_touched_after_its_scan_makes_no_copy() {
    let mut fixture = stored_candidate().await;
    let (picture, path) = with_image(&mut fixture, "cover.gif").await;
    fixture
        .tmp
        .open_with(&path, std::fs::OpenOptions::new().write(true))
        .set_modified(std::time::SystemTime::now())
        .expect("set the file's modification time");

    let refused = fixture.handle.read_picture(picture, Some(100), Priority::Visible).await;
    shut_down(fixture.handle).await;

    assert!(
        matches!(&refused, Err(crate::ImportError::Picture(error)) if matches!(**error, crate::PictureError::ChangedSinceScan(_))),
        "{refused:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_catalog_image_is_no_picture_of_the_import_list() {
    let fixture = stored_candidate().await;
    let catalog = CoverImageSource::Remote {
        image: bae_catalogs::remote_image::RemoteImageSet {
            url: "https://images.example/front.jpg".to_string(),
            downscaled: Vec::new(),
        },
    };

    let refused = fixture.handle.read_picture(catalog, Some(100), Priority::Visible).await;
    shut_down(fixture.handle).await;

    assert!(
        matches!(&refused, Err(crate::ImportError::Picture(error)) if matches!(**error, crate::PictureError::NotAnImportPicture)),
        "{refused:?}"
    );
}
