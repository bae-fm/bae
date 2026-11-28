use super::*;
use crate::test_fixtures::{artist_album_release, test_records, write_cover, RELEASE_A, RELEASE_B};
use coven::Statement;

/// `RELEASE_A` with a log and then a cover image among its files.
async fn release_of_two_files() -> (bae_store::Store, bae_util::test_files::TestFolder, Records) {
    let (db, temp) = bae_store::test_fixtures::temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let records = test_records(&db);
    artist_album_release().write(&records).await;
    for name in ["b.jpg", "a.log"] {
        let original = temp.write(name, name.as_bytes());
        let role = if name == "a.log" {
            crate::ReleaseFileRole::Document(crate::DocumentKind::RipLog)
        } else {
            crate::ReleaseFileRole::Artwork
        };
        let file = NewReleaseFile::new(
            RELEASE_A,
            name,
            ContentType::from_mime("image/jpeg"),
            role,
            bae_util::test_ids::test_uuid(name),
            time::UtcDateTime::now(),
        );
        records.insert_file_for_test(&file, &original).await.unwrap();
    }
    (db, temp, records)
}

/// A release reads with its files and its cover; one of its files reads by its
/// id, and a file asked of another release reads as none.
#[tokio::test]
async fn a_release_reads_with_its_files() {
    let (_db, _temp, records) = release_of_two_files().await;

    let uncovered = records.find_release_with_files(RELEASE_A).await.unwrap().unwrap();
    assert_eq!(uncovered.cover_file, None);
    write_cover(&records, RELEASE_A, "cover bytes").await;

    let release = records.find_release_with_files(RELEASE_A).await.unwrap().unwrap();
    let names: Vec<_> = release.files.iter().map(|file| file.original_filename.as_str()).collect();
    assert_eq!(names, ["a.log", "b.jpg"]);
    assert_eq!(release.album_id, uncovered.album_id);
    assert!(release.cover_file.is_some());

    let log = &release.files[0];
    let one = records.find_release_file(RELEASE_A, &log.id).await.unwrap();
    assert_eq!(one.as_ref(), Some(log));
    assert_eq!(records.find_release_file(RELEASE_B, &log.id).await.unwrap(), None);
    assert_eq!(records.find_release_with_files(RELEASE_B).await.unwrap(), None);
}

/// A release with its files, one of its files, and the plan of its deletion
/// are each one statement, which finds the release's files by their key.
#[tokio::test]
async fn a_releases_files_are_one_statement() {
    let (db, _temp, records) = release_of_two_files().await;
    let file = records.get_files_for_release(RELEASE_A).await.unwrap().remove(0);

    let counter = db.track_work_for_test().await.unwrap();
    records.find_release_with_files(RELEASE_A).await.unwrap();
    assert_eq!(counter.take().app_read_statements, 1);
    records.find_release_file(RELEASE_A, &file.id).await.unwrap();
    assert_eq!(counter.take().app_read_statements, 1);
    records.plan_release_deletion(RELEASE_A).await.unwrap();
    assert_eq!(counter.take().app_read_statements, 1);
    drop(counter);

    let plan = db
        .read(|sql| {
            let whole = OneReleaseWithFilesParams { release: RELEASE_A.to_string() };
            sql.query(
                &format!("EXPLAIN QUERY PLAN {}", OneReleaseWithFiles::SQL),
                whole.params().as_slice(),
                |row| row.get::<_, String>(3),
            )
            .map_err(DbError::from)
        })
        .await
        .unwrap();
    let walked: Vec<_> =
        plan.iter().filter(|step| step.starts_with("SCAN release_files")).collect();
    assert!(walked.is_empty(), "the read walks every release's files: {plan:#?}");
}
