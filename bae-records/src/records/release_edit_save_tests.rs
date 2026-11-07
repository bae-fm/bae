//! Saving an edit session's applied rows: refused before anything is
//! written when a track they rewrite is no longer the release's.

use super::*;
use crate::test_fixtures::{ALBUM_A, RELEASE_A, TRACK_A, TRACK_B};

/// Applied rows rewriting a track the release no longer holds, as one
/// another device's write took after the session was checked, refuse the
/// save as that track gone, and the release keeps its title.
#[tokio::test]
async fn applied_rows_over_a_track_gone_refuse_the_save() {
    let (db, _temp) = bae_store::test_fixtures::temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let records = crate::test_fixtures::test_records(&db);
    crate::test_fixtures::album_with_tracks(
        &records,
        (ALBUM_A, "Album Title"),
        RELEASE_A,
        &[(TRACK_A, "Track Title")],
    )
    .await;
    let album = records.find_album_by_id(ALBUM_A).await.unwrap().unwrap();
    let release = records.find_release_by_id(RELEASE_A).await.unwrap().unwrap();
    let rows = ReleaseRows {
        album: DbAlbum { title: "Renamed".to_string(), ..album },
        release,
        mediums: Vec::new(),
        tracks: vec![TrackRow {
            id: TRACK_B.to_string(),
            release_id: RELEASE_A.to_string(),
            title: "Gone".to_string(),
            medium_id: "medium".to_string(),
            side: std::num::NonZeroU8::MIN,
            track_number: std::num::NonZeroU32::MIN,
            created_at: chrono::Utc::now(),
        }],
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

    let refused = records
        .save_release_edit_atomic(
            None,
            ReleaseEditWrite::Applied { rows, picked: Vec::new() },
            AlbumFields::Written,
        )
        .await;

    assert!(
        matches!(
            &refused,
            Err(ReleaseEditSaveError::TrackGone { release_id, track_id })
                if release_id == RELEASE_A && track_id == TRACK_B
        ),
        "{refused:?}"
    );
    assert_eq!(records.find_album_by_id(ALBUM_A).await.unwrap().unwrap().title, "Album Title");
}
