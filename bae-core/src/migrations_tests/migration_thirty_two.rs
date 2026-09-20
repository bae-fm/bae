use super::*;

/// The ladder as it stood before per-field origins.
fn version_thirty_one() -> Vec<coven::Migration> {
    let mut migrations = all();
    migrations.truncate(31);
    migrations
}

const ARTIST: &str = "aaaaaaaa-0000-4000-8000-000000000001";
const ALBUM: &str = "bbbbbbbb-0000-4000-8000-000000000001";
/// The release whose draft was read from a catalog's document.
const FROM_RECORD: &str = "cccccccc-0000-4000-8000-000000000001";
/// The release whose draft was read off its files' own tags.
const FROM_TAGS: &str = "cccccccc-0000-4000-8000-000000000002";
/// The release that started blank.
const FROM_NOTHING: &str = "cccccccc-0000-4000-8000-000000000003";
const RECORD: &str = "dddddddd-0000-4000-8000-000000000001";
/// The candidate whose draft a pick filled.
const PICKED_HASH: &str = "hash-picked";
/// The candidate nobody has picked a source for.
const UNPICKED_HASH: &str = "hash-unpicked";

/// Every field a stored draft states takes the origin of whatever filled the
/// draft whole; a field it leaves blank takes none, and a draft read from
/// nothing describes no field at all.
#[tokio::test]
#[serial]
async fn every_stated_field_takes_the_origin_of_what_filled_the_draft() {
    let temp = tempfile::tempdir().expect("temp store");
    let store_dir = StoreDir::new_ephemeral(temp.path());
    let handle = open(
        store_dir.clone(),
        "migration-field-origins",
        version_thirty_one(),
    )
    .expect("open version thirty-one");
    handle
        .write(|sql| {
            sql.execute_batch(&format!(
                "INSERT INTO artists (id, name, _updated_at, created_at)
                     VALUES ('{ARTIST}', 'Artist Name', 'h1', '2026-01-01T00:00:00Z');
                 INSERT INTO albums (id, title, artist_id, year, is_compilation,
                                     _updated_at, created_at)
                     VALUES ('{ALBUM}', 'Album Title', '{ARTIST}', 1996, 0, 'h1',
                             '2026-01-01T00:00:00Z');
                 INSERT INTO releases
                     (id, album_id, year, label, barcode, draft_from_tags, remote,
                      _updated_at, created_at)
                     VALUES
                     ('{FROM_RECORD}', '{ALBUM}', 1997, 'Label Name', NULL, 0, 0, 'h1',
                      '2026-01-01T00:00:00Z'),
                     ('{FROM_TAGS}', '{ALBUM}', 1997, 'Label Name', NULL, 1, 0, 'h1',
                      '2026-01-01T00:00:00Z'),
                     ('{FROM_NOTHING}', '{ALBUM}', 1997, 'Label Name', NULL, 0, 0, 'h1',
                      '2026-01-01T00:00:00Z');
                 INSERT INTO release_records
                     (id, release_id, catalog, key, group_key, url, reads_draft,
                      _updated_at, created_at)
                     VALUES ('{RECORD}', '{FROM_RECORD}', 'musicbrainz', 'mb-1',
                             'mb-group', 'https://musicbrainz.org/release/mb-1', 1,
                             'h1', '2026-01-01T00:00:00Z');
                 INSERT INTO import_candidate_state
                     (content_hash, folder_path, metadata_revision, edit_revision)
                     VALUES ('{PICKED_HASH}', '/watched/Picked', 0, 0),
                            ('{UNPICKED_HASH}', '/watched/Unpicked', 0, 0);
                 INSERT INTO import_candidate_edit
                     (content_hash, album_title, album_year, year, format, label,
                      catalog_number, country, barcode)
                     VALUES
                     ('{PICKED_HASH}', 'Album Title', '1996', '1997', 'CD', 'Label Name',
                      'CAT-1', 'US', ''),
                     ('{UNPICKED_HASH}', 'Album Title', '', '', '', '', '', '', '');
                 INSERT INTO import_candidate_draft_provenance
                     (content_hash, kind, source, release_id, author)
                     VALUES ('{PICKED_HASH}', 'external_release', 'discogs', 'dg-1', 'user');",
            ))?;
            Ok(())
        })
        .await
        .expect("seed version-thirty-one drafts");
    drop(handle);

    let mut migrations = all();
    migrations.truncate(32);
    let handle = open(store_dir, "migration-field-origins", migrations)
        .expect("migrate the drafts onto per-field origins");
    handle
        .read(|sql| {
            let candidates = sql.query(
                "SELECT content_hash, album_title_origin, album_year_origin, year_origin,
                        format_origin, label_origin, catalog_number_origin, country_origin,
                        barcode_origin
                 FROM import_candidate_edit ORDER BY content_hash",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        (1..=8)
                            .map(|index| row.get::<_, Option<String>>(index))
                            .collect::<Result<Vec<_>, _>>()?,
                    ))
                },
            )?;
            let record = Some("record:discogs".to_string());
            assert_eq!(
                candidates,
                vec![
                    (
                        PICKED_HASH.to_string(),
                        vec![
                            record.clone(),
                            record.clone(),
                            record.clone(),
                            record.clone(),
                            record.clone(),
                            record.clone(),
                            record.clone(),
                            // The pick states no barcode, so nothing describes
                            // the blank it left.
                            None,
                        ],
                    ),
                    (UNPICKED_HASH.to_string(), vec![None; 8]),
                ],
                "a picked draft's fields were read from the catalog it was \
                 filled from; a draft nobody picked a source for was read from \
                 nothing"
            );

            let releases = sql.query(
                "SELECT id, album_title_origin, album_year_origin, year_origin,
                        format_origin, label_origin, barcode_origin
                 FROM releases ORDER BY id",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        (1..=6)
                            .map(|index| row.get::<_, Option<String>>(index))
                            .collect::<Result<Vec<_>, _>>()?,
                    ))
                },
            )?;
            let mb = Some("record:musicbrainz".to_string());
            let tags = Some("tags".to_string());
            assert_eq!(
                releases,
                vec![
                    (
                        FROM_RECORD.to_string(),
                        vec![mb.clone(), mb.clone(), mb.clone(), None, mb, None],
                    ),
                    (
                        FROM_TAGS.to_string(),
                        vec![tags.clone(), tags.clone(), tags.clone(), None, tags, None,],
                    ),
                    (FROM_NOTHING.to_string(), vec![None; 6]),
                ],
                "the record that reads a release's draft, or the files' tags, \
                 is where every field it states was read; a release read from \
                 nothing describes none of them"
            );
            Ok(())
        })
        .await
        .expect("read the migrated origins");
}
