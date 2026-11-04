//! How the store resolves the artists an import credits: a credit whose
//! catalog ids or name match a library artist is that artist, credits of
//! one write that name the same artist are one new artist, artists holding
//! one id are merged, and where the credit's ids disagree the MusicBrainz id
//! decides.

use super::*;
use crate::test_fixtures::temp_db;
use crate::{DbArtist, IdentityWriteError};
use chrono::Utc;
use uuid::Uuid;

const NOW: &str = "2026-01-01T00:00:00Z";

fn library() -> Connection {
    crate::schema::bare_library_connection(crate::test_fixtures::TEST_SCHEMA)
}

fn add_artist(
    conn: &Connection,
    id: &str,
    name: &str,
    discogs_artist_id: Option<&str>,
    musicbrainz_artist_id: Option<&str>,
) {
    conn.execute(
        "INSERT INTO artists (id, name, name_key, discogs_artist_id, musicbrainz_artist_id, created_at) VALUES (?, ?, ?, ?, ?, ?)",
        params![
            id,
            name,
            bae_types::text_match::artist_name_key(name),
            discogs_artist_id,
            musicbrainz_artist_id,
            NOW
        ],
    )
    .unwrap();
}

fn credit(
    id: &str,
    name: &str,
    discogs_artist_id: Option<&str>,
    musicbrainz_artist_id: Option<&str>,
) -> DbArtist {
    DbArtist {
        id: id.to_string(),
        name: name.to_string(),
        sort_name: None,
        discogs_artist_id: discogs_artist_id.map(str::to_string),
        musicbrainz_artist_id: musicbrainz_artist_id.map(str::to_string),
        created_at: chrono::DateTime::parse_from_rfc3339(NOW).unwrap().with_timezone(&Utc),
    }
}

fn resolve(conn: &Connection, credits: &[DbArtist]) -> ResolvedArtists {
    resolve_artists_on(conn, credits, &HashSet::new()).unwrap()
}

#[test]
fn a_name_only_credit_is_the_one_library_artist_of_that_name() {
    let conn = library();
    add_artist(&conn, "library-artist", "Artist Name", None, None);

    let resolved = resolve(&conn, &[credit("credit", "Artist Name", None, None)]);

    assert_eq!(resolved.ids, ["library-artist"]);
    assert!(resolved.inserts.is_empty());
}

#[test]
fn a_name_matches_however_it_is_cased_or_accented() {
    let conn = library();
    add_artist(&conn, "library-artist", "Artist Name", None, None);

    let resolved = resolve(
        &conn,
        &[
            credit("lowercase", "artist name", None, None),
            credit("accented", "Ärtist  Name", None, None),
        ],
    );

    assert_eq!(resolved.ids, ["library-artist", "library-artist"]);
    assert!(resolved.inserts.is_empty());
}

#[test]
fn a_catalog_credit_links_by_name_and_gives_the_artist_its_id() {
    let conn = library();
    add_artist(&conn, "library-artist", "Artist Name", None, None);

    let resolved = resolve(&conn, &[credit("credit", "Artist Name", Some("discogs-1"), None)]);

    assert_eq!(resolved.ids, ["library-artist"], "the artist keeps its id");
    assert!(resolved.inserts.is_empty());
    assert_eq!(resolved.external_id_updates.len(), 1);
    let (id, filled) = &resolved.external_id_updates[0];
    assert_eq!(id, "library-artist");
    assert_eq!(filled.discogs_artist_id.as_deref(), Some("discogs-1"));
}

#[test]
fn a_library_artist_holding_another_id_of_the_catalog_is_not_the_credit() {
    let conn = library();
    add_artist(&conn, "library-artist", "Artist Name", Some("discogs-x"), None);

    let resolved = resolve(&conn, &[credit("credit", "Artist Name", Some("discogs-y"), None)]);

    let identity = crate::identity::artist_id(None, Some("discogs-y")).unwrap();
    assert_eq!(resolved.ids, [identity.as_str()]);
    assert_eq!(resolved.inserts.len(), 1);
    assert_eq!(resolved.inserts[0].id, identity);
}

#[test]
fn two_library_artists_of_one_name_leave_the_credit_new() {
    let conn = library();
    add_artist(&conn, "first", "Artist Name", None, None);
    add_artist(&conn, "second", "Artist Name", Some("discogs-2"), None);

    let resolved = resolve(&conn, &[credit("credit", "Artist Name", None, None)]);

    assert_eq!(resolved.ids, ["credit"]);
    assert_eq!(resolved.inserts.len(), 1);
}

