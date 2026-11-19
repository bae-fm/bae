//! Saving an editor's form: refused, with nothing written, when the release
//! is not as the editor opened on it, as the save's own write finds it; and
//! a picked artist the library no longer holds saved as the editor shows it.

use super::*;
use crate::test_fixtures::{ALBUM_A, RELEASE_A, TRACK_A};

/// A write that lands after an editor opened makes its save refuse, and the
/// refusal is decided in the save's write: the check reads the release there,
/// so it sees the write that landed, and the release keeps what that write
/// left. A save whose check finds the release as it stands goes through.
#[tokio::test]
async fn a_save_is_refused_once_the_release_changed_after_the_editor_opened() {
    let (db, _temp) = bae_store::test_fixtures::temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let records = crate::test_fixtures::test_records(&db);
    crate::test_fixtures::album_with_tracks(
        &records,
        (ALBUM_A, "Album Title"),
        RELEASE_A,
        &[(TRACK_A, "Track Title")],
    )
    .await;
    // What the editor opened on.
    let opened = records.find_album_by_id(ALBUM_A).await.unwrap().unwrap().title;
    records.rename_album_for_test(ALBUM_A, "Written Elsewhere").await.unwrap();

    let album_title_is = |expected: &str| {
        let expected = expected.to_string();
        move |sql: &CovenWrite<'_>| {
            let title: String =
                sql.query_row("SELECT title FROM albums WHERE id = ?", [ALBUM_A], |row| {
                    row.get(0)
                })?;
            Ok(if title == expected { Ok(()) } else { Err(ReleaseEditRefusal::Stale) })
        }
    };
    let save = |check| async {
        let album = records.find_album_by_id(ALBUM_A).await.unwrap().unwrap();
        let release = records.find_release_by_id(RELEASE_A).await.unwrap().unwrap();
        let rows = ReleaseRows {
            album: DbAlbum { title: "Renamed".to_string(), ..album },
            release,
            mediums: records.get_release_mediums(RELEASE_A).await.unwrap().0,
            tracks: Vec::new(),
            artists: Vec::new(),
            album_artists: Vec::new(),
            track_artists: Vec::new(),
            work_graph: WorkGraphRows {
                works: Vec::new(),
                work_artists: Vec::new(),
                work_parts: Vec::new(),
                track_works: Vec::new(),
            },
            release_artist_roles: Vec::new(),
            track_artist_roles: Vec::new(),
        };
        records
            .save_release_edit_atomic(
                ReleaseEditPlan {
                    change: None,
                    write: ReleaseEditWrite::Applied { rows, picked: Vec::new() },
                    album_fields: AlbumFields::Written,
                },
                GonePicks::Refuse,
                check,
                |_| Ok(()),
            )
            .await
    };

    let refused = save(album_title_is(&opened)).await;
    assert!(matches!(&refused, Err(WriteError::Refused(ReleaseEditRefusal::Stale))), "{refused:?}");
    assert_eq!(
        records.find_album_by_id(ALBUM_A).await.unwrap().unwrap().title,
        "Written Elsewhere"
    );

    save(album_title_is("Written Elsewhere")).await.expect("the release is as the check found it");
    assert_eq!(records.find_album_by_id(ALBUM_A).await.unwrap().unwrap().title, "Renamed");
}

/// The id of an artist a form picked that the library no longer holds.
const GONE_ARTIST: &str = "0b8e2f4a-3c6d-4e1f-9a7b-5d2c8e1f4a3b";

/// An editor's fields-only save of release A crediting the album to the
/// library artist `GONE_ARTIST` the form picked, printed as `credit_name`,
/// with the values the form holds for it.
async fn save_crediting_a_gone_artist(
    records: &Records,
    credit_name: Option<&str>,
    gone: GonePicks,
) -> Result<(), ReleaseEditSaveError> {
    let mut metadata = records.release_metadata_for_test(RELEASE_A).await.unwrap();
    let now = time::UtcDateTime::now();
    metadata.credits = vec![DbArtist {
        id: GONE_ARTIST.to_string(),
        name: "Departed Artist".to_string(),
        sort_name: Some("Artist, Departed".to_string()),
        discogs_artist_id: None,
        musicbrainz_artist_id: None,
        created_at: now,
    }];
    metadata.picked = vec![GONE_ARTIST.to_string()];
    metadata.album_artists = vec![DbAlbumArtist {
        credit: credit_name.map(crate::credits::CreditName::of),
        ..DbAlbumArtist::new(&coven::UuidIds, ALBUM_A, GONE_ARTIST, 0, now)
    }];
    records
        .save_release_edit_atomic(
            ReleaseEditPlan {
                change: None,
                write: ReleaseEditWrite::Fields(metadata),
                album_fields: AlbumFields::Written,
            },
            gone,
            |_| Ok(Ok(())),
            |_| Ok(()),
        )
        .await
}

/// A save writes what its editor shows: an artist the form picked that the
/// library no longer holds is a credit of the values the form holds for it,
/// which the write decides as it does a typed name, here a new artist of
/// that name, under the id the form named, printed as the form printed it.
#[tokio::test]
async fn a_picked_artist_the_library_no_longer_holds_saves_as_a_credit() {
    let (db, _temp) = bae_store::test_fixtures::temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let records = crate::test_fixtures::test_records(&db);
    crate::test_fixtures::album_with_tracks(
        &records,
        (ALBUM_A, "Album Title"),
        RELEASE_A,
        &[(TRACK_A, "Track Title")],
    )
    .await;

    save_crediting_a_gone_artist(
        &records,
        Some("Departed Alias"),
        GonePicks::Unpick { unpicked: None },
    )
    .await
    .expect("the save writes the credit");

    let credits = records.get_artists_for_album(ALBUM_A).await.unwrap();
    let [credit] = credits.as_slice() else { panic!("one album credit: {credits:?}") };
    assert_eq!(credit.artist.id, GONE_ARTIST, "a new artist under the id the form named");
    assert_eq!(credit.artist.name, "Departed Artist");
    assert_eq!(credit.artist.sort_name.as_deref(), Some("Artist, Departed"));
    assert_eq!(credit.printed(), "Departed Alias", "printed as the form printed it");
}

/// A save whose caller named the artist itself refuses it once the library
/// no longer holds it, and writes nothing.
#[tokio::test]
async fn a_save_that_refuses_gone_picks_refuses_a_picked_artist_the_library_no_longer_holds() {
    let (db, _temp) = bae_store::test_fixtures::temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let records = crate::test_fixtures::test_records(&db);
    crate::test_fixtures::album_with_tracks(
        &records,
        (ALBUM_A, "Album Title"),
        RELEASE_A,
        &[(TRACK_A, "Track Title")],
    )
    .await;
    let before = records.get_artists_for_album(ALBUM_A).await.unwrap();

    let refused = save_crediting_a_gone_artist(&records, None, GonePicks::Refuse).await;

    assert!(
        matches!(
            &refused,
            Err(WriteError::Refused(ReleaseEditRefusal::Identity(IdentityRefusal::ArtistGone(id))))
                if id == GONE_ARTIST
        ),
        "{refused:?}"
    );
    assert_eq!(records.get_artists_for_album(ALBUM_A).await.unwrap(), before);
}
