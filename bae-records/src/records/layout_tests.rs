use super::*;
use crate::test_fixtures::{ALBUM_A, RELEASE_A};
use bae_store::Store;
use bae_types::mediums::ReleaseMedium;
use std::num::{NonZeroU16, NonZeroU32, NonZeroU8};

/// A release on the real schema with `mediums` (each its side count) and the
/// tracks `(id, medium, side, number)` in playing order, and CUE sheets
/// carving the tracks `(sheet, track id)`, each sheet a file of the release.
/// Built through the records' own writes. The folder holds the sheets'
/// originals.
async fn release(
    mediums: &[u8],
    tracks: &[(&str, u16, u8, u32)],
    carved: &[(&str, &str)],
) -> (Store, bae_util::test_files::TestFolder) {
    let (db, folder) = bae_store::test_fixtures::temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let records = crate::test_fixtures::test_records(&db);
    crate::test_fixtures::album_with_releases(&records, ALBUM_A, "Album", &[RELEASE_A]).await;
    for (index, sides) in mediums.iter().enumerate() {
        let position = u16::try_from(index + 1).unwrap();
        records
            .insert_release_medium(&DbReleaseMedium {
                id: medium_id(position),
                release_id: RELEASE_A.to_string(),
                position: NonZeroU16::new(position).unwrap(),
                medium: ReleaseMedium {
                    carrier: None,
                    sides: NonZeroU8::new(*sides).expect("a medium has a side"),
                },
                created_at: time::UtcDateTime::now(),
            })
            .await
            .unwrap();
    }
    for (id, medium, side, number) in tracks {
        let track = DbTrack {
            medium_id: medium_id(*medium),
            side: NonZeroU8::new(*side).unwrap(),
            track_number: NonZeroU32::new(*number).unwrap(),
            ..DbTrack::new_test(RELEASE_A, &uuid(id), id, *number)
        };
        records.insert_track(&track).await.unwrap();
    }
    let mut sheets: Vec<&str> = carved.iter().map(|(sheet, _)| *sheet).collect();
    sheets.dedup();
    for sheet in sheets {
        crate::test_fixtures::storage_listing::insert_file(
            &db,
            &folder,
            (&uuid(sheet), RELEASE_A, &format!("{sheet}.cue"), 1),
        )
        .await;
    }
    for (sheet, track) in carved.iter().map(|(sheet, track)| (uuid(sheet), uuid(track))) {
        db.call_sql(move |sql| write::insert_carved_track_row(&sql, &track, &sheet, "image.flac"))
            .await
            .unwrap();
    }
    (db, folder)
}

fn medium_id(position: u16) -> String {
    uuid(&format!("medium-{position}"))
}

/// The id a test names `label` by: the stores take UUIDs.
fn uuid(label: &str) -> String {
    bae_util::test_ids::test_uuid(label)
}

