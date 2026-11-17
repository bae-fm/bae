//! A candidate's folder images as its lightbox shows them: the image asked
//! for, the first when the folder holds no image by that id, and the images
//! before and after it.

use super::*;
use crate::folder_scanner::{CandidateFile, CategorizedFiles, FileRole, ScanItem, ScannedFile};
use crate::tables::tests::*;
use crate::Neighbors;
use std::time::Duration;

/// A scanned candidate whose folder holds `images` beside one audio file.
async fn folder_with(images: &[&str]) -> (ImportTables, tempfile::TempDir, crate::CandidateKey) {
    let (db, tmp, root) = watched_root().await;
    let tables = ImportTables::new(db);
    let folder = folder_below(&root, "Album");
    let mut files = vec![CandidateFile {
        file: ScannedFile::new(folder.join("01.flac"), "01.flac".to_string(), 1_000, 1)
            .with_test_flac_audio(),
        role: FileRole::Audio,
    }];
    files.extend(images.iter().map(|name| CandidateFile {
        file: ScannedFile::new(folder.join(name), name.to_string(), 1_000, 1),
        role: FileRole::Artwork,
    }));
    let candidate = candidate_with(
        &root,
        "Album",
        CategorizedFiles { files, parts: Vec::new() },
        crate::folder_scanner::ReleaseFileScope::Recursive,
    );
    tables.add_watched_import_folder(&root).await.unwrap();
    let generation = tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    tables
        .save_folder_scan_item(&root, generation, &ScanItem::Valid(candidate.clone()))
        .await
        .unwrap();
    tables.finish_folder_scan(&root, generation, None).await.unwrap();
    (tables, tmp, candidate.key())
}

/// The images the lightbox of `key` shows now, asked to show `focus`.
async fn read(
    tables: &ImportTables,
    key: &crate::CandidateKey,
    focus: Option<&str>,
) -> FolderImages {
    let asked = FolderImagesRequest { candidate: key.clone(), focus: focus.map(str::to_string) };
    let mut live = tables.subscribe_folder_images(Some(asked.clone()));
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let event = live.next_inline().await.expect("the read runs");
            if event.request.as_ref() == Some(&asked) {
                break event.result.unwrap().expect("the live query answers the request");
            }
        }
    })
    .await
    .expect("the read delivered the images asked for")
}

/// The shown image's file id and the file ids before and after it.
fn shown(images: &FolderImages) -> (String, Option<(String, String)>) {
    let focus = images.focus.clone().expect("a folder with images shows one");
    (focus.image.file_id, focus.neighbors.map(|Neighbors { previous, next }| (previous, next)))
}

/// The lightbox shows the image asked for, between the images before and
/// after it, and wraps around the ends of the folder's images.
#[tokio::test]
async fn the_lightbox_shows_the_image_asked_for_between_its_neighbors() {
    let (tables, _tmp, key) = folder_with(&["back.jpg", "cover.jpg", "inlay.png"]).await;
    let middle = read(&tables, &key, Some("cover.jpg")).await;
    let last = read(&tables, &key, Some("inlay.png")).await;

    assert_eq!(
        middle.images.iter().map(|image| image.file_id.as_str()).collect::<Vec<_>>(),
        ["back.jpg", "cover.jpg", "inlay.png"]
    );
    assert_eq!(
        shown(&middle),
        ("cover.jpg".to_string(), Some(("back.jpg".to_string(), "inlay.png".to_string())))
    );
    assert_eq!(
        shown(&last),
        ("inlay.png".to_string(), Some(("cover.jpg".to_string(), "back.jpg".to_string())))
    );
}

/// The lightbox shows the first image when the folder holds none by the id
/// asked for, a lone image has nothing before or after it, and a folder with
/// no image shows none.
#[tokio::test]
async fn the_lightbox_falls_back_to_the_first_image_and_a_lone_one_has_no_neighbors() {
    let (tables, _tmp, key) = folder_with(&["back.jpg", "cover.jpg"]).await;
    assert_eq!(
        shown(&read(&tables, &key, Some("gone.jpg")).await),
        ("back.jpg".to_string(), Some(("cover.jpg".to_string(), "cover.jpg".to_string())))
    );

    let (tables, _tmp, key) = folder_with(&["cover.jpg"]).await;
    assert_eq!(shown(&read(&tables, &key, None).await), ("cover.jpg".to_string(), None));

    let (tables, _tmp, key) = folder_with(&[]).await;
    let empty = read(&tables, &key, None).await;
    assert_eq!((empty.images.len(), empty.focus), (0, None));
}
