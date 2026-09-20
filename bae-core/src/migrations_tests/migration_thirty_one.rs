use super::*;

const ARTIST: &str = "aaaaaaaa-0000-4000-8000-000000000001";
const ALBUM: &str = "bbbbbbbb-0000-4000-8000-000000000001";
/// The release two catalogs describe.
const PAIRED: &str = "cccccccc-0000-4000-8000-000000000001";
/// The release whose draft started blank.
const BLANK: &str = "cccccccc-0000-4000-8000-000000000002";
/// The release whose draft was read off its files' own tags.
const TAGGED: &str = "cccccccc-0000-4000-8000-000000000003";
const IDENTITY_MB: &str = "dddddddd-0000-4000-8000-000000000001";
const IDENTITY_DISCOGS: &str = "dddddddd-0000-4000-8000-000000000002";

/// Every stored identity becomes a record: it gains the page its catalog
/// publishes, and the one the release's own column pointed at is the one that
/// reads the draft. The release column stops naming a document — all it says
/// now is whether the draft came off the files' own tags.
#[tokio::test]
#[serial]
async fn every_identity_becomes_a_record_and_the_draft_says_where_it_was_read() {
    let temp = tempfile::tempdir().expect("temp store");
    let store_dir = StoreDir::new_ephemeral(temp.path());
    let handle = open(
        store_dir.clone(),
        "migration-release-records",
        version_thirty(),
    )
    .expect("open version thirty");
    handle
        .write(|sql| {
            sql.execute_batch(&format!(
                "INSERT INTO artists (id, name, _updated_at, created_at)
                     VALUES ('{ARTIST}', 'Artist Name', 'h1', '2026-01-01T00:00:00Z');
                 INSERT INTO albums (id, title, artist_id, is_compilation, _updated_at, created_at)
                     VALUES ('{ALBUM}', 'Album Title', '{ARTIST}', 0, 'h1', '2026-01-01T00:00:00Z');
                 INSERT INTO releases
                     (id, album_id, metadata_source, metadata_source_release_id, remote,
                      _updated_at, created_at)
                     VALUES
                     ('{PAIRED}', '{ALBUM}', 'discogs', 'dg-1', 0, 'h1', '2026-01-01T00:00:00Z'),
                     ('{BLANK}', '{ALBUM}', 'none', NULL, 0, 'h1', '2026-01-01T00:00:00Z'),
                     ('{TAGGED}', '{ALBUM}', 'file_tags', NULL, 0, 'h1', '2026-01-01T00:00:00Z');
                 INSERT INTO release_identities
                     (id, release_id, source, source_group_id, source_release_id,
                      _updated_at, created_at)
                     VALUES
                     ('{IDENTITY_MB}', '{PAIRED}', 'musicbrainz', 'mb-group', 'mb-1',
                      'h1', '2026-01-01T00:00:00Z'),
                     ('{IDENTITY_DISCOGS}', '{PAIRED}', 'discogs', '909', 'dg-1',
                      'h1', '2026-01-01T00:00:00Z');",
            ))?;
            Ok(())
        })
        .await
        .expect("seed version-thirty identities");
    drop(handle);

    let handle = open(
        store_dir,
        "migration-release-records",
        all().into_iter().take(31).collect(),
    )
    .expect("migrate the identities into records");
    handle
        .read(|sql| {
            let records = sql.query(
                "SELECT id, release_id, catalog, key, group_key, url, reads_draft
                 FROM release_records ORDER BY catalog",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, i64>(6)?,
                    ))
                },
            )?;
            assert_eq!(
                records,
                vec![
                    (
                        IDENTITY_DISCOGS.to_string(),
                        PAIRED.to_string(),
                        "discogs".to_string(),
                        "dg-1".to_string(),
                        "909".to_string(),
                        "https://www.discogs.com/release/dg-1".to_string(),
                        1,
                    ),
                    (
                        IDENTITY_MB.to_string(),
                        PAIRED.to_string(),
                        "musicbrainz".to_string(),
                        "mb-1".to_string(),
                        "mb-group".to_string(),
                        "https://musicbrainz.org/release/mb-1".to_string(),
                        0,
                    ),
                ],
                "both catalogs the pick claimed keep their page, and only the one \
                 the draft was read from reads it"
            );

            let draft_source = sql.query(
                "SELECT id, draft_from_tags FROM releases ORDER BY id",
                [],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
            )?;
            assert_eq!(
                draft_source,
                vec![
                    (PAIRED.to_string(), 0),
                    (BLANK.to_string(), 0),
                    (TAGGED.to_string(), 1),
                ],
                "only the release seeded from its files' tags says so; a release \
                 read from a catalog reads the same as one that started blank, \
                 because its record is what names the document"
            );

            let orphaned: i64 = sql.query_row(
                &format!("SELECT COUNT(*) FROM release_records WHERE release_id <> '{PAIRED}'"),
                [],
                |row| row.get(0),
            )?;
            assert_eq!(
                orphaned, 0,
                "a release no catalog described carries no record"
            );
            Ok(())
        })
        .await
        .expect("read the migrated records");
}
