//! How the store resolves the artists an import credits: a credit whose
//! catalog ids or name match a library artist is that artist, credits of
//! one write that name the same artist are one new artist, and ids that
//! name two different artists are refused.

use crate::test_fixtures::temp_db;
use crate::{DbArtist, IdentityWriteError};
use chrono::Utc;
use uuid::Uuid;

fn make_artist(name: &str, discogs_id: Option<&str>, mb_id: Option<&str>) -> DbArtist {
    let now = Utc::now();
    DbArtist {
        id: Uuid::new_v4().to_string(),
        name: name.to_string(),
        sort_name: None,
        discogs_artist_id: discogs_id.map(|s| s.to_string()),
        musicbrainz_artist_id: mb_id.map(|s| s.to_string()),
        created_at: now,
    }
}

/// The id a new artist gets: the one its catalog ids name.
fn catalog_artist_id(artist: &DbArtist) -> String {
    crate::identity::artist_id(
        artist.musicbrainz_artist_id.as_deref(),
        artist.discogs_artist_id.as_deref(),
    )
    .expect("the artist has a catalog id")
}

#[tokio::test]
async fn test_same_discogs_id_reuses_existing() {
    let (db, _tmp) = temp_db().await;
    let existing = make_artist("Artist One", Some("d123"), None);
    db.insert_artist(&existing).await.unwrap();

    let incoming = make_artist("Artist One", Some("d123"), None);
    let resolved = db.find_or_create_artists(std::slice::from_ref(&incoming)).await.unwrap();

    assert_eq!(resolved[0], existing.id);
}

#[tokio::test]
async fn test_same_mb_id_reuses_existing() {
    let (db, _tmp) = temp_db().await;
    let existing = make_artist("Artist One", None, Some("mb-abc"));
    db.insert_artist(&existing).await.unwrap();

    let incoming = make_artist("Artist One", None, Some("mb-abc"));
    let resolved = db.find_or_create_artists(std::slice::from_ref(&incoming)).await.unwrap();

    assert_eq!(resolved[0], existing.id);
}

#[tokio::test]
async fn one_import_reuses_an_artist_already_waiting_to_be_inserted() {
    let (db, _tmp) = temp_db().await;
    let discogs_credit = make_artist("Artist One", Some("d123"), None);
    let cross_linked_credit = make_artist("Artist One", Some("d123"), Some("mb-abc"));

    let resolved = db
        .find_or_create_artists(&[discogs_credit.clone(), cross_linked_credit.clone()])
        .await
        .unwrap();

    // The one new artist carries both ids, and its row id is the one its
    // MusicBrainz id names.
    let artist_id = catalog_artist_id(&cross_linked_credit);
    assert_eq!(resolved, vec![artist_id.clone(), artist_id.clone()]);
    let saved = db.find_artist_by_id(&artist_id).await.unwrap().unwrap();
    assert_eq!(saved.musicbrainz_artist_id.as_deref(), Some("mb-abc"));
}

#[tokio::test]
async fn one_import_reuses_an_idless_artist_name_already_waiting_to_be_inserted() {
    let (db, _tmp) = temp_db().await;
    let album_credit = make_artist("Artist Name", None, None);
    let track_credit = make_artist("Artist Name", None, None);

    let resolved =
        db.find_or_create_artists(&[album_credit.clone(), track_credit.clone()]).await.unwrap();

    assert_eq!(resolved, vec![album_credit.id.clone(), album_credit.id.clone()]);
    assert!(db.find_artist_by_id(&album_credit.id).await.unwrap().is_some());
    assert!(db.find_artist_by_id(&track_credit.id).await.unwrap().is_none());
}

