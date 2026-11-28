use super::*;
use bae_store::FormerDeviceState;

/// The line naming the device a release's audio is on follows coven's
/// registration of that device: active, removed, replaced when it was reset,
/// and removed when its person was removed from the library, whatever its own
/// registration still says.
#[test]
fn the_other_device_follows_its_registration_and_its_person() {
    use coven::DeviceState;
    let name = || "Device B".to_string();
    assert_eq!(
        other_device(name(), DeviceState::Active, false),
        OtherDevice::Active { name: name() }
    );
    assert_eq!(
        other_device(name(), DeviceState::Removed, false),
        OtherDevice::Former { name: name(), state: FormerDeviceState::Removed }
    );
    assert_eq!(
        other_device(name(), DeviceState::Replaced, false),
        OtherDevice::Former { name: name(), state: FormerDeviceState::Replaced }
    );
    assert_eq!(
        other_device(name(), DeviceState::Active, true),
        OtherDevice::Former { name: name(), state: FormerDeviceState::Removed },
        "a removed person's devices are removed with them"
    );
    assert_eq!(
        other_device(name(), DeviceState::Replaced, true),
        OtherDevice::Former { name: name(), state: FormerDeviceState::Replaced },
        "a replaced device stays replaced when its person is removed"
    );
}

/// A read that names another device as the only one that holds a file names
/// that device as coven registers it, and a read that names this device is a
/// bug in coven that bae reports as one.
#[test]
fn a_read_names_the_device_coven_registers_under_its_id() {
    use crate::test_fixtures::*;
    bae_store::test_devices::run_two_store_test(TEST_SCHEMA, async |devices| {
        devices.converge_rows(|| crate::records::release_detail::RegisteredDevicesParams {}).await;
        let b = devices.b();
        let device = |is_self: bool| async move {
            let row = b
                .read_row(crate::records::release_detail::DeviceWhereSelfParams { is_self })
                .await
                .unwrap();
            (row.device, row.name)
        };
        let (a, name_of_a) = device(false).await;
        let (this, _) = device(true).await;
        let records = test_records(b);

        assert_eq!(records.other_device(a).await.unwrap(), OtherDevice::Active { name: name_of_a });
        let refused = records.other_device(this).await;
        assert!(
            matches!(&refused, Err(OtherDeviceError::ThisDevice(device)) if *device == this),
            "{refused:?}"
        );
    });
}

/// The track statements, the release editor's among them, and the headings of
/// a release no table holds place each track by one rule: a CD or a download
/// among several mediums is a disc, numbered by its position among them, and
/// a record's or cassette's sides get letters across the release's sided
/// mediums.
#[tokio::test]
async fn the_statements_place_tracks_by_one_rule() {
    use crate::test_fixtures::library::RecordsFixture;
    use bae_types::mediums::{ReleaseMedium, TrackPosition, TrackSide};
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

    let (rows, places) = db
        .read(move |sql| {
            let rows = editor_release_on(&sql, release)?.unwrap();
            let places = release_tracks::track_places_on(
                &sql,
                &ids.iter().map(|(id, ..)| id.clone()).collect::<Vec<_>>(),
            )?;
            Ok((rows, places))
        })
        .await
        .unwrap();
    let groups = rows.decoded().unwrap().detail.track_groups;

    let statement =
        crate::records::track::TrackNumbersOfReleaseParams { release: release.to_string() };
    let numbers: Vec<(String, u32)> = db
        .read_rows(statement)
        .await
        .unwrap()
        .into_iter()
        .map(|row| (row.id, row.track_number))
        .collect();
    let sided = |letter: &str| TrackSide::Sided { side_letter: letter.to_owned() };
    let expected_sides = [
        TrackSide::Disc { disc: 1 },
        sided("A"),
        sided("B"),
        TrackSide::Disc { disc: 3 },
        sided("C"),
        sided("D"),
        sided("E"),
        sided("F"),
        sided("G"),
    ];
    let expected: Vec<_> = numbers
        .iter()
        .zip(&expected_sides)
        .map(|((id, number), side)| {
            let number = *number;
            let position = match side.clone() {
                TrackSide::Sided { side_letter } => TrackPosition::Sided { side_letter, number },
                TrackSide::Disc { disc } => TrackPosition::Disc { disc, number },
                TrackSide::Flat => TrackPosition::Flat { number },
            };
            (id.clone(), position)
        })
        .collect();
    let read: Vec<_> = groups
        .iter()
        .flat_map(|group| &group.tracks)
        .map(|track| (track.id.clone(), track.position.clone()))
        .collect();
    assert_eq!(read, expected);
    let sides: Vec<_> = places.iter().map(|place| place.on.clone()).collect();
    assert_eq!(sides, expected_sides);
    let listed: Vec<ReleaseMedium> = carriers
        .iter()
        .map(|(carrier, sides)| ReleaseMedium {
            carrier: Some(*carrier),
            sides: std::num::NonZeroU8::new(*sides).unwrap(),
        })
        .collect();
    let headings = db
        .read(move |sql| release_tracks::side_headings_on(&sql, &listed, &[1, 2, 3, 4, 5]))
        .await
        .unwrap();
    let expected_headings: Vec<_> = expected_sides.iter().filter_map(TrackSide::heading).collect();
    assert_eq!(headings, expected_headings, "a catalog's release names its sides as a stored one");
    #[cfg(desktop)]
    {
        let edited = db
            .read(move |sql| {
                Ok(crate::records::editor_release_on(&sql, release)?.expect("the release"))
            })
            .await
            .unwrap()
            .decoded()
            .unwrap()
            .tracks;
        let edited: Vec<_> =
            edited.into_iter().map(|track| (track.track.id, track.position)).collect();
        assert_eq!(edited, expected);
    }
}

