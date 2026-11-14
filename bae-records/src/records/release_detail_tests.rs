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
        matches!(&refused, Err(OtherDeviceError::NotOnOneDevice(file)) if file == "gone"),
        "{refused:?}"
    );
}

/// The places the statements put tracks in are the rule
/// [`bae_types::mediums::track_position`] states over decoded rows: a CD of
/// a release of several mediums is a numbered disc, the sides of records and
/// cassettes are lettered across the release's sided mediums, and a release
/// of one medium is plain.
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
