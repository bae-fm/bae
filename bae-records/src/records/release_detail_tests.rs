use super::*;
use bae_store::FormerDeviceState;

/// The line naming the device a release's audio is on follows coven's
/// registration of that device: active, removed, replaced when it was reset,
/// and removed when its person was removed from the library, whatever its own
/// registration still says.
#[test]
fn the_other_device_follows_its_registration_and_its_person() {
    let name = || "Device B".to_string();
    assert_eq!(other_device(name(), "active", false), Some(OtherDevice::Active { name: name() }));
    assert_eq!(
        other_device(name(), "removed", false),
        Some(OtherDevice::Former { name: name(), state: FormerDeviceState::Removed })
    );
    assert_eq!(
        other_device(name(), "replaced", false),
        Some(OtherDevice::Former { name: name(), state: FormerDeviceState::Replaced })
    );
    assert_eq!(
        other_device(name(), "active", true),
        Some(OtherDevice::Former { name: name(), state: FormerDeviceState::Removed }),
        "a removed person's devices are removed with them"
    );
    assert_eq!(other_device(name(), "lost", false), None);
}

/// A release file the library holds no row of, as one that left after the
/// read that found it on one device only, is on no one device.
#[tokio::test]
async fn a_file_on_no_one_device_names_no_other_device() {
    let (db, _temp) = bae_store::test_fixtures::temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let records = crate::test_fixtures::test_records(&db);

    let refused = records.release_file_other_device("gone").await;

    assert!(
        matches!(&refused, Err(OtherDeviceError::NotOnAnotherDevice(file)) if file == "gone"),
        "{refused:?}"
    );
}

/// The track statements place each track where
/// [`bae_types::mediums::track_position`] places it over decoded rows: a CD
/// among several mediums gets a disc number, a record's or cassette's sides
/// get letters across the release's sided mediums, and a release of one
/// medium shows only track numbers.
#[tokio::test]
async fn the_statements_place_tracks_as_the_rule_over_decoded_rows_does() {
    use crate::test_fixtures::library::RecordsFixture;
    use bae_types::mediums::{track_position, ReleaseMedium};
    use bae_types::pressing::Medium;
    let (db, _temp) = bae_store::test_fixtures::temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let release = "f2b6dac7-8094-4b1d-8c53-6e7f8a9b0c01";
    let carriers = [
        (Medium::Cd, 1u8),
        (Medium::Vinyl, 2),
        (Medium::Digital, 1),
        (Medium::Cassette, 2),
        (Medium::Vinyl, 3),
    ];
    let medium_id = |position: usize| format!("{release}-medium-{position}");
    let mut fixture = RecordsFixture::new()
        .artist("f2b6dac7-8094-4b1d-8c53-6e7f8a9b0c02", "Artist Name")
        .album(
            "f2b6dac7-8094-4b1d-8c53-6e7f8a9b0c03",
            "Album Title",
            None,
            &["f2b6dac7-8094-4b1d-8c53-6e7f8a9b0c02"],
        )
        .release("f2b6dac7-8094-4b1d-8c53-6e7f8a9b0c03", release);
    let mut ids = Vec::new();
    for (position, (_, sides)) in carriers.iter().enumerate() {
        for side in 1..=*sides {
            ids.push((
                bae_util::test_ids::test_uuid(&format!("track-{position}-{side}")),
                position,
                side,
            ));
        }
    }
    let tracks: Vec<(&str, &str)> =
        ids.iter().map(|(id, ..)| (id.as_str(), "Track Title")).collect();
    fixture = fixture.tracks(release, &tracks).edit(|rows| {
        rows.mediums = carriers
            .iter()
            .enumerate()
            .map(|(position, (carrier, sides))| {
                let mut row = crate::DbReleaseMedium::new_test(release);
                row.id = medium_id(position);
                row.position = std::num::NonZeroU16::new(position as u16 + 1).unwrap();
                row.medium = ReleaseMedium {
                    carrier: Some(*carrier),
                    sides: std::num::NonZeroU8::new(*sides).unwrap(),
                };
                row
            })
            .collect();
        for (track, (_, position, side)) in rows.tracks.iter_mut().zip(&ids) {
            track.medium_id = medium_id(*position);
            track.side = std::num::NonZeroU8::new(*side).unwrap();
        }
    });
    fixture.write(&crate::test_fixtures::test_records(&db)).await;

    let (groups, mediums, places) = db
        .read(move |sql| {
            let (detail, ()) =
                release_detail_rows_on(&sql, release, NO_STARS, "", |_| Ok(()))?.unwrap();
            let groups = detail.process().track_groups;
            let mediums = release_mediums_on(&sql, release)?;
            let places = release_tracks::track_places_on(
                &sql,
                &ids.iter().map(|(id, ..)| id.clone()).collect::<Vec<_>>(),
            )?;
            Ok((groups, mediums, places))
        })
        .await
        .unwrap();

    let rows = db
        .read(move |sql| {
            sql.query(
                "SELECT id, medium_id, side, track_number FROM tracks WHERE release_id = ?1 ORDER BY position",
                [release],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, u8>(2)?, row.get::<_, u32>(3)?)),
            )
            .map_err(bae_store::DbError::from)
        })
        .await
        .unwrap();
    let expected: Vec<_> = rows
        .iter()
        .map(|(id, medium_id, side, number)| {
            let medium = mediums.0.iter().find(|medium| &medium.id == medium_id).unwrap();
            let place = bae_types::mediums::TrackPlace {
                medium: medium.position,
                side: std::num::NonZeroU8::new(*side).unwrap(),
                number: std::num::NonZeroU32::new(*number).unwrap(),
            };
            (id.clone(), track_position(&place, &mediums.mediums()))
        })
        .collect();
    let read: Vec<_> = groups
        .iter()
        .flat_map(|group| &group.tracks)
        .map(|track| (track.id.clone(), track.position.clone()))
        .collect();
    assert_eq!(read, expected);
    let sides: Vec<_> = places.iter().map(|place| place.on.clone()).collect();
    let expected_sides: Vec<_> = expected.iter().map(|(_, position)| position.side()).collect();
    assert_eq!(sides, expected_sides);
}