/// The headings of some of a release's mediums count the sides of the
/// mediums before them: the second of two records holds sides C and D, and
/// the second of two CDs is disc 2.
#[tokio::test]
async fn side_headings_of_a_later_medium_count_the_mediums_before_it() {
    use bae_types::mediums::{ReleaseMedium, SideHeading};
    use bae_types::pressing::Medium;
    let (db, _temp) = bae_store::test_fixtures::temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let record =
        ReleaseMedium { carrier: Some(Medium::Vinyl), sides: std::num::NonZeroU8::new(2).unwrap() };
    let cd = ReleaseMedium::one_sided(Some(Medium::Cd));
    let (records, cds) = db
        .read(move |sql| {
            Ok((
                release_tracks::side_headings_on(&sql, &[record, record], &[2])?,
                release_tracks::side_headings_on(&sql, &[cd, cd], &[2])?,
            ))
        })
        .await
        .unwrap();
    let sided = |letter: &str| SideHeading::Sided { side_letter: letter.to_owned() };
    assert_eq!(records, [sided("C"), sided("D")]);
    assert_eq!(cds, [SideHeading::Disc { disc: 2 }]);
}

/// The detail's statement states by key which carriers are played side by
/// side, and it names every carrier `Medium::is_sided` does and no other.
#[tokio::test]
async fn the_detail_letters_the_sides_of_exactly_the_sided_carriers() {
    use crate::test_fixtures::library::RecordsFixture;
    use bae_types::mediums::{ReleaseMedium, TrackSide};
    use bae_types::pressing::Medium;
    let (db, _temp) = bae_store::test_fixtures::temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let release = "5b0c1f2e-7d34-4a8e-9c1b-2f6a4d8e0b01";
    let ids: Vec<String> = Medium::ALL
        .iter()
        .map(|medium| bae_util::test_ids::test_uuid(&format!("carrier-{}", medium.key())))
        .collect();
    let tracks: Vec<(&str, &str)> = ids.iter().map(|id| (id.as_str(), "Track Title")).collect();
    RecordsFixture::new()
        .album("5b0c1f2e-7d34-4a8e-9c1b-2f6a4d8e0b02", "Album Title", None, &[])
        .release("5b0c1f2e-7d34-4a8e-9c1b-2f6a4d8e0b02", release)
        .tracks(release, &tracks)
        .edit(|rows| {
            rows.mediums = Medium::ALL
                .iter()
                .zip(1..)
                .map(|(carrier, position)| {
                    let mut row = crate::DbReleaseMedium::new_test(release);
                    row.id = format!("{release}-medium-{position}");
                    row.position = std::num::NonZeroU16::new(position).unwrap();
                    row.medium = ReleaseMedium::one_sided(Some(*carrier));
                    row
                })
                .collect();
            for (track, medium) in rows.tracks.iter_mut().zip(&rows.mediums) {
                track.medium_id = medium.id.clone();
            }
        })
        .write(&crate::test_fixtures::test_records(&db))
        .await;

    let detail = editor_detail_of(&db, release).await;

    let lettered: Vec<bool> = detail
        .track_groups
        .iter()
        .map(|group| matches!(group.side, TrackSide::Sided { .. }))
        .collect();
    let sided: Vec<bool> = Medium::ALL.iter().map(|medium| medium.is_sided()).collect();
    assert_eq!(lettered, sided);
}