#[test]
fn one_write_makes_one_new_artist_of_two_ambiguous_credits() {
    let conn = library();
    add_artist(&conn, "first", "Artist Name", None, None);
    add_artist(&conn, "second", "Artist Name", None, None);

    let resolved = resolve(
        &conn,
        &[
            credit("album-credit", "Artist Name", None, None),
            credit("track-credit", "artist name", None, None),
        ],
    );

    assert_eq!(resolved.ids, ["album-credit", "album-credit"]);
    assert_eq!(resolved.inserts.len(), 1);
}

#[test]
fn a_catalog_id_wins_over_the_name() {
    let conn = library();
    add_artist(&conn, "named", "Artist Name", None, None);
    add_artist(&conn, "catalogued", "Other Name", Some("discogs-1"), None);

    let resolved = resolve(&conn, &[credit("credit", "Artist Name", Some("discogs-1"), None)]);

    assert_eq!(resolved.ids, ["catalogued"]);
}

#[test]
fn an_absorbed_artist_is_not_matched_by_its_name() {
    let conn = library();
    add_artist(&conn, "absorbed", "Artist Name", None, None);
    add_artist(&conn, "survivor", "Other Name", None, None);
    conn.execute(
        "INSERT INTO artist_merges (id, into_artist_id, created_at) VALUES ('absorbed', 'survivor', ?)",
        params![NOW],
    )
    .unwrap();

    let resolved = resolve(&conn, &[credit("credit", "Artist Name", None, None)]);

    assert_eq!(resolved.ids, ["credit"]);
}

#[test]
fn a_new_catalog_artist_takes_the_catalog_identity() {
    let conn = library();

    let resolved = resolve(
        &conn,
        &[
            credit("album-credit", "Artist Name", None, None),
            credit("track-credit", "Artist Name", Some("discogs-1"), None),
        ],
    );

    let identity = crate::identity::artist_id(None, Some("discogs-1")).unwrap();
    assert_eq!(resolved.ids, [identity.clone(), identity.clone()]);
    assert_eq!(resolved.inserts.len(), 1);
    assert_eq!(resolved.inserts[0].discogs_artist_id.as_deref(), Some("discogs-1"));
}

#[test]
fn a_picked_artist_that_is_gone_refuses_the_write() {
    let conn = library();
    let picked = HashSet::from(["gone".to_string()]);

    let error = resolve_artists_on(&conn, &[credit("gone", "Artist Name", None, None)], &picked)
        .unwrap_err();

    assert!(matches!(error, IdentityWriteError::Unresolvable(_)), "{error}");
}

/// A credit whose MusicBrainz id and Discogs id name two library artists is
/// the MusicBrainz one, and the Discogs id that disagrees is dropped: no
/// artist is given it.
#[test]
fn two_artists_holding_the_credits_two_ids_leave_the_musicbrainz_one() {
    let conn = library();
    add_artist(&conn, "by-discogs", "Artist Name", Some("discogs-1"), None);
    add_artist(&conn, "by-musicbrainz", "Artist Name", None, Some("mb-1"));

    let resolved =
        resolve(&conn, &[credit("credit", "Artist Name", Some("discogs-1"), Some("mb-1"))]);

    assert_eq!(resolved.ids, ["by-musicbrainz"]);
    assert!(resolved.inserts.is_empty());
    assert!(resolved.merges.is_empty());
    assert!(
        resolved.external_id_updates.iter().all(|(_, artist)| artist.discogs_artist_id.is_none()),
        "the disagreeing Discogs id goes nowhere: {:?}",
        resolved.external_id_updates
    );
}

/// A credit whose MusicBrainz id names an artist holding another Discogs id
/// is that artist; the credit's Discogs id is dropped.
#[test]
fn a_musicbrainz_artist_holding_another_discogs_id_is_the_credit() {
    let conn = library();
    add_artist(&conn, "library-artist", "Artist Name", Some("discogs-x"), Some("mb-1"));

    let resolved =
        resolve(&conn, &[credit("credit", "Artist Name", Some("discogs-1"), Some("mb-1"))]);

    assert_eq!(resolved.ids, ["library-artist"]);
    assert!(resolved.inserts.is_empty());
    assert!(resolved
        .external_id_updates
        .iter()
        .all(|(_, artist)| artist.discogs_artist_id.as_deref() != Some("discogs-1")));
}

