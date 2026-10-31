//! A track always sits on one of its release's medium rows: the schema refuses
//! a write that leaves one on a missing medium, and coven keeps two devices'
//! concurrent writes from leaving one either.

use super::*;

const RELEASE: &str = "0252dedb-ee39-4547-8803-438dbeb57a64";
const KEPT_TRACK: &str = "4f3c4c52-4d1b-4bd5-9b45-6a6f2f3c1a10";
const MOVED_TRACK: &str = "6b1e1f0e-55a8-4d0f-8a5e-0d2e4a3c7b21";
const STRAY_TRACK: &str = "8d2f6a1b-7c3e-4e5f-9a0b-1c2d3e4f5a6b";

fn medium_id(position: u16) -> String {
    crate::db::identity::release_medium_id(RELEASE, position)
}

/// The seeded release with two CDs and one track on each.
fn seed_two_discs(conn: &coven::SqlContext<'_, '_>) -> Result<(), crate::db::DbError> {
    seed_artist_album_release(conn)?;
    for position in [1u16, 2] {
        conn.execute(
            "INSERT INTO release_mediums (id, release_id, position, medium, side_count, created_at) \
             VALUES (?1, ?2, ?3, 'cd', 1, '2026-01-01T00:00:00Z')",
            rusqlite::params![medium_id(position), RELEASE, position],
        )?;
    }
    for (position, track, medium) in [(0, KEPT_TRACK, 1u16), (1, MOVED_TRACK, 2)] {
        insert_track(conn, track, position, medium)?;
    }
    Ok(())
}

fn insert_track(
    conn: &coven::SqlContext<'_, '_>,
    track: &str,
    position: i64,
    medium: u16,
) -> Result<(), crate::db::DbError> {
    conn.execute(
        "INSERT INTO tracks (id, release_id, title, medium_id, side, track_number, \
             duration_ms, position, created_at) \
         VALUES (?1, ?2, 'Track Title', ?3, 1, 1, 1000, ?4, '2026-01-01T00:00:00Z')",
        rusqlite::params![track, RELEASE, medium_id(medium), position],
    )?;
    Ok(())
}

/// Removing a medium a track still sits on fails at commit; moving the track
/// first, in the same write, removes it.
#[tokio::test]
async fn a_medium_still_holding_a_track_cannot_be_removed() {
    let (db, _tmp) = temp_db().await;
    db.call(seed_two_discs).await.unwrap();

    let refused = db
        .call(|conn| {
            conn.execute(
                "DELETE FROM release_mediums WHERE id = ?1",
                rusqlite::params![medium_id(2)],
            )?;
            Ok(())
        })
        .await;
    assert!(refused.is_err(), "the track on disc 2 keeps its medium");

    db.call(|conn| {
        conn.execute(
            "UPDATE tracks SET medium_id = ?1 WHERE medium_id = ?2",
            rusqlite::params![medium_id(1), medium_id(2)],
        )?;
        conn.execute(
            "DELETE FROM release_mediums WHERE id = ?1",
            rusqlite::params![medium_id(2)],
        )?;
        Ok(())
    })
    .await
    .unwrap();
    assert_eq!(
        db.get_release_mediums(RELEASE).await.unwrap().0.len(),
        1,
        "the emptied disc is gone"
    );
}

/// The tracks of either device that sit on no medium row its library holds.
async fn tracks_on_missing_mediums(db: &crate::db::Database) -> Vec<String> {
    db.read(|sql| {
        sql.query(
            "SELECT t.id FROM tracks t \
             LEFT JOIN release_mediums m ON m.id = t.medium_id \
             WHERE m.id IS NULL",
            [],
            |row| row.get(0),
        )
        .map_err(crate::db::DbError::from)
    })
    .await
    .unwrap()
}

/// One device removes disc 2, moving its track to disc 1, while the other,
/// apart, writes a new track on disc 2. Once each has the other's writes,
/// neither holds a track on a missing medium.
#[test]
fn concurrent_medium_removal_and_track_write_leave_no_track_on_a_missing_medium() {
    crate::sync::test_devices::run_two_device_test(check_concurrent_medium_removal);
}

async fn check_concurrent_medium_removal(devices: &mut crate::sync::test_devices::TwoDevices) {
    devices.a().database().call(seed_two_discs).await.unwrap();
    let rows = "SELECT t.id || ':' || t.medium_id FROM tracks t \
         UNION ALL SELECT 'medium:' || m.id FROM release_mediums m";
    devices.converge(rows).await;

    devices.pause().await;
    devices
        .a()
        .database()
        .call(|conn| {
            conn.execute(
                "UPDATE tracks SET medium_id = ?1 WHERE medium_id = ?2",
                rusqlite::params![medium_id(1), medium_id(2)],
            )?;
            conn.execute(
                "DELETE FROM release_mediums WHERE id = ?1",
                rusqlite::params![medium_id(2)],
            )?;
            Ok(())
        })
        .await
        .unwrap();
    devices
        .b()
        .database()
        .call(|conn| insert_track(conn, STRAY_TRACK, 2, 2))
        .await
        .unwrap();
    devices.resume().await;
    devices.converge(rows).await;

    for device in [devices.a(), devices.b()] {
        assert_eq!(
            tracks_on_missing_mediums(device.database()).await,
            Vec::<String>::new()
        );
    }
}
