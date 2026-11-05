//! A release's files in each order the Storage sheet sorts them by.

use crate::*;
use bae_records::test_fixtures::aggregate_ordering::*;
use bae_records::test_fixtures::*;

/// A release's files come back in each order the Storage sheet sorts them
/// by — name, size, or kind, each forward or backward, ties by name and then
/// id — from the read itself, which walks that order's index either way. The
/// read follows a new order or release in place.
#[tokio::test]
async fn release_files_are_read_in_each_order_the_storage_sheet_sorts_by() {
    use crate::{ReleaseFileColumn, ReleaseFileOrder, ReleaseFilesRequest};
    let (db, tmp) = release_detail_db(crate::TEST_SCHEMA).await;
    for (i, (name, size, mime)) in [
        ("Track 10.flac", 30, "audio/flac"),
        ("cover.jpg", 20, "image/jpeg"),
        ("Track 2.flac", 30, "audio/flac"),
        ("album.cue", 10, "application/x-cue"),
    ]
    .into_iter()
    .enumerate()
    {
        insert_file(
            &db,
            tmp.path(),
            NewReleaseFile {
                id: bae_util::test_ids::test_uuid(&format!("sorted-file-{i}")),
                release_id: RELEASE_A.to_string(),
                original_filename: name.to_string(),
                content_type: bae_util::content_type::ContentType::from_mime(mime),
                source_audio: None,
                created_at: chrono::Utc::now(),
            },
            size,
        )
        .await;
    }
    let order = |column, descending| ReleaseFileOrder { column, descending };
    let request = |order| Some(ReleaseFilesRequest { release_id: RELEASE_A.to_string(), order });
    // The names of the first files the read gives for `asked`.
    async fn names(
        files: &mut coven::ReconfigurableLiveQuery<
            Option<ReleaseFilesRequest>,
            Option<Vec<DbFile>>,
        >,
        asked: &Option<ReleaseFilesRequest>,
    ) -> Vec<String> {
        loop {
            let event = files.next().await.expect("the read runs");
            if event.request == *asked {
                break event
                    .result
                    .unwrap()
                    .expect("the release exists")
                    .into_iter()
                    .map(|file| file.original_filename)
                    .collect();
            }
        }
    }
    let first = request(order(ReleaseFileColumn::Name, false));
    let mut files = crate::LibraryViews::new(db.clone()).subscribe_release_files(first.clone());
    assert_eq!(
        names(&mut files, &first).await,
        ["album.cue", "cover.jpg", "Track 2.flac", "Track 10.flac"]
    );
    for (column, descending, expected, index) in [
        (
            ReleaseFileColumn::Name,
            true,
            ["Track 10.flac", "Track 2.flac", "cover.jpg", "album.cue"],
            Some("idx_release_files_release_order"),
        ),
        (
            ReleaseFileColumn::Size,
            false,
            ["album.cue", "cover.jpg", "Track 2.flac", "Track 10.flac"],
            None,
        ),
        (
            ReleaseFileColumn::Size,
            true,
            ["Track 10.flac", "Track 2.flac", "cover.jpg", "album.cue"],
            None,
        ),
        (
            ReleaseFileColumn::Kind,
            false,
            ["album.cue", "Track 2.flac", "Track 10.flac", "cover.jpg"],
            Some("idx_release_files_release_kind"),
        ),
        (
            ReleaseFileColumn::Kind,
            true,
            ["cover.jpg", "Track 10.flac", "Track 2.flac", "album.cue"],
            Some("idx_release_files_release_kind"),
        ),
    ] {
        let asked = request(order(column, descending));
        files.requests().set(asked.clone()).unwrap();
        assert_eq!(
            names(&mut files, &asked).await,
            expected,
            "{column:?} descending: {descending}"
        );
        let statement = crate::release_files::release_files_in(order(column, descending));
        let plan = db
            .read(move |sql| {
                sql.query(&format!("EXPLAIN QUERY PLAN {statement}"), [RELEASE_A], |row| {
                    row.get::<_, String>(3)
                })
                .map_err(DbError::from)
            })
            .await
            .unwrap();
        match index {
            Some(index) => assert!(
                plan.iter().any(|step| step.contains(index))
                    && !plan.iter().any(|step| step.contains("TEMP B-TREE")),
                "{column:?} descending: {descending} walks {index}: {plan:?}"
            ),
            // Sizes are coven's: the release's few files are found by an
            // index of the release's and sorted by their coven files' sizes.
            None => assert!(
                plan.iter()
                    .any(|step| step.starts_with("SEARCH f USING") && step.contains("release_id=?")),
                "{column:?} descending: {descending} finds the release's files by index: {plan:?}"
            ),
        }
    }
}
