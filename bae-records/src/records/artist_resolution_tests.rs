//! How the store resolves the artists an import credits: a credit whose
//! catalog ids or name match a library artist is that artist, credits of
//! one write that name the same artist are one new artist, artists holding
//! one id are merged, and where the credit's ids disagree the MusicBrainz id
//! decides.

use super::*;
use crate::{DbArtist, IdentityWriteError, IdentityWriteRefusal};
use bae_store::test_fixtures::temp_db;
use chrono::Utc;

const NOW: &str = "2026-01-01T00:00:00Z";

async fn library() -> (bae_store::Store, tempfile::TempDir) {
    temp_db(crate::test_fixtures::TEST_SCHEMA).await
}

async fn add_artist(
    db: &bae_store::Store,
    id: &str,
    name: &str,
    discogs_artist_id: Option<&str>,
    musicbrainz_artist_id: Option<&str>,
) {
    crate::test_fixtures::test_records(db)
        .insert_artist(&credit(id, name, discogs_artist_id, musicbrainz_artist_id))
        .await
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

async fn resolve(db: &bae_store::Store, credits: &[DbArtist]) -> ResolvedArtists {
    let credits = credits.to_vec();
    db.read_coven(move |sql| resolve_artists_on(&sql, &credits, &HashSet::new())).await.unwrap()
}

#[tokio::test]
async fn a_name_only_credit_is_the_one_library_artist_of_that_name() {
    let (db, _tmp) = library().await;
    add_artist(&db, "library-artist", "Artist Name", None, None).await;

    let resolved = resolve(&db, &[credit("credit", "Artist Name", None, None)]).await;

    assert_eq!(resolved.ids, ["library-artist"]);
    assert!(resolved.inserts.is_empty());
}

#[tokio::test]
async fn a_name_matches_however_it_is_cased_or_accented() {
    let (db, _tmp) = library().await;
    add_artist(&db, "library-artist", "Artist Name", None, None).await;

    let resolved = resolve(
        &db,
        &[
            credit("lowercase", "artist name", None, None),
            credit("accented", "Ärtist  Name", None, None),
        ],
    )
    .await;

    assert_eq!(resolved.ids, ["library-artist", "library-artist"]);
    assert!(resolved.inserts.is_empty());
}

#[tokio::test]
async fn a_catalog_credit_links_by_name_and_gives_the_artist_its_id() {
    let (db, _tmp) = library().await;
    add_artist(&db, "library-artist", "Artist Name", None, None).await;

    let resolved = resolve(&db, &[credit("credit", "Artist Name", Some("discogs-1"), None)]).await;

    assert_eq!(resolved.ids, ["library-artist"], "the artist keeps its id");
    assert!(resolved.inserts.is_empty());
    assert_eq!(resolved.external_id_updates.len(), 1);
    let (id, filled) = &resolved.external_id_updates[0];
    assert_eq!(id, "library-artist");
    assert_eq!(filled.discogs_artist_id.as_deref(), Some("discogs-1"));
}

#[tokio::test]
async fn a_library_artist_holding_another_id_of_the_catalog_is_not_the_credit() {
    let (db, _tmp) = library().await;
    add_artist(&db, "library-artist", "Artist Name", Some("discogs-x"), None).await;

    let resolved = resolve(&db, &[credit("credit", "Artist Name", Some("discogs-y"), None)]).await;

    let identity = crate::identity::artist_id(None, Some("discogs-y")).unwrap();
    assert_eq!(resolved.ids, [identity.as_str()]);
    assert_eq!(resolved.inserts.len(), 1);
    assert_eq!(resolved.inserts[0].id, identity);
}

#[tokio::test]
async fn two_library_artists_of_one_name_leave_the_credit_new() {
    let (db, _tmp) = library().await;
    add_artist(&db, "first", "Artist Name", None, None).await;
    add_artist(&db, "second", "Artist Name", Some("discogs-2"), None).await;

    let resolved = resolve(&db, &[credit("credit", "Artist Name", None, None)]).await;

    assert_eq!(resolved.ids, ["credit"]);
    assert_eq!(resolved.inserts.len(), 1);
}

#[tokio::test]
async fn one_write_makes_one_new_artist_of_two_ambiguous_credits() {
    let (db, _tmp) = library().await;
    add_artist(&db, "first", "Artist Name", None, None).await;
    add_artist(&db, "second", "Artist Name", None, None).await;

    let resolved = resolve(
        &db,
        &[
            credit("album-credit", "Artist Name", None, None),
            credit("track-credit", "artist name", None, None),
        ],
    )
    .await;

    assert_eq!(resolved.ids, ["album-credit", "album-credit"]);
    assert_eq!(resolved.inserts.len(), 1);
}

#[tokio::test]
async fn a_catalog_id_wins_over_the_name() {
    let (db, _tmp) = library().await;
    add_artist(&db, "named", "Artist Name", None, None).await;
    add_artist(&db, "catalogued", "Other Name", Some("discogs-1"), None).await;

    let resolved = resolve(&db, &[credit("credit", "Artist Name", Some("discogs-1"), None)]).await;

    assert_eq!(resolved.ids, ["catalogued"]);
}

#[tokio::test]
async fn an_absorbed_artist_is_not_matched_by_its_name() {
    let (db, _tmp) = library().await;
    add_artist(&db, "first", "Other Name", Some("discogs-1"), None).await;
    add_artist(&db, "second", "Artist Name", Some("discogs-1"), None).await;
    // The write crediting the id both hold merges the second into the first,
    // the smaller id.
    let credited = crate::test_fixtures::test_records(&db)
        .find_or_create_artists(&[credit("merging", "Other Name", Some("discogs-1"), None)])
        .await
        .unwrap();
    assert_eq!(credited, ["first"]);

    let resolved = resolve(&db, &[credit("credit", "Artist Name", None, None)]).await;

    assert_eq!(resolved.ids, ["credit"]);
}

#[tokio::test]
async fn a_new_catalog_artist_takes_the_catalog_identity() {
    let (db, _tmp) = library().await;

    let resolved = resolve(
        &db,
        &[
            credit("album-credit", "Artist Name", None, None),
            credit("track-credit", "Artist Name", Some("discogs-1"), None),
        ],
    )
    .await;

    let identity = crate::identity::artist_id(None, Some("discogs-1")).unwrap();
    assert_eq!(resolved.ids, [identity.clone(), identity.clone()]);
    assert_eq!(resolved.inserts.len(), 1);
    assert_eq!(resolved.inserts[0].discogs_artist_id.as_deref(), Some("discogs-1"));
}

#[tokio::test]
async fn a_picked_artist_that_is_gone_refuses_the_write() {
    let (db, _tmp) = library().await;
    let picked = HashSet::from(["gone".to_string()]);

    let credits = [credit("gone", "Artist Name", None, None)];
    let error =
        db.read_coven(move |sql| resolve_artists_on(&sql, &credits, &picked)).await.unwrap_err();

    assert!(
        matches!(&error, IdentityWriteError::Refused(IdentityWriteRefusal::Identity(IdentityRefusal::ArtistGone(id))) if id == "gone"),
        "{error}"
    );
}

/// A credit whose MusicBrainz id and Discogs id name two library artists is
/// the MusicBrainz one, and the Discogs id that disagrees is dropped: no
/// artist is given it.
#[tokio::test]
async fn two_artists_holding_the_credits_two_ids_leave_the_musicbrainz_one() {
    let (db, _tmp) = library().await;
    add_artist(&db, "by-discogs", "Artist Name", Some("discogs-1"), None).await;
    add_artist(&db, "by-musicbrainz", "Artist Name", None, Some("mb-1")).await;

    let resolved =
        resolve(&db, &[credit("credit", "Artist Name", Some("discogs-1"), Some("mb-1"))]).await;

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
#[tokio::test]
async fn a_musicbrainz_artist_holding_another_discogs_id_is_the_credit() {
    let (db, _tmp) = library().await;
    add_artist(&db, "library-artist", "Artist Name", Some("discogs-x"), Some("mb-1")).await;

    let resolved =
        resolve(&db, &[credit("credit", "Artist Name", Some("discogs-1"), Some("mb-1"))]).await;

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
#[tokio::test]
async fn a_discogs_artist_holding_another_musicbrainz_id_leaves_the_credit_its_musicbrainz_id() {
    let (db, _tmp) = library().await;
    add_artist(&db, "library-artist", "Artist Name", Some("discogs-1"), Some("mb-2")).await;

    let resolved =
        resolve(&db, &[credit("credit", "Artist Name", Some("discogs-1"), Some("mb-1"))]).await;

    let identity = crate::identity::artist_id(Some("mb-1"), None).unwrap();
    assert_eq!(resolved.ids, [identity.as_str()]);
    assert_eq!(resolved.inserts.len(), 1);
    assert_eq!(resolved.inserts[0].musicbrainz_artist_id.as_deref(), Some("mb-1"));
    assert_eq!(resolved.inserts[0].discogs_artist_id, None);
}

/// Two credits of one write sharing a MusicBrainz id are one artist, which
/// keeps the Discogs id the first gave it: the second credit's MusicBrainz id
/// names that artist, and its Discogs id that disagrees is dropped.
#[tokio::test]
async fn credits_of_one_write_sharing_a_musicbrainz_id_keep_the_first_discogs_id() {
    let (db, _tmp) = library().await;

    let resolved = resolve(
        &db,
        &[
            credit("first", "Artist Name", Some("discogs-1"), Some("mb-1")),
            credit("second", "Artist Name", Some("discogs-2"), Some("mb-1")),
        ],
    )
    .await;

    let identity = crate::identity::artist_id(Some("mb-1"), None).unwrap();
    assert_eq!(resolved.ids, [identity.as_str(), identity.as_str()]);
    assert_eq!(resolved.inserts.len(), 1);
    assert_eq!(resolved.inserts[0].discogs_artist_id.as_deref(), Some("discogs-1"));
}

/// Two credits of one write sharing a Discogs id and carrying two MusicBrainz
/// ids are two artists: the second goes by its MusicBrainz id alone.
#[tokio::test]
async fn credits_of_one_write_sharing_a_discogs_id_with_two_musicbrainz_ids_are_two_artists() {
    let (db, _tmp) = library().await;

    let resolved = resolve(
        &db,
        &[
            credit("first", "Artist Name", Some("discogs-1"), Some("mb-1")),
            credit("second", "Artist Name", Some("discogs-1"), Some("mb-2")),
        ],
    )
    .await;

    let first = crate::identity::artist_id(Some("mb-1"), None).unwrap();
    let second = crate::identity::artist_id(Some("mb-2"), None).unwrap();
    assert_eq!(resolved.ids, [first.as_str(), second.as_str()]);
    let held: Vec<_> = resolved
        .inserts
        .iter()
        .map(|artist| {
            (artist.musicbrainz_artist_id.as_deref(), artist.discogs_artist_id.as_deref())
        })
        .collect();
    assert_eq!(held, [(Some("mb-1"), Some("discogs-1")), (Some("mb-2"), None)]);
}

/// A credit whose Discogs id names a library artist and whose MusicBrainz id
/// names an artist this write makes, holding another Discogs id, is the one
/// this write makes; the library artist keeps its ids.
#[tokio::test]
async fn a_credit_naming_a_new_artist_by_musicbrainz_and_a_library_artist_by_discogs_is_the_new_one(
) {
    let (db, _tmp) = library().await;
    add_artist(&db, "by-discogs", "Artist Name", Some("discogs-1"), None).await;

    let resolved = resolve(
        &db,
        &[
            credit("first", "Artist Name", Some("discogs-2"), Some("mb-1")),
            credit("second", "Artist Name", Some("discogs-1"), Some("mb-1")),
        ],
    )
    .await;

    let identity = crate::identity::artist_id(Some("mb-1"), None).unwrap();
    assert_eq!(resolved.ids, [identity.as_str(), identity.as_str()]);
    assert_eq!(resolved.inserts.len(), 1);
    assert_eq!(resolved.inserts[0].discogs_artist_id.as_deref(), Some("discogs-2"));
    assert!(resolved.external_id_updates.is_empty(), "{:?}", resolved.external_id_updates);
    assert!(resolved.merges.is_empty());
}

/// A credit naming Discogs' Various Artists entry alone is the artist holding
/// MusicBrainz' Various Artists id, which keeps the other Discogs id it holds.
#[tokio::test]
async fn discogs_various_artists_is_the_musicbrainz_one_holding_another_discogs_id() {
    let (db, _tmp) = library().await;
    let various = &crate::VARIOUS_ARTISTS;
    add_artist(&db, "various", "Various Artists", Some("discogs-1"), Some(various.musicbrainz))
        .await;

    let resolved =
        resolve(&db, &[credit("credit", "Various Artists", Some(various.discogs), None)]).await;

    assert_eq!(resolved.ids, ["various"]);
    assert!(resolved.inserts.is_empty());
    assert!(
        resolved
            .external_id_updates
            .iter()
            .all(|(_, artist)| artist.discogs_artist_id.as_deref() != Some(various.discogs)),
        "{:?}",
        resolved.external_id_updates
    );
}

/// A credit naming MusicBrainz' Various Artists entry alone, where the
/// artist holding Discogs' entry holds another MusicBrainz id, goes by its
/// MusicBrainz id: a new artist.
#[tokio::test]
async fn musicbrainz_various_artists_is_not_a_discogs_one_holding_another_musicbrainz_id() {
    let (db, _tmp) = library().await;
    let various = &crate::VARIOUS_ARTISTS;
    add_artist(&db, "various", "Various Artists", Some(various.discogs), Some("mb-1")).await;

    let resolved =
        resolve(&db, &[credit("credit", "Various Artists", None, Some(various.musicbrainz))]).await;

    let identity = crate::identity::artist_id(Some(various.musicbrainz), None).unwrap();
    assert_eq!(resolved.ids, [identity.as_str()]);
    assert_eq!(resolved.inserts.len(), 1);
    assert_eq!(resolved.inserts[0].discogs_artist_id, None);
    assert!(resolved.external_id_updates.is_empty(), "{:?}", resolved.external_id_updates);
}

/// One catalog id is one artist: two library artists holding it are merged
/// into the smaller id, which the credit names and which takes the other's
/// ids where it has none.
#[tokio::test]
async fn a_catalog_id_two_library_artists_hold_merges_them() {
    let (db, _tmp) = library().await;
    add_artist(&db, "first", "Artist Name", Some("discogs-1"), None).await;
    add_artist(&db, "second", "Artist Name", Some("discogs-1"), Some("mb-2")).await;

    let resolved = resolve(&db, &[credit("credit", "Artist Name", Some("discogs-1"), None)]).await;

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
#[tokio::test]
async fn a_catalog_id_on_its_own_row_and_on_another_artist_merges_them() {
    let (db, _tmp) = library().await;
    let identity = crate::identity::artist_id(Some("mb-1"), None).unwrap();
    add_artist(&db, &identity, "Artist Name", None, None).await;
    add_artist(&db, "filled-in", "Artist Name", None, Some("mb-1")).await;

    let resolved = resolve(&db, &[credit("credit", "Artist Name", None, Some("mb-1"))]).await;

    let survivor = identity.as_str().min("filled-in");
    assert_eq!(resolved.ids, [survivor]);
    assert_eq!(resolved.merges.len(), 1);
    assert_eq!(resolved.merges[0].survivor, survivor);
}

async fn read_credit(
    db: &bae_store::Store,
    name: &str,
    discogs_artist_id: Option<&str>,
) -> crate::CreditResolution {
    let credit = bae_catalogs::credits::ArtistCredit {
        credited_as: None,
        name: name.to_string(),
        sort_name: None,
        musicbrainz_artist_id: None,
        discogs_artist_id: discogs_artist_id.map(str::to_string),
    };
    db.read(move |sql| resolve_credit_on(&sql, &credit)).await.unwrap()
}

fn artist_ids(artists: &[crate::ExistingArtist]) -> Vec<&str> {
    artists.iter().map(|artist| artist.artist_id.as_str()).collect()
}

#[tokio::test]
async fn a_read_credit_is_new_until_the_library_holds_its_artist() {
    let (db, _tmp) = library().await;
    assert_eq!(read_credit(&db, "Artist Name", None).await, crate::CreditResolution::New);

    add_artist(&db, "library-artist", "Ärtist Name", None, None).await;

    let crate::CreditResolution::Library { artist } = read_credit(&db, "artist name", None).await
    else {
        panic!("the folded name names the library artist");
    };
    assert_eq!(artist.artist_id, "library-artist");
}

#[tokio::test]
async fn a_read_credit_shared_by_two_library_artists_is_ambiguous() {
    let (db, _tmp) = library().await;
    add_artist(&db, "first", "Artist Name", None, None).await;
    add_artist(&db, "second", "Artist Name", None, Some("mb-2")).await;

    let crate::CreditResolution::Ambiguous { artists } =
        read_credit(&db, "Artist Name", None).await
    else {
        panic!("two artists share the name");
    };
    assert_eq!(artist_ids(&artists), ["first", "second"]);
}

#[tokio::test]
async fn a_read_credit_with_another_catalog_id_than_the_named_artist_is_new() {
    let (db, _tmp) = library().await;
    add_artist(&db, "library-artist", "Artist Name", Some("discogs-x"), None).await;

    assert_eq!(
        read_credit(&db, "Artist Name", Some("discogs-y")).await,
        crate::CreditResolution::New
    );
}

/// A read of a credit whose two ids name two artists names the MusicBrainz
/// one, as the write does.
#[tokio::test]
async fn a_read_credit_whose_ids_name_two_artists_is_the_musicbrainz_one() {
    let (db, _tmp) = library().await;
    add_artist(&db, "by-discogs", "Artist One", Some("discogs-1"), None).await;
    add_artist(&db, "by-musicbrainz", "Artist Two", None, Some("mb-1")).await;

    let credit = bae_catalogs::credits::ArtistCredit {
        credited_as: None,
        name: "Artist Name".to_string(),
        sort_name: None,
        musicbrainz_artist_id: Some("mb-1".to_string()),
        discogs_artist_id: Some("discogs-1".to_string()),
    };
    let resolution = db.read(move |sql| resolve_credit_on(&sql, &credit)).await.unwrap();

    let crate::CreditResolution::Library { artist } = resolution else {
        panic!("the MusicBrainz id names the artist, got {resolution:?}");
    };
    assert_eq!(artist.artist_id, "by-musicbrainz");
}

/// A read of a credit whose id two library artists hold names the one the
/// next write merges them into.
#[tokio::test]
async fn a_read_credit_whose_id_two_artists_hold_is_the_smaller_one() {
    let (db, _tmp) = library().await;
    add_artist(&db, "first", "Artist One", Some("discogs-1"), None).await;
    add_artist(&db, "second", "Artist Two", Some("discogs-1"), None).await;

    let crate::CreditResolution::Library { artist } =
        read_credit(&db, "Artist Name", Some("discogs-1")).await
    else {
        panic!("one id names one artist");
    };
    assert_eq!(artist.artist_id, "first");
}

fn make_artist(name: &str, discogs_id: Option<&str>, mb_id: Option<&str>) -> DbArtist {
    let now = Utc::now();
    DbArtist {
        id: coven::IdSource::new_id(&coven::UuidIds).to_string(),
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
    let records = crate::test_fixtures::test_records(&db);
    let existing = make_artist("Artist One", Some("d123"), None);
    records.insert_artist(&existing).await.unwrap();

    let incoming = make_artist("Artist One", Some("d123"), None);
    let resolved = records.find_or_create_artists(std::slice::from_ref(&incoming)).await.unwrap();

    assert_eq!(resolved[0], existing.id);
}

#[tokio::test]
async fn test_same_mb_id_reuses_existing() {
    let (db, _tmp) = temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let records = crate::test_fixtures::test_records(&db);
    let existing = make_artist("Artist One", None, Some("mb-abc"));
    records.insert_artist(&existing).await.unwrap();

    let incoming = make_artist("Artist One", None, Some("mb-abc"));
    let resolved = records.find_or_create_artists(std::slice::from_ref(&incoming)).await.unwrap();

    assert_eq!(resolved[0], existing.id);
}

#[tokio::test]
async fn one_import_reuses_an_artist_already_waiting_to_be_inserted() {
    let (db, _tmp) = temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let records = crate::test_fixtures::test_records(&db);
    let discogs_credit = make_artist("Artist One", Some("d123"), None);
    let cross_linked_credit = make_artist("Artist One", Some("d123"), Some("mb-abc"));

    let resolved = records
        .find_or_create_artists(&[discogs_credit.clone(), cross_linked_credit.clone()])
        .await
        .unwrap();

    // The one new artist carries both ids, and its row id is the one its
    // MusicBrainz id names.
    let artist_id = catalog_artist_id(&cross_linked_credit);
    assert_eq!(resolved, vec![artist_id.clone(), artist_id.clone()]);
    let saved = records.find_artist_by_id(&artist_id).await.unwrap().unwrap();
    assert_eq!(saved.musicbrainz_artist_id.as_deref(), Some("mb-abc"));
}

#[tokio::test]
async fn one_import_reuses_an_idless_artist_name_already_waiting_to_be_inserted() {
    let (db, _tmp) = temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let records = crate::test_fixtures::test_records(&db);
    let album_credit = make_artist("Artist Name", None, None);
    let track_credit = make_artist("Artist Name", None, None);

    let resolved = records
        .find_or_create_artists(&[album_credit.clone(), track_credit.clone()])
        .await
        .unwrap();

    assert_eq!(resolved, vec![album_credit.id.clone(), album_credit.id.clone()]);
    assert!(records.find_artist_by_id(&album_credit.id).await.unwrap().is_some());
    assert!(records.find_artist_by_id(&track_credit.id).await.unwrap().is_none());
}

#[tokio::test]
async fn one_import_combines_two_pending_artists_when_a_later_credit_links_them() {
    let (db, _tmp) = temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let records = crate::test_fixtures::test_records(&db);
    let discogs_credit = make_artist("Artist One", Some("d123"), None);
    let musicbrainz_credit = make_artist("Artist One", None, Some("mb-abc"));
    let cross_linked_credit = make_artist("Artist One", Some("d123"), Some("mb-abc"));

    let resolved = records
        .find_or_create_artists(&[
            discogs_credit.clone(),
            musicbrainz_credit,
            cross_linked_credit.clone(),
        ])
        .await
        .unwrap();

    let artist_id = catalog_artist_id(&cross_linked_credit);
    assert_eq!(resolved, vec![artist_id.clone(), artist_id.clone(), artist_id.clone()]);
    let saved = records.find_artist_by_id(&artist_id).await.unwrap().unwrap();
    assert_eq!(saved.discogs_artist_id.as_deref(), Some("d123"));
    assert_eq!(saved.musicbrainz_artist_id.as_deref(), Some("mb-abc"));
}

#[tokio::test]
async fn a_pending_artist_is_folded_into_a_committed_cross_provider_match() {
    let (db, _tmp) = temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let records = crate::test_fixtures::test_records(&db);
    let existing = make_artist("Artist One", None, Some("mb-abc"));
    records.insert_artist(&existing).await.unwrap();
    let discogs_credit = make_artist("Artist One", Some("d123"), None);
    let cross_linked_credit = make_artist("Artist One", Some("d123"), Some("mb-abc"));

    let resolved =
        records.find_or_create_artists(&[discogs_credit, cross_linked_credit]).await.unwrap();

    assert_eq!(resolved, vec![existing.id.clone(), existing.id.clone()]);
    let saved = records.find_artist_by_id(&existing.id).await.unwrap().unwrap();
    assert_eq!(saved.discogs_artist_id.as_deref(), Some("d123"));
    assert_eq!(saved.musicbrainz_artist_id.as_deref(), Some("mb-abc"));
}

#[tokio::test]
async fn one_import_reuses_an_external_id_waiting_to_be_added_to_an_existing_artist() {
    let (db, _tmp) = temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let records = crate::test_fixtures::test_records(&db);
    let existing = make_artist("Artist One", None, Some("mb-abc"));
    records.insert_artist(&existing).await.unwrap();
    let cross_linked_credit = make_artist("Artist One", Some("d123"), Some("mb-abc"));
    let discogs_credit = make_artist("Artist One", Some("d123"), None);

    let resolved =
        records.find_or_create_artists(&[cross_linked_credit, discogs_credit]).await.unwrap();

    assert_eq!(resolved, vec![existing.id.clone(), existing.id.clone()]);
    let saved = records.find_artist_by_id(&existing.id).await.unwrap().unwrap();
    assert_eq!(saved.discogs_artist_id.as_deref(), Some("d123"));
}

#[tokio::test]
async fn credits_of_one_write_reuse_an_artist_waiting_to_be_inserted() {
    let (db, _tmp) = temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let records = crate::test_fixtures::test_records(&db);
    let discogs_credit = make_artist("Artist One", Some("d123"), None);
    let cross_linked_credit = make_artist("Artist One", Some("d123"), Some("mb-abc"));

    let resolved =
        records.find_or_create_artists(&[discogs_credit, cross_linked_credit]).await.unwrap();

    assert_eq!(resolved, vec![resolved[0].clone(); 2]);
    let saved = records.find_artist_by_id(&resolved[0]).await.unwrap().unwrap();
    assert_eq!(saved.musicbrainz_artist_id.as_deref(), Some("mb-abc"));
}

/// A write crediting ids that name two library artists credits the
/// MusicBrainz one and leaves both artists as they were.
#[tokio::test]
async fn exact_source_ids_naming_two_artists_credit_the_musicbrainz_one() {
    let (db, _tmp) = temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let records = crate::test_fixtures::test_records(&db);
    let discogs_artist = make_artist("Artist One", Some("d123"), None);
    let musicbrainz_artist = make_artist("Artist Two", None, Some("mb-abc"));
    records.insert_artist(&discogs_artist).await.unwrap();
    records.insert_artist(&musicbrainz_artist).await.unwrap();

    let incoming = make_artist("Artist Three", Some("d123"), Some("mb-abc"));
    let resolved = records.find_or_create_artists(std::slice::from_ref(&incoming)).await.unwrap();

    assert_eq!(resolved, std::slice::from_ref(&musicbrainz_artist.id));
    for artist in [&discogs_artist, &musicbrainz_artist] {
        let stored = records.find_artist_by_id(&artist.id).await.unwrap().unwrap();
        assert_eq!(
            (stored.discogs_artist_id, stored.musicbrainz_artist_id),
            (artist.discogs_artist_id.clone(), artist.musicbrainz_artist_id.clone())
        );
    }
    assert!(records.find_artist_by_id(&incoming.id).await.unwrap().is_none());
}

#[tokio::test]
async fn a_same_name_credit_without_ids_is_the_library_artist() {
    let (db, _tmp) = temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let records = crate::test_fixtures::test_records(&db);
    let existing = make_artist("Artist One", None, None);
    records.insert_artist(&existing).await.unwrap();

    let incoming = make_artist("artist one", None, None);
    let resolved = records.find_or_create_artists(std::slice::from_ref(&incoming)).await.unwrap();

    assert_eq!(resolved[0], existing.id);
}

#[tokio::test]
async fn test_same_name_same_mb_id_reuses() {
    let (db, _tmp) = temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let records = crate::test_fixtures::test_records(&db);
    let existing = make_artist("Artist Two", None, Some("mb-artist-two"));
    records.insert_artist(&existing).await.unwrap();

    let incoming = make_artist("Artist Two", None, Some("mb-artist-two"));
    let resolved = records.find_or_create_artists(std::slice::from_ref(&incoming)).await.unwrap();

    assert_eq!(resolved[0], existing.id);
}

#[tokio::test]
async fn test_same_name_different_mb_id_creates_new() {
    let (db, _tmp) = temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let records = crate::test_fixtures::test_records(&db);
    let existing = make_artist("Artist Two", None, Some("mb-artist-two-uk"));
    records.insert_artist(&existing).await.unwrap();

    let incoming = make_artist("Artist Two", None, Some("mb-artist-two-ca"));
    let resolved = records.find_or_create_artists(std::slice::from_ref(&incoming)).await.unwrap();

    // Should create a new artist, not reuse existing
    assert_eq!(resolved[0], catalog_artist_id(&incoming));
    assert_ne!(resolved[0], existing.id);
}

#[tokio::test]
async fn test_same_name_different_discogs_id_creates_new() {
    let (db, _tmp) = temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let records = crate::test_fixtures::test_records(&db);
    let existing = make_artist("Artist Two", Some("d100"), None);
    records.insert_artist(&existing).await.unwrap();

    let incoming = make_artist("Artist Two", Some("d200"), None);
    let resolved = records.find_or_create_artists(std::slice::from_ref(&incoming)).await.unwrap();

    assert_eq!(resolved[0], catalog_artist_id(&incoming));
    assert_ne!(resolved[0], existing.id);
}

#[tokio::test]
async fn ids_of_different_catalogs_do_not_keep_a_name_match_apart() {
    let (db, _tmp) = temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let records = crate::test_fixtures::test_records(&db);
    // Existing has a Discogs ID only; the credit a MusicBrainz ID only.
    // Neither contradicts the other, so the name decides.
    let existing = make_artist("Artist One", Some("d456"), None);
    records.insert_artist(&existing).await.unwrap();

    let incoming = make_artist("Artist One", None, Some("mb-xyz"));
    let resolved = records.find_or_create_artists(std::slice::from_ref(&incoming)).await.unwrap();

    assert_eq!(resolved[0], existing.id, "the artist keeps its own id");
    let linked = records.find_artist_by_id(&existing.id).await.unwrap().unwrap();
    assert_eq!(linked.discogs_artist_id.as_deref(), Some("d456"));
    assert_eq!(linked.musicbrainz_artist_id.as_deref(), Some("mb-xyz"));
    assert!(records.find_artist_by_id(&catalog_artist_id(&incoming)).await.unwrap().is_none());
}

#[tokio::test]
async fn test_new_artist_inserts() {
    let (db, _tmp) = temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let records = crate::test_fixtures::test_records(&db);

    let incoming = make_artist("New Band", Some("d999"), Some("mb-999"));
    let resolved = records.find_or_create_artists(std::slice::from_ref(&incoming)).await.unwrap();

    assert_eq!(resolved[0], catalog_artist_id(&incoming));

    // Verify it's in the DB
    let saved = records.find_artist_by_id(&resolved[0]).await.unwrap().unwrap();
    assert_eq!(saved.name, "New Band");
    assert_eq!(saved.discogs_artist_id.as_deref(), Some("d999"));
    assert_eq!(saved.musicbrainz_artist_id.as_deref(), Some("mb-999"));
}

/// Two devices apart each give one Discogs id to a different artist. Once
/// each has the other's write, the next write crediting that id merges the
/// two into the smaller id, which a later write credits and both devices
/// read the credit as.
#[test]
fn a_catalog_id_two_devices_gave_two_artists_merges_them() {
    bae_store::test_devices::run_two_store_test(
        crate::test_fixtures::TEST_SCHEMA,
        check_two_devices_give_one_artist_id,
    );
}

async fn check_two_devices_give_one_artist_id(devices: &bae_store::test_devices::TwoStores) {
    for (id, name) in [("artist-a", "Artist Name A"), ("artist-b", "Artist Name B")] {
        crate::test_fixtures::test_records(devices.a())
            .insert_artist(&credit(id, name, None, None))
            .await
            .unwrap();
    }
    devices.converge("SELECT id FROM artists").await;

    crate::test_fixtures::artists_apart::merge_apart(
        devices,
        ("Artist Name A", "Artist Name B"),
        "discogs-1",
    )
    .await;

    let credited = crate::test_fixtures::test_records(devices.a())
        .find_or_create_artists(&[credit("credit", "Artist Name", Some("discogs-1"), None)])
        .await
        .unwrap();
    assert_eq!(credited, ["artist-a"]);

    let read = bae_catalogs::credits::ArtistCredit {
        discogs_artist_id: Some("discogs-1".to_string()),
        ..bae_catalogs::credits::ArtistCredit::named("Artist Name")
    };
    for device in [devices.a(), devices.b()] {
        let resolved = crate::test_fixtures::test_records(device)
            .resolve_artist_credits(std::slice::from_ref(&read))
            .await
            .unwrap();
        let crate::CreditResolution::Library { artist } = &resolved[0].resolution else {
            panic!("the id names one artist, got {:?}", resolved[0].resolution);
        };
        assert_eq!(artist.artist_id, "artist-a");
    }
}

/// A refusal whose write then fails to roll back, or to remove the bytes it
/// staged, stays the refusal it is, with that failure beside it.
#[test]
fn a_refusal_keeps_its_type_when_its_rollback_or_cleanup_fails() {
    use coven::WriteFailure;
    let refused = || IdentityWriteError::from(IdentityRefusal::ArtistGone("gone".to_string()));

    let rolled_back = refused().with_rollback(rusqlite::Error::InvalidQuery);
    let IdentityWriteError::Rollback { operation, .. } = &rolled_back else {
        panic!("a failed rollback keeps the refusal under it, got {rolled_back:?}");
    };
    assert!(matches!(**operation, IdentityWriteError::Refused(_)), "{operation:?}");

    let cleaned = IdentityWriteError::with_cleanup(
        refused(),
        vec![coven::DbError::Sqlite(rusqlite::Error::InvalidQuery)],
    );
    let IdentityWriteError::FileCleanup { write, failures } = &cleaned else {
        panic!("a failed byte removal keeps the refusal under it, got {cleaned:?}");
    };
    assert!(matches!(**write, IdentityWriteError::Refused(_)), "{write:?}");
    assert_eq!(failures.len(), 1);
}
