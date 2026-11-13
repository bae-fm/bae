//! What this device imported places: a release on Done, dated by when its
//! release was written, and what its Done row shows of its library album:
//! the album's title, its credits and the names they print, each moved by
//! the write that changes it in the library.

use crate::tables::import_list::placement::tests::*;
use crate::tables::import_list::tests::*;
use bae_records::test_fixtures::artists_apart::{
    meet_musicbrainz_artist, merge_apart, musicbrainz_artist,
};
use bae_records::test_fixtures::test_records;

/// What this device imported from a folder's files puts it on Done, dated by
/// when its release was written and showing its library album, whatever else
/// is stored for it; the release deleted from the library puts it back.
#[tokio::test]
async fn an_imported_release_is_done_dated_by_its_release() {
    let lib = Library::new().await;
    lib.run(lib.scan_dated("Album", 100) + lib.skip("Album")).await;
    let imported_at = 1_767_225_600_000;
    assert_eq!(
        lib.moved(lib.imported("Album", "2026-01-01T00:00:00.123456789+00:00")).await,
        moves([(
            lib.release("Album"),
            Some(found("Album").on("done").dated(imported_at + 123).shown("Album Title", Some("")))
        )])
    );
    assert_eq!(
        lib.moved(lib.release_deleted("Album")).await,
        moves([(lib.release("Album"), Some(found("Album").on("skipped").dated(100)))])
    );
}

/// The MusicBrainz artist the album is first credited to.
const ARTIST_A_MUSICBRAINZ: &str = "artist-a-mbid";

/// The MusicBrainz artist the album's second credit names.
const ARTIST_B_MUSICBRAINZ: &str = "artist-b-mbid";

/// A MusicBrainz artist whose library id sorts before `than`'s, which a
/// merge of the two keeps.
fn surviving_musicbrainz(than: &str) -> String {
    let id = musicbrainz_artist(than);
    (0..)
        .map(|n| format!("survivor-mbid-{n}"))
        .find(|candidate| musicbrainz_artist(candidate) < id)
        .expect("some id sorts first")
}

/// The name each device's library holds for artist `artist_id`.
async fn artist_name(db: &bae_store::Store, artist_id: &str) -> String {
    test_records(db)
        .find_artist_by_id(artist_id)
        .await
        .unwrap()
        .expect("the library holds the artist")
        .name
}

