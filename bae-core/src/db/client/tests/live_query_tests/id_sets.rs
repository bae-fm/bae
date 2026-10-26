//! Live queries whose request is a set of ids the UI holds — the releases an
//! import pane offers, the albums the grid has selected — moved in place as
//! the set changes.

use super::*;

/// The statuses of every offered release come from one subscription: its
/// checks move in place as the offers change, and a record write wakes the
/// one read that answers them all.
#[tokio::test]
async fn library_statuses_follow_the_offered_releases_on_one_subscription() {
    let (db, _temp) = live_db().await;
    let offered = LibraryCheck {
        release_id: "source-release-1".to_string(),
        source: Catalog::MusicBrainz,
        source_group_id: Some("source-group-1".to_string()),
    };
    let other = LibraryCheck {
        release_id: "source-release-2".to_string(),
        source: Catalog::MusicBrainz,
        source_group_id: None,
    };
    let mut live = db.subscribe_library_statuses(BTreeSet::new());
    let requests = live.requests();
    assert!(
        live.next().await.result.unwrap().is_empty(),
        "no offers check nothing"
    );

    requests
        .set([offered.clone(), other.clone()].into_iter().collect())
        .unwrap();
    let initial = live.next().await.result.unwrap();
    assert_eq!(initial.len(), 2);
    assert!(initial.iter().all(|status| !status.album_in_library()));

    exec(
        &db,
        "INSERT INTO release_records
         (id, release_id, catalog, kind, key, album_key, url, created_at) VALUES (?1, ?2, 'musicbrainz', 'pressing', 'source-release-1', 'source-group-1', 'https://musicbrainz.org/release/source-release-1', '2026-01-01T00:00:00Z')",
        &[IDENTITY_ID, RELEASE_ID],
    )
    .await;
    let updated = tokio::time::timeout(Duration::from_secs(2), live.next())
        .await
        .expect("identity write wakes the library statuses")
        .result
        .unwrap();
    let status = |statuses: &[LibraryStatus], release_id: &str| {
        statuses
            .iter()
            .find(|status| status.release_id == release_id)
            .cloned()
            .expect("a status per check")
    };
    let held = status(&updated, "source-release-1");
    assert!(held.release_in_library());
    assert!(held.album_in_library());
    assert_eq!(held.albums[0].album_id, ALBUM_ID);
    assert!(!status(&updated, "source-release-2").album_in_library());

    exec(&db,
        "UPDATE release_records SET kind = 'album', key = 'source-group-1', album_key = NULL, url = 'https://musicbrainz.org/release-group/source-group-1' WHERE id = ?1",
        &[IDENTITY_ID],
    ).await;
    let album_only = tokio::time::timeout(Duration::from_secs(2), live.next())
        .await
        .expect("identity kind change wakes the library statuses")
        .result
        .unwrap();
    let held = status(&album_only, "source-release-1");
    assert!(!held.release_in_library());
    assert!(held.album_in_library());

    requests.set([other].into_iter().collect()).unwrap();
    let narrowed = live.next().await.result.unwrap();
    assert_eq!(
        narrowed
            .iter()
            .map(|status| status.release_id.as_str())
            .collect::<Vec<_>>(),
        vec!["source-release-2"],
        "a release no longer offered is no longer checked"
    );
}

/// The grid's selection reads through one subscription: its ids move in
/// place, it answers with the selected ids that name no album in the library,
/// and deleting a selected album wakes the read that shows it gone.
#[tokio::test]
async fn album_selection_follows_its_ids_on_one_subscription() {
    let (db, _temp) = live_db().await;
    let mut live = db.subscribe_album_selection(BTreeSet::new());
    let requests = live.requests();
    assert!(live.next().await.result.unwrap().is_empty());

    requests
        .set(
            [ALBUM_ID.to_string(), "missing-album".to_string()]
                .into_iter()
                .collect(),
        )
        .unwrap();
    let selected = live.next().await;
    assert_eq!(selected.request.len(), 2);
    assert_eq!(selected.result.unwrap(), vec!["missing-album"]);

    exec(&db, "DELETE FROM albums WHERE id = ?1", &[ALBUM_ID]).await;
    let deleted = tokio::time::timeout(Duration::from_secs(2), live.next())
        .await
        .expect("deleting a selected album wakes the selection")
        .result
        .unwrap();
    assert_eq!(deleted, vec![ALBUM_ID, "missing-album"]);
}

/// Each check answers with the albums holding its pressing first, then the
/// albums holding another release of its album, oldest first; a check with
/// no album answers only by its pressing; and the answers come in the checks'
/// order, one per check, matched or not.
#[tokio::test]
async fn library_statuses_answer_every_check_in_order() {
    let (db, _temp) = live_db().await;
    exec_batch(
        &db,
        &format!(
            "INSERT INTO albums (id, title, is_compilation, created_at) VALUES ('{OTHER_ALBUM_ID}', 'Older Album', 0, '2025-01-01T00:00:00Z');
             INSERT INTO releases (id, album_id, created_at, album_order) VALUES ('{OTHER_RELEASE_ID}', '{OTHER_ALBUM_ID}', '2025-01-01T00:00:00Z', 0);
             INSERT INTO release_records (id, release_id, catalog, kind, key, album_key, url, created_at) VALUES ('{IDENTITY_ID}', '{RELEASE_ID}', 'musicbrainz', 'album', 'group-1', NULL, 'https://musicbrainz.org/release-group/group-1', '2026-01-01T00:00:00Z');
             INSERT INTO release_records (id, release_id, catalog, kind, key, album_key, url, created_at) VALUES ('0d8f8a3e-6c1b-4d5e-9a3c-2f1e7b6a5d4c', '{OTHER_RELEASE_ID}', 'musicbrainz', 'pressing', 'pressing-1', 'group-1', 'https://musicbrainz.org/release/pressing-1', '2025-01-01T00:00:00Z');"
        ),
    )
    .await;
    let check = |release_id: &str, group: Option<&str>| LibraryCheck {
        release_id: release_id.to_string(),
        source: Catalog::MusicBrainz,
        source_group_id: group.map(str::to_string),
    };
    let statuses = db
        .check_releases_in_library(&[
            check("pressing-1", Some("group-1")),
            check("pressing-2", None),
            check("pressing-3", Some("group-1")),
            check("pressing-1", None),
        ])
        .await
        .unwrap();
    fn albums(status: &LibraryStatus) -> Vec<(&str, bool)> {
        status
            .albums
            .iter()
            .map(|album| (album.album_id.as_str(), album.holds_release))
            .collect()
    }
    assert_eq!(
        statuses
            .iter()
            .map(|status| status.release_id.as_str())
            .collect::<Vec<_>>(),
        ["pressing-1", "pressing-2", "pressing-3", "pressing-1"]
    );
    assert_eq!(
        albums(&statuses[0]),
        [(OTHER_ALBUM_ID, true), (ALBUM_ID, false)],
        "the album holding the pressing comes before the one holding its album"
    );
    assert!(statuses[1].albums.is_empty());
    assert_eq!(
        albums(&statuses[2]),
        [(OTHER_ALBUM_ID, false), (ALBUM_ID, false)],
        "albums holding only its album come oldest first"
    );
    assert_eq!(albums(&statuses[3]), [(OTHER_ALBUM_ID, true)]);
}
