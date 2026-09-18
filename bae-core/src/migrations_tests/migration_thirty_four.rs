use super::*;

/// The ladder as it stood before a release kept what the rip databases said.
fn version_thirty_three() -> Vec<coven::Migration> {
    let mut migrations = all();
    migrations.truncate(33);
    migrations
}

const ARTIST: &str = "aaaaaaaa-0000-4000-8000-000000000011";
const ALBUM: &str = "bbbbbbbb-0000-4000-8000-000000000011";
const RELEASE: &str = "cccccccc-0000-4000-8000-000000000011";
const VERIFICATION: &str = "dddddddd-0000-4000-8000-000000000011";

/// The artist, album and release a verification row hangs off.
fn seed_release() -> String {
    format!(
        "INSERT INTO artists (id, name, _updated_at, created_at)
             VALUES ('{ARTIST}', 'Artist Name', 'h1', '2026-01-01T00:00:00Z');
         INSERT INTO albums (id, title, artist_id, is_compilation, _updated_at, created_at)
             VALUES ('{ALBUM}', 'Album Title', '{ARTIST}', 0, 'h1', '2026-01-01T00:00:00Z');
         INSERT INTO releases (id, album_id, remote, _updated_at, created_at)
             VALUES ('{RELEASE}', '{ALBUM}', 0, 'h1', '2026-01-01T00:00:00Z');",
    )
}

/// A library that had never read a rip log arrives with nothing verified, and
/// the releases it already holds survive the rung: the counts are read off the
/// log at import, and an import that already happened never read one.
#[tokio::test]
#[serial]
async fn a_release_from_before_the_rung_is_unverified() {
    let temp = tempfile::tempdir().expect("temp store");
    let store_dir = StoreDir::new_ephemeral(temp.path());
    let handle = open(
        store_dir.clone(),
        "migration-release-verification",
        version_thirty_three(),
    )
    .expect("open version thirty-three");
    handle
        .write(|sql| {
            sql.execute_batch(&seed_release())?;
            Ok(())
        })
        .await
        .expect("seed a version-thirty-three release");
    drop(handle);

    let handle = open(store_dir, "migration-release-verification", all())
        .expect("migrate the releases to verification");
    handle
        .read(|sql| {
            let rows: i64 =
                sql.query_row("SELECT COUNT(*) FROM release_verification", [], |row| {
                    row.get(0)
                })?;
            assert_eq!(rows, 0, "nothing read a log for a release already imported");
            let releases: i64 =
                sql.query_row("SELECT COUNT(*) FROM releases", [], |row| row.get(0))?;
            assert_eq!(releases, 1, "the release survives the rung");
            Ok(())
        })
        .await
        .expect("read the migrated release");
}

/// A release's verification syncs with it and cascades with it, and one track
/// carries one reading: a second row for the same track is refused.
#[tokio::test]
#[serial]
async fn a_release_s_verification_is_one_row_per_track_and_cascades() {
    let temp = tempfile::tempdir().expect("temp store");
    let store_dir = StoreDir::new_ephemeral(temp.path());
    let handle = open(store_dir, "migration-release-verification-cascade", all())
        .expect("open at the top of the ladder");
    handle
        .write(|sql| {
            sql.execute_batch(&seed_release())?;
            sql.execute_batch(&format!(
                "INSERT INTO release_verification
                     (id, release_id, track, source, accuraterip_confidence,
                      ctdb_confidence, crc, _updated_at, created_at)
                     VALUES ('{VERIFICATION}', '{RELEASE}', 1, 'log', 37, 12, 3914318293,
                             'h1', '2026-01-01T00:00:00Z');",
            ))?;
            Ok(())
        })
        .await
        .expect("seed a track's counts");
    handle
        .write(|sql| {
            let second = sql.execute_batch(&format!(
                "INSERT INTO release_verification
                     (id, release_id, track, source, _updated_at, created_at)
                     VALUES ('dddddddd-0000-4000-8000-000000000012', '{RELEASE}', 1, 'log',
                             'h1', '2026-01-01T00:00:00Z');",
            ));
            assert!(
                second.is_err(),
                "one track carries one reading, not two that disagree"
            );
            let unknown_source = sql.execute_batch(&format!(
                "INSERT INTO release_verification
                     (id, release_id, track, source, _updated_at, created_at)
                     VALUES ('dddddddd-0000-4000-8000-000000000013', '{RELEASE}', 2, 'guessed',
                             'h1', '2026-01-01T00:00:00Z');",
            ));
            assert!(
                unknown_source.is_err(),
                "a source nothing here writes is refused"
            );
            sql.execute_batch(&format!("DELETE FROM releases WHERE id = '{RELEASE}';"))?;
            Ok(())
        })
        .await
        .expect("delete the release");
    handle
        .read(|sql| {
            let rows: i64 =
                sql.query_row("SELECT COUNT(*) FROM release_verification", [], |row| {
                    row.get(0)
                })?;
            assert_eq!(rows, 0, "the counts go with the release they were read for");
            Ok(())
        })
        .await
        .expect("read the counts back");
}