/// A credit whose Discogs id names an artist holding another MusicBrainz id
/// goes by its MusicBrainz id: no library artist holds it, so it is a new
/// artist carrying that id alone.
#[test]
fn a_discogs_artist_holding_another_musicbrainz_id_leaves_the_credit_its_musicbrainz_id() {
    let conn = library();
    add_artist(&conn, "library-artist", "Artist Name", Some("discogs-1"), Some("mb-2"));

    let resolved =
        resolve(&conn, &[credit("credit", "Artist Name", Some("discogs-1"), Some("mb-1"))]);

    let identity = crate::identity::artist_id(Some("mb-1"), None).unwrap();
    assert_eq!(resolved.ids, [identity.as_str()]);
    assert_eq!(resolved.inserts.len(), 1);
    assert_eq!(resolved.inserts[0].musicbrainz_artist_id.as_deref(), Some("mb-1"));
    assert_eq!(resolved.inserts[0].discogs_artist_id, None);
}

/// One catalog id is one artist: two library artists holding it are merged
/// into the smaller id, which the credit names and which takes the other's
/// ids where it has none.
#[test]
fn a_catalog_id_two_library_artists_hold_merges_them() {
    let conn = library();
    add_artist(&conn, "first", "Artist Name", Some("discogs-1"), None);
    add_artist(&conn, "second", "Artist Name", Some("discogs-1"), Some("mb-2"));

    let resolved = resolve(&conn, &[credit("credit", "Artist Name", Some("discogs-1"), None)]);

    assert_eq!(resolved.ids, ["first"]);
    let merged: Vec<(&str, &str)> = resolved
        .merges
        .iter()
        .map(|merge| (merge.absorbed.id.as_str(), merge.survivor.as_str()))
        .collect();
    assert_eq!(merged, [("second", "first")]);
    let (_, survivor) = &resolved.external_id_updates[0];
    assert_eq!(survivor.musicbrainz_artist_id.as_deref(), Some("mb-2"));
}

/// An id on the row it derives and on another artist that was given it is
/// one artist too: the smaller id survives.
#[test]
fn a_catalog_id_on_its_own_row_and_on_another_artist_merges_them() {
    let conn = library();
    let identity = crate::identity::artist_id(Some("mb-1"), None).unwrap();
    add_artist(&conn, &identity, "Artist Name", None, None);
    add_artist(&conn, "filled-in", "Artist Name", None, Some("mb-1"));

    let resolved = resolve(&conn, &[credit("credit", "Artist Name", None, Some("mb-1"))]);

    let survivor = identity.as_str().min("filled-in");
    assert_eq!(resolved.ids, [survivor]);
    assert_eq!(resolved.merges.len(), 1);
    assert_eq!(resolved.merges[0].survivor, survivor);
}

fn read_credit(
    conn: &Connection,
    name: &str,
    discogs_artist_id: Option<&str>,
) -> crate::CreditResolution {
    resolve_credit_on(
        conn,
        &bae_catalogs::credits::ArtistCredit {
            credited_as: None,
            name: name.to_string(),
            sort_name: None,
            musicbrainz_artist_id: None,
            discogs_artist_id: discogs_artist_id.map(str::to_string),
        },
    )
    .unwrap()
}

fn artist_ids(artists: &[crate::ExistingArtist]) -> Vec<&str> {
    artists.iter().map(|artist| artist.artist_id.as_str()).collect()
}

#[test]
fn a_read_credit_is_new_until_the_library_holds_its_artist() {
    let conn = library();
    assert_eq!(read_credit(&conn, "Artist Name", None), crate::CreditResolution::New);

    add_artist(&conn, "library-artist", "Ärtist Name", None, None);

    let crate::CreditResolution::Library { artist } = read_credit(&conn, "artist name", None)
    else {
        panic!("the folded name names the library artist");
    };
    assert_eq!(artist.artist_id, "library-artist");
}

#[test]
fn a_read_credit_shared_by_two_library_artists_is_ambiguous() {
    let conn = library();
    add_artist(&conn, "first", "Artist Name", None, None);
    add_artist(&conn, "second", "Artist Name", None, Some("mb-2"));

    let crate::CreditResolution::Ambiguous { artists } = read_credit(&conn, "Artist Name", None)
    else {
        panic!("two artists share the name");
    };
    assert_eq!(artist_ids(&artists), ["first", "second"]);
}

#[test]
fn a_read_credit_with_another_catalog_id_than_the_named_artist_is_new() {
    let conn = library();
    add_artist(&conn, "library-artist", "Artist Name", Some("discogs-x"), None);

    assert_eq!(read_credit(&conn, "Artist Name", Some("discogs-y")), crate::CreditResolution::New);
}