#[tokio::test]
async fn one_import_combines_two_pending_artists_when_a_later_credit_links_them() {
    let (db, _tmp) = temp_db().await;
    let discogs_credit = make_artist("Artist One", Some("d123"), None);
    let musicbrainz_credit = make_artist("Artist One", None, Some("mb-abc"));
    let cross_linked_credit = make_artist("Artist One", Some("d123"), Some("mb-abc"));

    let resolved = db
        .find_or_create_artists(&[
            discogs_credit.clone(),
            musicbrainz_credit,
            cross_linked_credit.clone(),
        ])
        .await
        .unwrap();

    let artist_id = catalog_artist_id(&cross_linked_credit);
    assert_eq!(resolved, vec![artist_id.clone(), artist_id.clone(), artist_id.clone()]);
    let saved = db.find_artist_by_id(&artist_id).await.unwrap().unwrap();
    assert_eq!(saved.discogs_artist_id.as_deref(), Some("d123"));
    assert_eq!(saved.musicbrainz_artist_id.as_deref(), Some("mb-abc"));
}

#[tokio::test]
async fn a_pending_artist_is_folded_into_a_committed_cross_provider_match() {
    let (db, _tmp) = temp_db().await;
    let existing = make_artist("Artist One", None, Some("mb-abc"));
    db.insert_artist(&existing).await.unwrap();
    let discogs_credit = make_artist("Artist One", Some("d123"), None);
    let cross_linked_credit = make_artist("Artist One", Some("d123"), Some("mb-abc"));

    let resolved = db.find_or_create_artists(&[discogs_credit, cross_linked_credit]).await.unwrap();

    assert_eq!(resolved, vec![existing.id.clone(), existing.id.clone()]);
    let saved = db.find_artist_by_id(&existing.id).await.unwrap().unwrap();
    assert_eq!(saved.discogs_artist_id.as_deref(), Some("d123"));
    assert_eq!(saved.musicbrainz_artist_id.as_deref(), Some("mb-abc"));
}

#[tokio::test]
async fn one_import_reuses_an_external_id_waiting_to_be_added_to_an_existing_artist() {
    let (db, _tmp) = temp_db().await;
    let existing = make_artist("Artist One", None, Some("mb-abc"));
    db.insert_artist(&existing).await.unwrap();
    let cross_linked_credit = make_artist("Artist One", Some("d123"), Some("mb-abc"));
    let discogs_credit = make_artist("Artist One", Some("d123"), None);

    let resolved = db.find_or_create_artists(&[cross_linked_credit, discogs_credit]).await.unwrap();

    assert_eq!(resolved, vec![existing.id.clone(), existing.id.clone()]);
    let saved = db.find_artist_by_id(&existing.id).await.unwrap().unwrap();
    assert_eq!(saved.discogs_artist_id.as_deref(), Some("d123"));
}

#[tokio::test]
async fn credits_of_one_write_reuse_an_artist_waiting_to_be_inserted() {
    let (db, _tmp) = temp_db().await;
    let discogs_credit = make_artist("Artist One", Some("d123"), None);
    let cross_linked_credit = make_artist("Artist One", Some("d123"), Some("mb-abc"));

    let resolved = db.find_or_create_artists(&[discogs_credit, cross_linked_credit]).await.unwrap();

    assert_eq!(resolved, vec![resolved[0].clone(); 2]);
    let saved = db.find_artist_by_id(&resolved[0]).await.unwrap().unwrap();
    assert_eq!(saved.musicbrainz_artist_id.as_deref(), Some("mb-abc"));
}

#[tokio::test]
async fn conflicting_exact_source_ids_fail_instead_of_choosing_an_artist() {
    let (db, _tmp) = temp_db().await;
    let discogs_artist = make_artist("Artist One", Some("d123"), None);
    let musicbrainz_artist = make_artist("Artist Two", None, Some("mb-abc"));
    db.insert_artist(&discogs_artist).await.unwrap();
    db.insert_artist(&musicbrainz_artist).await.unwrap();

    let incoming = make_artist("Artist Three", Some("d123"), Some("mb-abc"));
    let error = db
        .find_or_create_artists(std::slice::from_ref(&incoming))
        .await
        .expect_err("source IDs belonging to different artists must fail");

    let IdentityWriteError::ArtistIdentityConflict(conflict) = error else {
        panic!("expected a typed artist identity conflict");
    };
    assert_eq!(conflict.incoming_artist_name, "Artist Three");
    assert_eq!(conflict.discogs_artist_id, "d123");
    assert_eq!(conflict.musicbrainz_artist_id, "mb-abc");
    assert_eq!(conflict.discogs_artist.artist_id, discogs_artist.id);
    assert_eq!(conflict.musicbrainz_artist.artist_id, musicbrainz_artist.id);
    assert!(db.find_artist_by_id(&incoming.id).await.unwrap().is_none());
}

