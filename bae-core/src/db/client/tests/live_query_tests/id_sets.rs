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
        live.next().await.into_result().unwrap().is_empty(),
        "no offers check nothing"
    );

    requests
        .set([offered.clone(), other.clone()].into_iter().collect())
        .unwrap();
    let initial = live.next().await.into_result().unwrap();
    assert_eq!(initial.len(), 2);
    assert!(initial.iter().all(|status| !status.album_in_library));

    exec(
        &db,
        "INSERT INTO release_records
         (id, release_id, catalog, kind, key, album_key, url,
          _updated_at, created_at)
         VALUES (?1, ?2, 'musicbrainz', 'pressing', 'source-release-1', 'source-group-1',
                 'https://musicbrainz.org/release/source-release-1',
                 'identity-v1', '2026-01-01T00:00:00Z')",
        &[IDENTITY_ID, RELEASE_ID],
    )
    .await;
    let updated = tokio::time::timeout(Duration::from_secs(2), live.next())
        .await
        .expect("identity write wakes the library statuses")
        .into_result()
        .unwrap();
    let status = |statuses: &[LibraryStatus], release_id: &str| {
        statuses
            .iter()
            .find(|status| status.release_id == release_id)
            .cloned()
            .expect("a status per check")
    };
    let held = status(&updated, "source-release-1");
    assert!(held.release_in_library);
    assert!(held.album_in_library);
    assert_eq!(held.album_id.as_deref(), Some(ALBUM_ID));
    assert!(!status(&updated, "source-release-2").album_in_library);

    exec(&db,
        "UPDATE release_records SET kind = 'album', key = 'source-group-1', album_key = NULL, url = 'https://musicbrainz.org/release-group/source-group-1' WHERE id = ?1",
        &[IDENTITY_ID],
    ).await;
    let album_only = tokio::time::timeout(Duration::from_secs(2), live.next())
        .await
        .expect("identity kind change wakes the library statuses")
        .into_result()
        .unwrap();
    let held = status(&album_only, "source-release-1");
    assert!(!held.release_in_library);
    assert!(held.album_in_library);

    requests.set([other].into_iter().collect()).unwrap();
    let narrowed = live.next().await.into_result().unwrap();
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
/// place, an id with no summary names an album that is not in the library,
/// and deleting a selected album wakes the read that shows it gone.
#[tokio::test]
async fn album_selection_follows_its_ids_on_one_subscription() {
    let (db, _temp) = live_db().await;
    let mut live = db.subscribe_album_selection(BTreeSet::new());
    let requests = live.requests();
    assert!(live.next().await.into_result().unwrap().albums.is_empty());

    requests
        .set(
            [ALBUM_ID.to_string(), "missing-album".to_string()]
                .into_iter()
                .collect(),
        )
        .unwrap();
    let selected = live.next().await;
    assert_eq!(selected.request().len(), 2);
    let selected = selected.into_result().unwrap();
    assert_eq!(
        selected
            .albums
            .iter()
            .map(|album| album.id.as_str())
            .collect::<Vec<_>>(),
        vec![ALBUM_ID]
    );

    exec(&db, "DELETE FROM albums WHERE id = ?1", &[ALBUM_ID]).await;
    let deleted = tokio::time::timeout(Duration::from_secs(2), live.next())
        .await
        .expect("deleting a selected album wakes the selection")
        .into_result()
        .unwrap();
    assert!(deleted.albums.is_empty());
}
