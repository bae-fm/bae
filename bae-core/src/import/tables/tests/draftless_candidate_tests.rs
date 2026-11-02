//! A candidate this device did not import that holds no draft row, as a
//! folder whose release was deleted does: what its row and its pane read,
//! and what its first save writes.

use super::import_list_live_query_tests::list_request;
use super::*;

/// A folder of one bound FLAC and a front cover image.
fn with_cover(root: &std::path::Path, name: &str) -> FolderCandidate {
    let folder = folder_below(root, name);
    candidate_with(
        root,
        name,
        CategorizedFiles {
            files: vec![
                CandidateFile {
                    file: ScannedFile::new(folder.join("01.flac"), "01.flac".to_string(), 1_000, 1)
                        .with_test_flac_audio(),
                    role: FileRole::Audio,
                },
                CandidateFile {
                    file: ScannedFile::new(
                        folder.join("cover.jpg"),
                        "cover.jpg".to_string(),
                        500,
                        1,
                    ),
                    role: FileRole::Artwork,
                },
            ],
            parts: Vec::new(),
        },
        ReleaseFileScope::Recursive,
    )
}

/// With no draft row a candidate reads as its files' blank draft and the
/// folder's own cover, on its row and in its pane, and its first save stores
/// the draft and that cover.
#[tokio::test]
async fn a_candidate_holding_no_draft_row_reads_its_blank_draft_and_folder_cover() {
    let (db, _tmp, root) = watched_root().await;
    let tables = crate::import::tables::ImportTables::new(db.clone());
    tables.add_watched_import_folder(&root).await.unwrap();
    let candidate = with_cover(&root, "Album");
    let key = candidate.key();
    let hash = candidate.files.content_hash();
    let generation =
        tables.begin_folder_scan(&root, crate::import::VolumeKind::Local).await.unwrap();
    tables
        .save_folder_scan_item(
            &root,
            generation,
            &crate::import::folder_scanner::ScanItem::Valid(candidate.clone()),
        )
        .await
        .unwrap();
    tables.finish_folder_scan(&root, generation, None).await.unwrap();
    let folder_cover = crate::import::CoverSelection::Local("cover.jpg".to_string());
    assert_eq!(candidate.blank_cover(), Some(folder_cover.clone()));
    // As an import deletes them, with no release recorded: the folder is
    // back in Found.
    exec(&db, "DELETE FROM import_candidate_edit WHERE content_hash = ?1", &[&hash]).await;
    exec(&db, "DELETE FROM import_candidate_cover WHERE content_hash = ?1", &[&hash]).await;

    let listed = tables
        .load_import_list(list_request(&db, crate::import::TriageTab::Pending, [(0, 50)]).await)
        .await
        .unwrap();
    let row = listed
        .windows
        .iter()
        .flat_map(|window| &window.items)
        .find_map(|item| match item {
            crate::import::ImportListItem::Candidate { row, .. } => Some(row.clone()),
            _ => None,
        })
        .expect("the folder is listed");
    assert_eq!(row.metadata_summary, None, "it shows its folder's name");
    assert_eq!(
        row.cover,
        Some(crate::import::CoverImageSource::Local {
            file: candidate.files.files[1].file.version(),
        })
    );

    let pane =
        tables.load_import_candidate(&key).await.unwrap().expect("the folder has a pane").resolve();
    let draft = pane.draft().expect("a folder nobody imported holds a draft");
    assert_eq!(draft.metadata_draft, candidate.blank_draft().release_edit());
    assert_eq!(draft.metadata_author, crate::import::MetadataAuthor::Nobody);
    assert_eq!(
        draft.cover.as_ref().map(|cover| cover.selection.clone()),
        Some(folder_cover.clone())
    );

    crate::import::CandidatePreparations::new(db.clone())
        .set_field(&hash, crate::import::CandidateEditField::AlbumTitle, "Typed album")
        .await
        .unwrap();
    let stored = tables.load_candidate_preparation(&hash).await.unwrap().unwrap().stored().unwrap();
    assert_eq!(stored.metadata.draft.album_title, "Typed album");
    assert_eq!(stored.metadata.cover, Some(folder_cover));
    assert_eq!(
        db.count_rows_for_test("import_candidate_cover", "content_hash", &hash).await.unwrap(),
        1,
        "the first save stores the cover it read"
    );
}