/// The tracks as the layout places them for the discs picked: each with its
/// medium, side and number, in playing order, and how many mediums remain.
async fn laid(db: &Store, picks: &[(&str, u32)]) -> (Vec<(String, u16, u8, u32)>, usize) {
    let moves = serde_json::to_string(
        &picks
            .iter()
            .map(|(sheet, disc)| serde_json::json!({ "sheet_file_id": uuid(sheet), "disc": disc }))
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let layout = db
        .read(move |sql| {
            laid_out_on(
                &sql,
                &LayoutParams {
                    release_id: RELEASE_A,
                    applied: false,
                    mediums: "[]",
                    places: "[]",
                    moves: &moves,
                },
            )
        })
        .await
        .unwrap();
    let places = layout
        .tracks
        .iter()
        .map(|track| {
            (
                track.track_id.clone(),
                track.place.medium.get(),
                track.place.side.get(),
                track.place.number.get(),
            )
        })
        .collect();
    (places, layout.mediums.len())
}

fn place(id: &str, medium: u16, side: u8, number: u32) -> (String, u16, u8, u32) {
    (uuid(id), medium, side, number)
}

/// A sheet's tracks go after the tracks of the disc it moves to, numbered on
/// from them, and the disc it left is gone.
#[tokio::test]
async fn a_sheet_moves_after_the_tracks_of_its_new_disc() {
    let (db, _folder) = release(
        &[1, 1],
        &[("a1", 1, 1, 1), ("a2", 1, 1, 2), ("b1", 2, 1, 1), ("b2", 2, 1, 2)],
        &[("sheet-a", "a1"), ("sheet-a", "a2")],
    )
    .await;
    let (places, mediums) = laid(&db, &[("sheet-a", 2)]).await;
    assert_eq!(
        places,
        [place("b1", 1, 1, 1), place("b2", 1, 1, 2), place("a1", 1, 1, 3), place("a2", 1, 1, 4)]
    );
    assert_eq!(mediums, 1);
}

/// A number the disc's tracks leave a gap in is not taken again: the moved
/// tracks number from one more than the highest.
#[tokio::test]
async fn moved_tracks_number_past_the_highest_number_of_the_side() {
    let (db, _folder) = release(
        &[1, 1],
        &[("a1", 1, 1, 1), ("b1", 2, 1, 1), ("b2", 2, 1, 5)],
        &[("sheet-a", "a1")],
    )
    .await;
    let (places, _) = laid(&db, &[("sheet-a", 2)]).await;
    assert_eq!(places, [place("b1", 1, 1, 1), place("b2", 1, 1, 5), place("a1", 1, 1, 6)]);
}

/// A medium between others that loses its tracks goes, and the mediums
/// after it move up.
#[tokio::test]
async fn the_mediums_after_an_emptied_one_move_up() {
    let (db, _folder) =
        release(&[1, 1, 1], &[("a", 1, 1, 1), ("b", 2, 1, 1), ("c", 3, 1, 1)], &[("sheet-b", "b")])
            .await;
    let (places, mediums) = laid(&db, &[("sheet-b", 1)]).await;
    assert_eq!(places, [place("a", 1, 1, 1), place("b", 1, 1, 2), place("c", 2, 1, 1)]);
    assert_eq!(mediums, 2);
}

/// A pick of a disc the release lacks, and of the disc a sheet is on
/// already, change nothing, not even the order of the tracks.
#[tokio::test]
async fn a_move_that_goes_nowhere_leaves_the_layout_alone() {
    let (db, _folder) =
        release(&[1, 1], &[("b1", 2, 1, 1), ("a1", 1, 1, 1)], &[("sheet-a", "a1")]).await;
    let stored = laid(&db, &[]).await;
    for disc in [0, 1, 3, 70_000] {
        assert_eq!(laid(&db, &[("sheet-a", disc)]).await, stored, "a pick of disc {disc}");
    }
}

/// A medium played side by side takes the moved tracks on its last side.
#[tokio::test]
async fn a_sided_medium_takes_the_tracks_on_its_last_side() {
    let (db, _folder) =
        release(&[1, 2], &[("a", 1, 1, 1), ("s1", 2, 1, 1), ("s2", 2, 2, 1)], &[("sheet-a", "a")])
            .await;
    let (places, _) = laid(&db, &[("sheet-a", 2)]).await;
    assert_eq!(places, [place("s1", 1, 1, 1), place("s2", 1, 2, 1), place("a", 1, 2, 2)]);
}

/// Two sheets going to one disc keep the order of their first tracks.
#[tokio::test]
async fn two_sheets_on_one_disc_keep_their_order() {
    let (db, _folder) = release(
        &[1, 1, 1],
        &[("a", 1, 1, 1), ("b", 2, 1, 1), ("c", 3, 1, 1)],
        &[("sheet-a", "a"), ("sheet-b", "b")],
    )
    .await;
    let (places, mediums) = laid(&db, &[("sheet-b", 3), ("sheet-a", 3)]).await;
    assert_eq!(places, [place("c", 1, 1, 1), place("a", 1, 1, 2), place("b", 1, 1, 3)]);
    assert_eq!(mediums, 1);
}