/// Reads `release`'s detail from `store` and returns the other device it
/// names, or `None` when the release plays here.
async fn elsewhere_of(store: &bae_store::Store, release: &'static str) -> Option<OtherDevice> {
    store
        .read(move |sql| {
            Ok(release_detail_rows_on(&sql, release, NO_STARS, "", |_| Ok(()))?
                .expect("the release")
                .0
                .process()
                .elsewhere)
        })
        .await
        .unwrap()
}

/// Imports one file named `name` of `content_type` into `release` on
/// `store`'s device, from an original that the function writes under `dir`.
async fn import_file(
    store: &bae_store::Store,
    dir: &std::path::Path,
    release: &str,
    name: &str,
    content_type: bae_util::content_type::ContentType,
) {
    let original = dir.join(name);
    std::fs::write(&original, name).unwrap();
    let file = NewReleaseFile::new(
        release,
        name,
        content_type,
        crate::ReleaseFileRole::Other,
        bae_util::test_ids::test_uuid(name),
        chrono::Utc::now(),
    );
    crate::test_fixtures::test_records(store).insert_file_for_test(&file, &original).await.unwrap();
}

/// A release is on another device when that device alone holds one of its
/// audio files, and then the release detail names that device. The detail
/// names no other device for audio whose original this device holds, or for
/// files that are not audio.
#[test]
fn a_release_names_the_device_that_holds_one_of_its_audio_files_only() {
    use crate::test_fixtures::*;
    use bae_util::content_type::ContentType::{Flac, Jpeg};
    const PLAYS_HERE: &str = "5a1c0a52-0a3c-4c3e-9d0f-1d2b3c4d5e01";
    const ONE_ELSEWHERE: &str = "5a1c0a52-0a3c-4c3e-9d0f-1d2b3c4d5e02";
    const ARTWORK_ELSEWHERE: &str = "5a1c0a52-0a3c-4c3e-9d0f-1d2b3c4d5e03";
    const TWO_ELSEWHERE: &str = "5a1c0a52-0a3c-4c3e-9d0f-1d2b3c4d5e04";
    bae_store::test_devices::run_two_store_test(TEST_SCHEMA, async |devices| {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (devices.a(), devices.b());
        crate::test_fixtures::library::RecordsFixture::new()
            .album(ALBUM_A, "Album Title", None, &[])
            .release(ALBUM_A, PLAYS_HERE)
            .release(ALBUM_A, ONE_ELSEWHERE)
            .release(ALBUM_A, ARTWORK_ELSEWHERE)
            .release(ALBUM_A, TWO_ELSEWHERE)
            .write(&test_records(a))
            .await;
        devices.converge("SELECT id FROM releases").await;

        import_file(b, dir.path(), PLAYS_HERE, "here.flac", Flac).await;
        import_file(a, dir.path(), ONE_ELSEWHERE, "one.flac", Flac).await;
        import_file(a, dir.path(), ONE_ELSEWHERE, "cover.jpg", Jpeg).await;
        import_file(a, dir.path(), ARTWORK_ELSEWHERE, "front.jpg", Jpeg).await;
        import_file(a, dir.path(), TWO_ELSEWHERE, "b-second.flac", Flac).await;
        import_file(a, dir.path(), TWO_ELSEWHERE, "a-first.flac", Flac).await;
        devices.converge("SELECT id, release_id, file FROM release_files").await;
        devices.converge("SELECT id, coordinates FROM coven_files").await;

        let name_of_a: String = b
            .read(|sql| {
                Ok(sql.query_row(
                    "SELECT name FROM coven_devices WHERE NOT is_self",
                    [],
                    |row| row.get(0),
                )?)
            })
            .await
            .unwrap();
        let a_active = Some(OtherDevice::Active { name: name_of_a });
        assert_eq!(
            elsewhere_of(b, PLAYS_HERE).await,
            None,
            "audio the device holds the original of"
        );
        assert_eq!(elsewhere_of(a, ONE_ELSEWHERE).await, None, "the device that imported it");
        assert_eq!(elsewhere_of(b, ONE_ELSEWHERE).await, a_active);
        assert_eq!(elsewhere_of(b, ARTWORK_ELSEWHERE).await, None, "artwork is not audio");
        assert_eq!(elsewhere_of(b, TWO_ELSEWHERE).await, a_active);
        assert_eq!(
            elsewhere_of(a, PLAYS_HERE).await,
            Some(OtherDevice::Active { name: "Device B".into() })
        );
    });
}
