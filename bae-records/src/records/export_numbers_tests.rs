use super::*;
use crate::test_fixtures::library::RecordsFixture;
use bae_types::mediums::ReleaseMedium;
use bae_types::pressing::Medium;

const RELEASE: &str = "0d6f3b52-7c41-4f0e-9a1b-2c3d4e5f6a01";

/// The export numbers of a release whose tracks sit, in playing order, on
/// the places `places` ((medium, side, number on the side)) of `records`
/// two-sided records.
async fn numbers_of(records: u16, places: &[(u16, u8, u32)]) -> Vec<(u32, Option<u32>)> {
    let (db, _temp) = bae_store::test_fixtures::temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let ids: Vec<String> = (0..places.len())
        .map(|index| bae_util::test_ids::test_uuid(&format!("export-track-{index}")))
        .collect();
    let tracks: Vec<(&str, &str)> = ids.iter().map(|id| (id.as_str(), "Track Title")).collect();
    let medium_id = |position: u16| format!("{RELEASE}-medium-{position}");
    RecordsFixture::new()
        .artist("0d6f3b52-7c41-4f0e-9a1b-2c3d4e5f6a02", "Artist Name")
        .album(
            "0d6f3b52-7c41-4f0e-9a1b-2c3d4e5f6a03",
            "Album Title",
            None,
            &["0d6f3b52-7c41-4f0e-9a1b-2c3d4e5f6a02"],
        )
        .release("0d6f3b52-7c41-4f0e-9a1b-2c3d4e5f6a03", RELEASE)
        .tracks(RELEASE, &tracks)
        .edit(|rows| {
            rows.mediums = (1..=records)
                .map(|position| {
                    let mut row = crate::DbReleaseMedium::new_test(RELEASE);
                    row.id = medium_id(position);
                    row.position = std::num::NonZeroU16::new(position).unwrap();
                    row.medium = ReleaseMedium {
                        carrier: Some(Medium::Vinyl),
                        sides: std::num::NonZeroU8::new(2).unwrap(),
                    };
                    row
                })
                .collect();
            for (track, (medium, side, number)) in rows.tracks.iter_mut().zip(places) {
                track.medium_id = medium_id(*medium);
                track.side = std::num::NonZeroU8::new(*side).unwrap();
                track.track_number = std::num::NonZeroU32::new(*number).unwrap();
            }
        })
        .write(&crate::test_fixtures::test_records(&db))
        .await;
    let numbered = db.read(|sql| numbered_tracks_of_release_on(&sql, RELEASE)).await.unwrap();
    for track in &numbered {
        let id = track.track.id.clone();
        let one = db.read(move |sql| numbered_track_on(&sql, &id)).await.unwrap();
        assert_eq!(one.as_ref(), Some(track), "one track reads as its release's list has it");
    }
    assert_eq!(
        numbered.iter().map(|track| track.track.id.clone()).collect::<Vec<_>>(),
        ids,
        "the release's tracks come in playing order"
    );
    numbered.into_iter().map(|track| (track.number, track.disc)).collect()
}

/// A single record's tracks number on across its two sides, so side B's
/// first track is not a second track 1, and the file states no disc.
#[tokio::test]
async fn one_record_numbers_its_tracks_across_both_sides() {
    let numbers = numbers_of(1, &[(1, 1, 1), (1, 1, 2), (1, 2, 1), (1, 2, 2)]).await;
    assert_eq!(numbers, [(1, None), (2, None), (3, None), (4, None)]);
}

/// Each record of a set numbers its own tracks from one, under its
/// position as the disc.
#[tokio::test]
async fn each_record_of_a_set_numbers_its_own_tracks_as_a_disc() {
    let numbers = numbers_of(2, &[(1, 1, 1), (1, 2, 1), (2, 1, 1), (2, 2, 1), (2, 2, 2)]).await;
    assert_eq!(numbers, [(1, Some(1)), (2, Some(1)), (1, Some(2)), (2, Some(2)), (3, Some(2))]);
}