/// The detail's statement states by content type which images the gallery
/// shows: each format a cover can be made from, and no other.
#[tokio::test]
async fn the_detail_shows_exactly_the_images_a_cover_can_be_made_from() {
    use crate::test_fixtures::aggregate_ordering::{insert_file, release_detail_db};
    use crate::test_fixtures::RELEASE_A;
    use bae_util::content_type::ContentType;
    let (db, tmp) = release_detail_db(crate::test_fixtures::TEST_SCHEMA).await;
    let mut types = ContentType::SUPPORTED_COVERS.to_vec();
    types.extend(["image/bmp", "image/tiff", "image/svg+xml"].map(ContentType::from_mime));
    for (position, content_type) in types.iter().enumerate() {
        insert_file(
            &db,
            &tmp,
            crate::NewReleaseFile {
                id: bae_util::test_ids::test_uuid(&format!("shown-file-{position}")),
                release_id: RELEASE_A.to_string(),
                original_filename: format!("picture {position}"),
                content_type: content_type.clone(),
                role: crate::ReleaseFileRole::Other,
                source_audio: None,
                created_at: time::UtcDateTime::now(),
            },
            1,
        )
        .await;
    }

    let detail = editor_detail_of(&db, RELEASE_A).await;

    let shown: Vec<&ContentType> =
        detail.files.images.iter().map(|file| &file.content_type).collect();
    assert_eq!(shown, ContentType::SUPPORTED_COVERS.iter().collect::<Vec<_>>());
}

/// Reads `release`'s detail from `store` and returns the other device it
/// names, or `None` when the release plays here.
async fn elsewhere_of(store: &bae_store::Store, release: &'static str) -> Option<OtherDevice> {
    editor_detail_of(store, release).await.elsewhere
}

/// `release`'s detail as its editor reads it from `store`.
async fn editor_detail_of(store: &bae_store::Store, release: &'static str) -> DbReleaseDetail {
    store
        .read(move |sql| Ok(editor_release_on(&sql, release)?.expect("the release")))
        .await
        .unwrap()
        .decoded()
        .unwrap()
        .detail
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
        let dir = bae_util::test_files::TestFolder::new();
        let (a, b) = (devices.a(), devices.b());
        crate::test_fixtures::library::RecordsFixture::new()
            .album(ALBUM_A, "Album Title", None, &[])
            .release(ALBUM_A, PLAYS_HERE)
            .release(ALBUM_A, ONE_ELSEWHERE)
            .release(ALBUM_A, ARTWORK_ELSEWHERE)
            .release(ALBUM_A, TWO_ELSEWHERE)
            .write(&test_records(a))
            .await;
        devices.converge_rows(|| crate::records::release::ReleaseIdsParams {}).await;

        storage_listing::import_file(b, &dir, PLAYS_HERE, "here.flac", Flac).await;
        storage_listing::import_file(a, &dir, ONE_ELSEWHERE, "one.flac", Flac).await;
        storage_listing::import_file(a, &dir, ONE_ELSEWHERE, "cover.jpg", Jpeg).await;
        storage_listing::import_file(a, &dir, ARTWORK_ELSEWHERE, "front.jpg", Jpeg).await;
        storage_listing::import_file(a, &dir, TWO_ELSEWHERE, "b-second.flac", Flac).await;
        storage_listing::import_file(a, &dir, TWO_ELSEWHERE, "a-first.flac", Flac).await;
        devices.converge_rows(|| crate::records::files::ReleaseFileRowsParams {}).await;
        devices.converge_rows(|| crate::records::files::CovenFileIdsParams {}).await;

        let other = crate::records::release_detail::DeviceWhereSelfParams { is_self: false };
        let name_of_a = b.read_row(other).await.unwrap().name;
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

/// The release detail fails to read, and names the device, when the release's
/// storage row names a device that coven doesn't register or names this
/// device, instead of reading as a release that plays here.
#[tokio::test]
async fn a_stored_device_coven_does_not_register_or_this_device_fails_the_read() {
    use crate::test_fixtures::*;
    let (store, dir) = bae_store::test_fixtures::temp_db(TEST_SCHEMA).await;
    library::RecordsFixture::new()
        .album(ALBUM_A, "Album Title", None, &[])
        .release(ALBUM_A, RELEASE_A)
        .write(&test_records(&store))
        .await;
    let detail_error = || async {
        store
            .read(|sql| Ok(editor_release_on(&sql, RELEASE_A)?.expect("the release")))
            .await
            .unwrap()
            .decoded()
            .err()
            .expect("the read fails")
            .to_string()
    };

    let unregistered = coven::DeviceId(0x0123_4567_89ab_cdef);
    crate::release_storage::plant_audio_elsewhere_device_for_test(
        dir.path(),
        RELEASE_A,
        unregistered,
    );
    let error = detail_error().await;
    assert!(error.contains("0123456789ABCDEF, which coven doesn't register"), "{error}");

    crate::release_storage::plant_audio_elsewhere_device_for_test(
        dir.path(),
        RELEASE_A,
        store.device_id(),
    );
    let error = detail_error().await;
    assert!(error.contains("names this device"), "{error}");
}
