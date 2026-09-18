use super::*;

/// The ladder as it stood before a release said what identified it.
fn version_thirty_four() -> Vec<coven::Migration> {
    let mut migrations = all();
    migrations.truncate(34);
    migrations
}

const ARTIST: &str = "aaaaaaaa-0000-4000-8000-000000000001";
const ALBUM: &str = "bbbbbbbb-0000-4000-8000-000000000001";
const RELEASE: &str = "cccccccc-0000-4000-8000-000000000001";

/// Nothing is invented for the releases already in the library: the lookups
/// that found them were dropped at commit, so no reading of a stored row can
/// say which name tied their files to a record.
#[tokio::test]
#[serial]
async fn releases_already_in_the_library_name_nothing() {
    let temp = tempfile::tempdir().expect("temp store");
    let store_dir = StoreDir::new_ephemeral(temp.path());
    let handle = open(
        store_dir.clone(),
        "migration-identified-by",
        version_thirty_four(),
    )
    .expect("open version thirty-four");
    handle
        .write(|sql| {
            sql.execute_batch(&format!(
                "INSERT INTO artists (id, name, _updated_at, created_at)
                     VALUES ('{ARTIST}', 'Artist Name', 'h1', '2026-01-01T00:00:00Z');
                 INSERT INTO albums (id, title, artist_id, is_compilation, _updated_at, created_at)
                     VALUES ('{ALBUM}', 'Album Title', '{ARTIST}', 0, 'h1', '2026-01-01T00:00:00Z');
                 INSERT INTO releases (id, album_id, remote, _updated_at, created_at)
                     VALUES ('{RELEASE}', '{ALBUM}', 0, 'h1', '2026-01-01T00:00:00Z');",
            ))?;
            Ok(())
        })
        .await
        .expect("seed a version-thirty-four release");
    drop(handle);

    let handle = open(store_dir, "migration-identified-by", all())
        .expect("migrate the releases to identified_by");
    handle
        .read(|sql| {
            let stored: Option<String> = sql.query_row(
                &format!("SELECT identified_by FROM releases WHERE id = '{RELEASE}'"),
                [],
                |row| row.get(0),
            )?;
            assert_eq!(stored, None, "nothing stood behind an answer here");
            Ok(())
        })
        .await
        .expect("read the migrated release");
}

/// The column holds one of the three names an object carries, and nothing
/// else — a word no `MarkKind` reads back as is refused at the row.
#[tokio::test]
#[serial]
async fn only_a_name_the_object_carries_is_stored() {
    let temp = tempfile::tempdir().expect("temp store");
    let store_dir = StoreDir::new_ephemeral(temp.path());
    let handle = open(store_dir, "migration-identified-by-values", all())
        .expect("open at the top of the ladder");
    handle
        .write(|sql| {
            sql.execute_batch(&format!(
                "INSERT INTO artists (id, name, _updated_at, created_at)
                     VALUES ('{ARTIST}', 'Artist Name', 'h1', '2026-01-01T00:00:00Z');
                 INSERT INTO albums (id, title, artist_id, is_compilation, _updated_at, created_at)
                     VALUES ('{ALBUM}', 'Album Title', '{ARTIST}', 0, 'h1', '2026-01-01T00:00:00Z');
                 INSERT INTO releases
                     (id, album_id, remote, identified_by, _updated_at, created_at)
                     VALUES ('{RELEASE}', '{ALBUM}', 0, 'disc_id', 'h1',
                             '2026-01-01T00:00:00Z');",
            ))?;
            let unknown = sql.execute_batch(&format!(
                "UPDATE releases SET identified_by = 'matrix' WHERE id = '{RELEASE}';"
            ));
            assert!(
                unknown.is_err(),
                "a matrix number is not a name any lookup answers to yet"
            );
            Ok(())
        })
        .await
        .expect("seed and probe the column");
    handle
        .read(|sql| {
            let stored: Option<String> = sql.query_row(
                &format!("SELECT identified_by FROM releases WHERE id = '{RELEASE}'"),
                [],
                |row| row.get(0),
            )?;
            assert_eq!(stored.as_deref(), Some("disc_id"));
            Ok(())
        })
        .await
        .expect("read the stored name back");
}