/// A read of a credit whose two ids name two artists names the MusicBrainz
/// one, as the write does.
#[test]
fn a_read_credit_whose_ids_name_two_artists_is_the_musicbrainz_one() {
    let conn = library();
    add_artist(&conn, "by-discogs", "Artist One", Some("discogs-1"), None);
    add_artist(&conn, "by-musicbrainz", "Artist Two", None, Some("mb-1"));

    let resolution = resolve_credit_on(
        &conn,
        &bae_catalogs::credits::ArtistCredit {
            credited_as: None,
            name: "Artist Name".to_string(),
            sort_name: None,
            musicbrainz_artist_id: Some("mb-1".to_string()),
            discogs_artist_id: Some("discogs-1".to_string()),
        },
    )
    .unwrap();

    let crate::CreditResolution::Library { artist } = resolution else {
        panic!("the MusicBrainz id names the artist, got {resolution:?}");
    };
    assert_eq!(artist.artist_id, "by-musicbrainz");
}

/// A read of a credit whose id two library artists hold names the one the
/// next write merges them into.
#[test]
fn a_read_credit_whose_id_two_artists_hold_is_the_smaller_one() {
    let conn = library();
    add_artist(&conn, "first", "Artist One", Some("discogs-1"), None);
    add_artist(&conn, "second", "Artist Two", Some("discogs-1"), None);

    let crate::CreditResolution::Library { artist } =
        read_credit(&conn, "Artist Name", Some("discogs-1"))
    else {
        panic!("one id names one artist");
    };
    assert_eq!(artist.artist_id, "first");
}

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
    let (db, _tmp) = temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let existing = make_artist("Artist One", Some("d123"), None);
    db.insert_artist(&existing).await.unwrap();

    let incoming = make_artist("Artist One", Some("d123"), None);
    let resolved = db.find_or_create_artists(std::slice::from_ref(&incoming)).await.unwrap();

    assert_eq!(resolved[0], existing.id);
}

#[tokio::test]
async fn test_same_mb_id_reuses_existing() {
    let (db, _tmp) = temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let existing = make_artist("Artist One", None, Some("mb-abc"));
    db.insert_artist(&existing).await.unwrap();

    let incoming = make_artist("Artist One", None, Some("mb-abc"));
    let resolved = db.find_or_create_artists(std::slice::from_ref(&incoming)).await.unwrap();

    assert_eq!(resolved[0], existing.id);
}

#[tokio::test]
async fn one_import_reuses_an_artist_already_waiting_to_be_inserted() {
    let (db, _tmp) = temp_db(crate::test_fixtures::TEST_SCHEMA).await;
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
    let (db, _tmp) = temp_db(crate::test_fixtures::TEST_SCHEMA).await;
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
    let (db, _tmp) = temp_db(crate::test_fixtures::TEST_SCHEMA).await;
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
    let (db, _tmp) = temp_db(crate::test_fixtures::TEST_SCHEMA).await;
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
    let (db, _tmp) = temp_db(crate::test_fixtures::TEST_SCHEMA).await;
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
    let (db, _tmp) = temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let discogs_credit = make_artist("Artist One", Some("d123"), None);
    let cross_linked_credit = make_artist("Artist One", Some("d123"), Some("mb-abc"));

    let resolved = db.find_or_create_artists(&[discogs_credit, cross_linked_credit]).await.unwrap();

    assert_eq!(resolved, vec![resolved[0].clone(); 2]);
    let saved = db.find_artist_by_id(&resolved[0]).await.unwrap().unwrap();
    assert_eq!(saved.musicbrainz_artist_id.as_deref(), Some("mb-abc"));
}

/// A write crediting ids that name two library artists credits the
/// MusicBrainz one and leaves both artists as they were.
#[tokio::test]
async fn exact_source_ids_naming_two_artists_credit_the_musicbrainz_one() {
    let (db, _tmp) = temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let discogs_artist = make_artist("Artist One", Some("d123"), None);
    let musicbrainz_artist = make_artist("Artist Two", None, Some("mb-abc"));
    db.insert_artist(&discogs_artist).await.unwrap();
    db.insert_artist(&musicbrainz_artist).await.unwrap();

    let incoming = make_artist("Artist Three", Some("d123"), Some("mb-abc"));
    let resolved = db.find_or_create_artists(std::slice::from_ref(&incoming)).await.unwrap();

    assert_eq!(resolved, std::slice::from_ref(&musicbrainz_artist.id));
    for artist in [&discogs_artist, &musicbrainz_artist] {
        let stored = db.find_artist_by_id(&artist.id).await.unwrap().unwrap();
        assert_eq!(
            (stored.discogs_artist_id, stored.musicbrainz_artist_id),
            (artist.discogs_artist_id.clone(), artist.musicbrainz_artist_id.clone())
        );
    }
    assert!(db.find_artist_by_id(&incoming.id).await.unwrap().is_none());
}