/// The Done row follows its library album: retitling it, its credits coming,
/// going and changing, the artists they name being renamed or merged by
/// another device, the credit names they print, and the release moving to
/// another album. A release nobody imported moves for none of it.
#[test]
fn a_done_row_follows_its_library_album() {
    bae_store::test_devices::run_two_store_test(crate::TEST_SCHEMA, async |devices| {
        devices.pause().await;
        let lib = Library::on(devices.a().clone()).await;
        let records = lib.records();
        meet_musicbrainz_artist(&records, ARTIST_A_MUSICBRAINZ, "Artist A", None).await;
        meet_musicbrainz_artist(&records, ARTIST_B_MUSICBRAINZ, "Artist B", None).await;
        let (artist_a, artist_b) =
            (musicbrainz_artist(ARTIST_A_MUSICBRAINZ), musicbrainz_artist(ARTIST_B_MUSICBRAINZ));
        lib.run(
            lib.scan_dated("Album", 100)
                + lib.draft("Album", "Draft Title")
                + lib.imported("Album", "2026-01-01T00:00:00Z")
                + lib.scan_dated("Other", 100),
        )
        .await;
        let done = |title: &str, artists: &str| {
            found("Album").on("done").dated(1_767_225_600_000).shown(title, Some(artists))
        };
        assert_eq!(
            lib.placed().await.get(&lib.release("Album")),
            Some(&done("Album Title", "")),
            "the library album's title, not the draft's"
        );
        let moved_to = |title: &str, artists: &str| {
            moves([(lib.release("Album"), Some(done(title, artists)))])
        };
        let release_id = lib.release_id("Album");
        let album_id = records.find_release_by_id(&release_id).await.unwrap().unwrap().album_id;
        let credit = |artist: &str, position: u32, credit: Option<&str>, join: Option<&str>| {
            bae_records::DbAlbumArtist {
                credit: credit.map(bae_records::credits::CreditName::of),
                join_phrase: join.map(str::to_string),
                ..bae_records::DbAlbumArtist::new(
                    &coven::UuidIds,
                    &album_id,
                    artist,
                    position,
                    bae_store::test_fixtures::fixed_now(),
                )
            }
        };
        let credited = |credits: Vec<bae_records::DbAlbumArtist>| {
            let records = records.clone();
            let release_id = release_id.clone();
            async move {
                let mut artists = Vec::new();
                for credit in &credits {
                    artists.push(
                        records
                            .find_artist_by_id(&credit.artist_id)
                            .await
                            .unwrap()
                            .expect("the library holds the credited artist"),
                    );
                }
                records
                    .save_release_metadata_for_test(&release_id, |rows| {
                        for artist in artists {
                            if !rows.picked.contains(&artist.id) {
                                rows.picked.push(artist.id.clone());
                            }
                            if !rows.credits.iter().any(|credit| credit.id == artist.id) {
                                rows.credits.push(artist);
                            }
                        }
                        rows.album_artists = credits;
                    })
                    .await
                    .unwrap()
            }
        };
        assert_eq!(
            lib.moved_after(async {
                records.rename_album_for_test(&album_id, "Retitled").await.unwrap()
            })
            .await,
            moved_to("Retitled", "")
        );
        assert_eq!(
            lib.moved_after(credited(vec![credit(&artist_a, 0, None, Some(" & "))])).await,
            moved_to("Retitled", "Artist A")
        );
        assert_eq!(
            lib.moved_after(credited(vec![
                credit(&artist_a, 0, None, Some(" & ")),
                credit(&artist_b, 1, None, None),
            ]))
            .await,
            moved_to("Retitled", "Artist A, Artist B")
        );
        let entered = |name: &str| bae_store::EnteredName::parse(name).unwrap();
        assert_eq!(
            lib.moved_after(async {
                records.add_artist_credit_name(&artist_b, &entered("Printed B")).await.unwrap()
            })
            .await,
            moves([]),
            "a credit name no credit prints shows nowhere"
        );
        assert_eq!(
            lib.moved_after(credited(vec![
                credit(&artist_a, 0, None, Some(" & ")),
                credit(&artist_b, 1, Some("Printed B"), None),
            ]))
            .await,
            moved_to("Retitled", "Artist A, Printed B")
        );
        assert_eq!(
            lib.moved_after(async {
                records
                    .rename_artist_credit_name(
                        &artist_b,
                        &bae_records::credits::CreditName::of("Printed B").key,
                        &entered("Printed Bee"),
                    )
                    .await
                    .unwrap()
            })
            .await,
            moved_to("Retitled", "Artist A, Printed Bee")
        );
        // B, apart, meets artist A under another name; the later write's
        // name lands on A.
        meet_musicbrainz_artist(
            &test_records(devices.b()),
            ARTIST_A_MUSICBRAINZ,
            "Renamed A",
            None,
        )
        .await;
        assert_eq!(
            lib.moved_after(async {
                devices.resume().await;
                devices
                    .settle("artist A's new name", async |_, db| {
                        artist_name(db, &musicbrainz_artist(ARTIST_A_MUSICBRAINZ)).await
                            == "Renamed A"
                    })
                    .await;
            })
            .await,
            moved_to("Retitled", "Renamed A, Printed Bee")
        );
        // Each device gives one artist the same Discogs id while apart: the
        // one B holds, whose id sorts first, survives the merge.
        let survivor_musicbrainz = surviving_musicbrainz(ARTIST_A_MUSICBRAINZ);
        meet_musicbrainz_artist(
            &test_records(devices.b()),
            &survivor_musicbrainz,
            "Survivor",
            None,
        )
        .await;
        assert_eq!(
            lib.moved_after(merge_apart(devices, ("Renamed A", "Survivor"), "discogs-merged"))
                .await,
            moved_to("Retitled", "Survivor, Printed Bee"),
            "a merged artist prints as the artist it shows as"
        );
        assert_eq!(
            lib.moved_after(credited(vec![credit(&artist_a, 0, None, None)])).await,
            moved_to("Retitled", "Survivor")
        );
        let other_album = bae_util::test_ids::test_uuid("other album");
        assert_eq!(
            lib.moved_after(async {
                records
                    .set_records_atomic(bae_records::RecordsChange {
                        release_id: release_id.clone(),
                        records: Vec::new(),
                        current_album_id: album_id.clone(),
                        target_album_id: other_album.clone(),
                        new_album: Some(bae_records::DbAlbum {
                            id: other_album.clone(),
                            title: "Other Album".to_string(),
                            year: None,
                            is_compilation: false,
                            created_at: bae_store::test_fixtures::fixed_now(),
                        }),
                    })
                    .await
                    .unwrap()
            })
            .await,
            moved_to("Other Album", "Survivor"),
            "the album it lands on takes its credits"
        );
    });
}
