use super::*;

const NOW: &str = "2026-01-01T00:00:00Z";

fn library() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(include_str!("../../../migrations/001_initial.sql"))
        .unwrap();
    conn
}

fn add_artist(
    conn: &Connection,
    id: &str,
    name: &str,
    discogs_artist_id: Option<&str>,
    musicbrainz_artist_id: Option<&str>,
) {
    conn.execute(
        "INSERT INTO artists (id, name, name_key, discogs_artist_id, musicbrainz_artist_id, \
         _updated_at, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
        params![
            id,
            name,
            crate::util::text::normalize(name),
            discogs_artist_id,
            musicbrainz_artist_id,
            NOW,
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
        created_at: chrono::DateTime::parse_from_rfc3339(NOW)
            .unwrap()
            .with_timezone(&Utc),
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

    let resolved = resolve(
        &conn,
        &[credit("credit", "Artist Name", Some("discogs-1"), None)],
    );

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
    add_artist(
        &conn,
        "library-artist",
        "Artist Name",
        Some("discogs-x"),
        None,
    );

    let resolved = resolve(
        &conn,
        &[credit("credit", "Artist Name", Some("discogs-y"), None)],
    );

    let identity = crate::db::identity::artist_id(None, Some("discogs-y")).unwrap();
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

    let resolved = resolve(
        &conn,
        &[credit("credit", "Artist Name", Some("discogs-1"), None)],
    );

    assert_eq!(resolved.ids, ["catalogued"]);
}

#[test]
fn an_absorbed_artist_is_not_matched_by_its_name() {
    let conn = library();
    add_artist(&conn, "absorbed", "Artist Name", None, None);
    add_artist(&conn, "survivor", "Other Name", None, None);
    conn.execute(
        "INSERT INTO artist_merges (id, into_artist_id, _updated_at, created_at) \
         VALUES ('absorbed', 'survivor', ?, ?)",
        params![NOW, NOW],
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

    let identity = crate::db::identity::artist_id(None, Some("discogs-1")).unwrap();
    assert_eq!(resolved.ids, [identity.clone(), identity.clone()]);
    assert_eq!(resolved.inserts.len(), 1);
    assert_eq!(
        resolved.inserts[0].discogs_artist_id.as_deref(),
        Some("discogs-1")
    );
}

#[test]
fn a_picked_artist_that_is_gone_refuses_the_write() {
    let conn = library();
    let picked = HashSet::from(["gone".to_string()]);

    let error = resolve_artists_on(&conn, &[credit("gone", "Artist Name", None, None)], &picked)
        .unwrap_err();

    assert!(
        matches!(error, ArtistWriteError::Unresolvable(_)),
        "{error}"
    );
}

#[test]
fn two_artists_holding_the_credits_two_ids_are_the_identity_conflict() {
    let conn = library();
    add_artist(&conn, "by-discogs", "Artist Name", Some("discogs-1"), None);
    add_artist(&conn, "by-musicbrainz", "Artist Name", None, Some("mb-1"));

    let error = resolve_artists_on(
        &conn,
        &[credit(
            "credit",
            "Artist Name",
            Some("discogs-1"),
            Some("mb-1"),
        )],
        &HashSet::new(),
    )
    .unwrap_err();

    let ArtistWriteError::IdentityConflict(conflict) = error else {
        panic!("expected the identity conflict, got {error}");
    };
    assert_eq!(conflict.discogs_artist.artist_id, "by-discogs");
    assert_eq!(conflict.musicbrainz_artist.artist_id, "by-musicbrainz");
}

fn read_credit(
    conn: &Connection,
    name: &str,
    discogs_artist_id: Option<&str>,
) -> crate::import::CreditResolution {
    resolve_credit_on(
        conn,
        &crate::import::ArtistCredit {
            name: name.to_string(),
            sort_name: None,
            musicbrainz_artist_id: None,
            discogs_artist_id: discogs_artist_id.map(str::to_string),
        },
    )
    .unwrap()
}

fn artist_ids(artists: &[crate::import::ExistingArtist]) -> Vec<&str> {
    artists
        .iter()
        .map(|artist| artist.artist_id.as_str())
        .collect()
}

#[test]
fn a_read_credit_is_new_until_the_library_holds_its_artist() {
    let conn = library();
    assert_eq!(
        read_credit(&conn, "Artist Name", None),
        crate::import::CreditResolution::New
    );

    add_artist(&conn, "library-artist", "Ärtist Name", None, None);

    let crate::import::CreditResolution::Library { artist } =
        read_credit(&conn, "artist name", None)
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

    let crate::import::CreditResolution::Ambiguous { artists } =
        read_credit(&conn, "Artist Name", None)
    else {
        panic!("two artists share the name");
    };
    assert_eq!(artist_ids(&artists), ["first", "second"]);
}

#[test]
fn a_read_credit_with_another_catalog_id_than_the_named_artist_is_new() {
    let conn = library();
    add_artist(
        &conn,
        "library-artist",
        "Artist Name",
        Some("discogs-x"),
        None,
    );

    assert_eq!(
        read_credit(&conn, "Artist Name", Some("discogs-y")),
        crate::import::CreditResolution::New
    );
}

#[test]
fn a_read_credit_whose_ids_name_two_artists_is_conflicting() {
    let conn = library();
    add_artist(&conn, "by-discogs", "Artist One", Some("discogs-1"), None);
    add_artist(&conn, "by-musicbrainz", "Artist Two", None, Some("mb-1"));

    let resolution = resolve_credit_on(
        &conn,
        &crate::import::ArtistCredit {
            name: "Artist Name".to_string(),
            sort_name: None,
            musicbrainz_artist_id: Some("mb-1".to_string()),
            discogs_artist_id: Some("discogs-1".to_string()),
        },
    )
    .unwrap();

    let crate::import::CreditResolution::Conflicting { artists } = resolution else {
        panic!("the credit's two ids name two artists");
    };
    assert_eq!(artist_ids(&artists), ["by-discogs", "by-musicbrainz"]);
}
