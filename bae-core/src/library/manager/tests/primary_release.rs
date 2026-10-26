use super::*;

/// Two devices that each make a different release primary while apart agree
/// on one primary once they have pulled each other's writes: both releases
/// are ordered below the album's lowest, and the tie goes to the lower id.
#[test]
fn devices_setting_different_primaries_converge_on_one() {
    crate::sync::test_devices::run_two_device_test(check_converging_primaries);
}

async fn primary_of(database: &crate::db::Database, album_id: &str) -> String {
    database
        .find_album_detail(album_id)
        .await
        .unwrap()
        .expect("the album holds releases")
        .primary_release_id
}

async fn check_converging_primaries(devices: &crate::sync::test_devices::TwoDevices) {
    let manager = devices.a().manager();
    let album = create_test_album();
    manager.database.insert_album(&album).await.unwrap();
    let releases: Vec<DbRelease> = (0..3).map(|_| create_test_release(&album.id)).collect();
    for release in &releases {
        insert_release(manager, release).await;
    }
    let orders = format!(
        "SELECT id || ':' || album_order FROM releases WHERE album_id = '{}'",
        album.id
    );
    devices.converge(&orders).await;
    assert_eq!(
        primary_of(devices.b().database(), &album.id).await,
        releases[0].id
    );

    devices.pause().await;
    devices
        .a()
        .manager()
        .set_album_primary_release(&album.id, &releases[1].id)
        .await
        .unwrap();
    devices
        .b()
        .manager()
        .set_album_primary_release(&album.id, &releases[2].id)
        .await
        .unwrap();
    devices.resume().await;
    devices.converge(&orders).await;

    let lower = std::cmp::min(&releases[1].id, &releases[2].id).clone();
    assert_eq!(primary_of(devices.a().database(), &album.id).await, lower);
    assert_eq!(primary_of(devices.b().database(), &album.id).await, lower);
}