#[tokio::test]
async fn a_same_name_credit_without_ids_is_the_library_artist() {
    let (db, _tmp) = temp_db().await;
    let existing = make_artist("Artist One", None, None);
    db.insert_artist(&existing).await.unwrap();

    let incoming = make_artist("artist one", None, None);
    let resolved = db.find_or_create_artists(std::slice::from_ref(&incoming)).await.unwrap();

    assert_eq!(resolved[0], existing.id);
}

#[tokio::test]
async fn test_same_name_same_mb_id_reuses() {
    let (db, _tmp) = temp_db().await;
    let existing = make_artist("Artist Two", None, Some("mb-artist-two"));
    db.insert_artist(&existing).await.unwrap();

    let incoming = make_artist("Artist Two", None, Some("mb-artist-two"));
    let resolved = db.find_or_create_artists(std::slice::from_ref(&incoming)).await.unwrap();

    assert_eq!(resolved[0], existing.id);
}

#[tokio::test]
async fn test_same_name_different_mb_id_creates_new() {
    let (db, _tmp) = temp_db().await;
    let existing = make_artist("Artist Two", None, Some("mb-artist-two-uk"));
    db.insert_artist(&existing).await.unwrap();

    let incoming = make_artist("Artist Two", None, Some("mb-artist-two-ca"));
    let resolved = db.find_or_create_artists(std::slice::from_ref(&incoming)).await.unwrap();

    // Should create a new artist, not reuse existing
    assert_eq!(resolved[0], catalog_artist_id(&incoming));
    assert_ne!(resolved[0], existing.id);
}

#[tokio::test]
async fn test_same_name_different_discogs_id_creates_new() {
    let (db, _tmp) = temp_db().await;
    let existing = make_artist("Artist Two", Some("d100"), None);
    db.insert_artist(&existing).await.unwrap();

    let incoming = make_artist("Artist Two", Some("d200"), None);
    let resolved = db.find_or_create_artists(std::slice::from_ref(&incoming)).await.unwrap();

    assert_eq!(resolved[0], catalog_artist_id(&incoming));
    assert_ne!(resolved[0], existing.id);
}

#[tokio::test]
async fn ids_of_different_catalogs_do_not_keep_a_name_match_apart() {
    let (db, _tmp) = temp_db().await;
    // Existing has a Discogs ID only; the credit a MusicBrainz ID only.
    // Neither contradicts the other, so the name decides.
    let existing = make_artist("Artist One", Some("d456"), None);
    db.insert_artist(&existing).await.unwrap();

    let incoming = make_artist("Artist One", None, Some("mb-xyz"));
    let resolved = db.find_or_create_artists(std::slice::from_ref(&incoming)).await.unwrap();

    assert_eq!(resolved[0], existing.id, "the artist keeps its own id");
    let linked = db.find_artist_by_id(&existing.id).await.unwrap().unwrap();
    assert_eq!(linked.discogs_artist_id.as_deref(), Some("d456"));
    assert_eq!(linked.musicbrainz_artist_id.as_deref(), Some("mb-xyz"));
    assert!(db.find_artist_by_id(&catalog_artist_id(&incoming)).await.unwrap().is_none());
}

#[tokio::test]
async fn test_new_artist_inserts() {
    let (db, _tmp) = temp_db().await;

    let incoming = make_artist("New Band", Some("d999"), Some("mb-999"));
    let resolved = db.find_or_create_artists(std::slice::from_ref(&incoming)).await.unwrap();

    assert_eq!(resolved[0], catalog_artist_id(&incoming));

    // Verify it's in the DB
    let saved = db.find_artist_by_id(&resolved[0]).await.unwrap().unwrap();
    assert_eq!(saved.name, "New Band");
    assert_eq!(saved.discogs_artist_id.as_deref(), Some("d999"));
    assert_eq!(saved.musicbrainz_artist_id.as_deref(), Some("mb-999"));
}