#[tokio::test]
async fn a_same_name_credit_without_ids_is_the_library_artist() {
    let (db, _tmp) = temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let existing = make_artist("Artist One", None, None);
    db.insert_artist(&existing).await.unwrap();

    let incoming = make_artist("artist one", None, None);
    let resolved = db.find_or_create_artists(std::slice::from_ref(&incoming)).await.unwrap();

    assert_eq!(resolved[0], existing.id);
}

#[tokio::test]
async fn test_same_name_same_mb_id_reuses() {
    let (db, _tmp) = temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let existing = make_artist("Artist Two", None, Some("mb-artist-two"));
    db.insert_artist(&existing).await.unwrap();

    let incoming = make_artist("Artist Two", None, Some("mb-artist-two"));
    let resolved = db.find_or_create_artists(std::slice::from_ref(&incoming)).await.unwrap();

    assert_eq!(resolved[0], existing.id);
}

#[tokio::test]
async fn test_same_name_different_mb_id_creates_new() {
    let (db, _tmp) = temp_db(crate::test_fixtures::TEST_SCHEMA).await;
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
    let (db, _tmp) = temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let existing = make_artist("Artist Two", Some("d100"), None);
    db.insert_artist(&existing).await.unwrap();

    let incoming = make_artist("Artist Two", Some("d200"), None);
    let resolved = db.find_or_create_artists(std::slice::from_ref(&incoming)).await.unwrap();

    assert_eq!(resolved[0], catalog_artist_id(&incoming));
    assert_ne!(resolved[0], existing.id);
}

#[tokio::test]
async fn ids_of_different_catalogs_do_not_keep_a_name_match_apart() {
    let (db, _tmp) = temp_db(crate::test_fixtures::TEST_SCHEMA).await;
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
    let (db, _tmp) = temp_db(crate::test_fixtures::TEST_SCHEMA).await;

    let incoming = make_artist("New Band", Some("d999"), Some("mb-999"));
    let resolved = db.find_or_create_artists(std::slice::from_ref(&incoming)).await.unwrap();

    assert_eq!(resolved[0], catalog_artist_id(&incoming));

    // Verify it's in the DB
    let saved = db.find_artist_by_id(&resolved[0]).await.unwrap().unwrap();
    assert_eq!(saved.name, "New Band");
    assert_eq!(saved.discogs_artist_id.as_deref(), Some("d999"));
    assert_eq!(saved.musicbrainz_artist_id.as_deref(), Some("mb-999"));
}

/// Two devices apart each give one Discogs id to a different artist of one
/// name. Once each has the other's write, the next write crediting that id
/// merges the two into the smaller id and credits it, and the other device
/// reads the credit as that artist.
#[test]
fn a_catalog_id_two_devices_gave_two_artists_merges_them() {
    crate::test_devices::run_two_store_test(
        crate::test_fixtures::TEST_SCHEMA,
        check_two_devices_give_one_artist_id,
    );
}

async fn check_two_devices_give_one_artist_id(devices: &crate::test_devices::TwoStores) {
    for id in ["artist-a", "artist-b"] {
        devices.a().insert_artist(&credit(id, "Artist Name", None, None)).await.unwrap();
    }
    let artists = "SELECT id || ':' || COALESCE(discogs_artist_id, '') FROM artists";
    devices.converge(artists).await;

    devices.pause().await;
    for (device, id) in [(devices.a(), "artist-a"), (devices.b(), "artist-b")] {
        device
            .call(move |sql| {
                sql.execute(
                    "UPDATE artists SET discogs_artist_id = 'discogs-1' WHERE id = ?",
                    [id],
                )?;
                Ok(())
            })
            .await
            .unwrap();
    }
    devices.resume().await;
    devices.converge(artists).await;

    let credited = devices
        .a()
        .find_or_create_artists(&[credit("credit", "Artist Name", Some("discogs-1"), None)])
        .await
        .unwrap();
    assert_eq!(credited, ["artist-a"]);
    devices.converge("SELECT id || '>' || into_artist_id FROM artist_merges").await;

    let read = bae_catalogs::credits::ArtistCredit {
        discogs_artist_id: Some("discogs-1".to_string()),
        ..bae_catalogs::credits::ArtistCredit::named("Artist Name")
    };
    for device in [devices.a(), devices.b()] {
        let resolved = device.resolve_artist_credits(std::slice::from_ref(&read)).await.unwrap();
        let crate::CreditResolution::Library { artist } = &resolved[0].resolution else {
            panic!("the id names one artist, got {:?}", resolved[0].resolution);
        };
        assert_eq!(artist.artist_id, "artist-a");
    }
}
