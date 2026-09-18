use super::*;

/// The ladder as it stood before a release kept its marks.
fn version_thirty_two() -> Vec<coven::Migration> {
    let mut migrations = all();
    migrations.truncate(32);
    migrations
}

const ARTIST: &str = "aaaaaaaa-0000-4000-8000-000000000001";
const ALBUM: &str = "bbbbbbbb-0000-4000-8000-000000000001";
/// The release whose re-identify pass had recomputed a disc ID into its column.
const WITH_DISC_ID: &str = "cccccccc-0000-4000-8000-000000000001";
/// The release whose column was empty.
const WITHOUT_DISC_ID: &str = "cccccccc-0000-4000-8000-000000000002";

/// The disc-ID column goes and leaves no mark behind. It held what a
/// re-identify pass recomputed from the stored tracks, not what was read off
/// the object — and nothing ever read it back, so there is nothing to carry
/// over. A release's marks start empty and are written by the imports that
/// follow.
#[tokio::test]
#[serial]
async fn the_disc_id_column_goes_and_leaves_no_mark() {
    let temp = tempfile::tempdir().expect("temp store");
    let store_dir = StoreDir::new_ephemeral(temp.path());
    let handle = open(
        store_dir.clone(),
        "migration-release-marks",
        version_thirty_two(),
    )
    .expect("open version thirty-two");
    handle
        .write(|sql| {
            sql.execute_batch(&format!(
                "INSERT INTO artists (id, name, _updated_at, created_at)
                     VALUES ('{ARTIST}', 'Artist Name', 'h1', '2026-01-01T00:00:00Z');
                 INSERT INTO albums (id, title, artist_id, is_compilation, _updated_at, created_at)
                     VALUES ('{ALBUM}', 'Album Title', '{ARTIST}', 0, 'h1', '2026-01-01T00:00:00Z');
                 INSERT INTO releases (id, album_id, disc_id, remote, _updated_at, created_at)
                     VALUES
                     ('{WITH_DISC_ID}', '{ALBUM}', 'XyZ.abc-123', 0, 'h1',
                      '2026-01-01T00:00:00Z'),
                     ('{WITHOUT_DISC_ID}', '{ALBUM}', NULL, 0, 'h1', '2026-01-01T00:00:00Z');",
            ))?;
            Ok(())
        })
        .await
        .expect("seed version-thirty-two releases");
    drop(handle);

    let handle =
        open(store_dir, "migration-release-marks", all()).expect("migrate the releases to marks");
    handle
        .read(|sql| {
            let marks: i64 =
                sql.query_row("SELECT COUNT(*) FROM release_marks", [], |row| row.get(0))?;
            assert_eq!(
                marks, 0,
                "the column held a recomputed value, not one read off the object"
            );

            let columns = sql.query(
                "SELECT name FROM pragma_table_info('releases')",
                [],
                |row| row.get::<_, String>(0),
            )?;
            assert!(
                !columns.iter().any(|name| name == "disc_id"),
                "the disc ID lives in a mark now, not beside it on the release: {columns:?}"
            );

            let releases: i64 =
                sql.query_row("SELECT COUNT(*) FROM releases", [], |row| row.get(0))?;
            assert_eq!(releases, 2, "both releases survive the rebuild");
            Ok(())
        })
        .await
        .expect("read the migrated releases");
}

/// Marks sync with the release they hang off, and cascade with it.
#[tokio::test]
#[serial]
async fn a_release_s_marks_cascade_with_it() {
    let temp = tempfile::tempdir().expect("temp store");
    let store_dir = StoreDir::new_ephemeral(temp.path());
    let handle = open(store_dir, "migration-release-marks-cascade", all())
        .expect("open at the top of the ladder");
    handle
        .write(|sql| {
            sql.execute_batch(&format!(
                "INSERT INTO artists (id, name, _updated_at, created_at)
                     VALUES ('{ARTIST}', 'Artist Name', 'h1', '2026-01-01T00:00:00Z');
                 INSERT INTO albums (id, title, artist_id, is_compilation, _updated_at, created_at)
                     VALUES ('{ALBUM}', 'Album Title', '{ARTIST}', 0, 'h1', '2026-01-01T00:00:00Z');
                 INSERT INTO releases (id, album_id, remote, _updated_at, created_at)
                     VALUES ('{WITH_DISC_ID}', '{ALBUM}', 0, 'h1', '2026-01-01T00:00:00Z');
                 INSERT INTO release_marks
                     (id, release_id, position, kind, value, origin, origin_path,
                      region_x, region_y, region_width, region_height,
                      _updated_at, created_at)
                     VALUES
                     ('dddddddd-0000-4000-8000-000000000001', '{WITH_DISC_ID}', 0, 'barcode',
                      '0075678164521', 'artwork', 'back.jpg', 0.1, 0.2, 0.3, 0.4, 'h1',
                      '2026-01-01T00:00:00Z');",
            ))?;
            Ok(())
        })
        .await
        .expect("seed a mark");
    handle
        .write(|sql| {
            let partial = sql.execute_batch(&format!(
                "INSERT INTO release_marks
                     (id, release_id, position, kind, value, origin, region_x,
                      _updated_at, created_at)
                     VALUES ('dddddddd-0000-4000-8000-000000000002', '{WITH_DISC_ID}', 1,
                             'barcode', '0075678164521', 'artwork', 0.1, 'h1',
                             '2026-01-01T00:00:00Z');",
            ));
            assert!(
                partial.is_err(),
                "half a box crops nothing, so the row is refused"
            );
            sql.execute_batch(&format!(
                "DELETE FROM releases WHERE id = '{WITH_DISC_ID}';"
            ))?;
            Ok(())
        })
        .await
        .expect("delete the release");
    handle
        .read(|sql| {
            let marks: i64 =
                sql.query_row("SELECT COUNT(*) FROM release_marks", [], |row| row.get(0))?;
            assert_eq!(marks, 0, "the marks go with the release they were read for");
            Ok(())
        })
        .await
        .expect("read the marks back");
}
