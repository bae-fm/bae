//! Saving an edit session: refused, with nothing written, when the release
//! is not as the session opened on it, as the save's own write finds it.

use super::*;
use crate::test_fixtures::{ALBUM_A, RELEASE_A, TRACK_A};

/// A write that lands after a session opened makes its save refuse, and the
/// refusal is decided in the save's write: the check reads the release there,
/// so it sees the write that landed, and the release keeps what that write
/// left. A save whose check finds the release as it stands goes through.
#[tokio::test]
async fn a_save_is_refused_once_the_release_changed_after_the_session_opened() {
    let (db, _temp) = bae_store::test_fixtures::temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let records = crate::test_fixtures::test_records(&db);
    crate::test_fixtures::album_with_tracks(
        &records,
        (ALBUM_A, "Album Title"),
        RELEASE_A,
        &[(TRACK_A, "Track Title")],
    )
    .await;
    // What the session opened on.
    let opened = records.find_album_by_id(ALBUM_A).await.unwrap().unwrap().title;
    records.rename_album_for_test(ALBUM_A, "Written Elsewhere").await.unwrap();

    let album_title_is = |expected: &str| {
        let expected = expected.to_string();
        move |sql: &CovenWrite<'_>| {
            let title: String =
                sql.query_row("SELECT title FROM albums WHERE id = ?", [ALBUM_A], |row| {
                    row.get(0)
                })?;
            Ok(title == expected)
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
                None,
                ReleaseEditWrite::Applied { rows, picked: Vec::new() },
                AlbumFields::Written,
                check,
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
