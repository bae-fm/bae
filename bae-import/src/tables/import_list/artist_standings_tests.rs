//! For each credit, the standing clauses over a draft's credits resolve the
//! same artist as the rule every write commits by, over the libraries the
//! rule's own tests build: a name, a catalog id, ids that disagree, ids two
//! artists hold, an artist a merge absorbed, and Various Artists in both
//! catalogs. And each field's standing follows its artists'.

use super::*;
use crate::identification::tests::fixed_now;
use crate::tables::import_state::tests::*;
use crate::{ArtistAssignment, Credited, RawTrackEdit, TrackArtistAssignments};
use bae_catalogs::credits::ArtistCredit;
use bae_records::{CreditResolution, DbArtist, ExistingArtist, ResolvedCredit};
use bae_store::test_fixtures::empty_db;

fn artist(id: &str, name: &str, discogs: Option<&str>, musicbrainz: Option<&str>) -> DbArtist {
    DbArtist {
        id: id.to_string(),
        name: name.to_string(),
        sort_name: id.len().is_multiple_of(2).then_some(format!("{name}, sorted")),
        discogs_artist_id: discogs.map(str::to_string),
        musicbrainz_artist_id: musicbrainz.map(str::to_string),
        created_at: fixed_now(),
    }
}

fn credit(name: &str, discogs: Option<&str>, musicbrainz: Option<&str>) -> ArtistCredit {
    ArtistCredit {
        credited_as: None,
        name: name.to_string(),
        sort_name: None,
        musicbrainz_artist_id: musicbrainz.map(str::to_string),
        discogs_artist_id: discogs.map(str::to_string),
    }
}

/// Write `artists`, write each credit of `merging` as an import commits it
/// (which merges artists holding one id), then give the draft's album the
/// `credits` and its one track the `track_credits`: the rule and the
/// statement read them.
async fn answers_alike(
    artists: Vec<DbArtist>,
    merging: Vec<DbArtist>,
    credits: Vec<ArtistCredit>,
    track_credits: Vec<ArtistCredit>,
) -> Vec<bae_records::ResolvedCredit> {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let records = bae_records::test_fixtures::test_records(&db);
    for artist in &artists {
        records.insert_artist(artist).await.unwrap();
    }
    if !merging.is_empty() {
        records.find_or_create_artists(&merging).await.unwrap();
    }
    let (_, hash) = stored_pane_candidate(&db).await;
    let controls = PaneControls::of_pane_candidate(&db);
    let album: Vec<Credited> = credits
        .iter()
        .map(|credit| Credited::plain(ArtistAssignment::Credit { credit: credit.clone() }))
        .collect();
    controls.set_album_artists(&hash, &album).await.unwrap();
    if !track_credits.is_empty() {
        let stored =
            ImportTables::new(db.clone()).load_import_candidate_pane_rows(&hash).await.unwrap();
        controls
            .edit_track(
                &hash,
                &RawTrackEdit {
                    id: stored.draft.tracks[0].edit.id.clone(),
                    title: stored.draft.tracks[0].edit.title.clone(),
                    artist_assignments: TrackArtistAssignments::Explicit(
                        track_credits
                            .iter()
                            .map(|credit| {
                                Credited::plain(ArtistAssignment::Credit { credit: credit.clone() })
                            })
                            .collect(),
                    ),
                    file: Some(stored.draft.tracks[0].edit.file.clone()),
                },
            )
            .await
            .unwrap();
    }

    let read_hash = hash.clone();
    let stated = db
        .read(move |sql| {
            resolved_on(
                &sql,
                CreditSource::Candidate(crate::tables::import_state::OneCandidate::Hash(
                    &read_hash,
                )),
            )
        })
        .await
        .unwrap();
    let every: Vec<ArtistCredit> = credits.iter().chain(&track_credits).cloned().collect();
    let ruled = db
        .read(move |sql| {
            bae_records::records::artist_resolution::resolve_credits_with(&every, |credit| {
                bae_records::records::artist_resolution::resolve_credit_on(&sql, credit)
            })
        })
        .await
        .unwrap();
    assert_eq!(stated, ruled);
    stated
}

/// What the clauses resolve each distinct credit of `source` to, in the
/// order the draft lists them: the `resolved` clause the standings join.
fn resolved_on(
    sql: &impl coven::SqlRead,
    source: CreditSource<'_>,
) -> Result<Vec<ResolvedCredit>, DbError> {
    Ok(sql.query(
        &format!("WITH {} SELECT * FROM resolved ORDER BY idx", artist_standing_clauses(source)),
        source.params().as_slice(),
        |row| {
            let artists: Vec<ExistingArtist> = match row.get::<_, Option<String>>("artists")? {
                Some(json) => serde_json::from_str(&json).unwrap(),
                None => Vec::new(),
            };
            let resolution = match row.get::<_, String>("kind")?.as_str() {
                "new" => CreditResolution::New,
                "library" => CreditResolution::Library {
                    artist: artists.into_iter().next().expect("one artist"),
                },
                "ambiguous" => CreditResolution::Ambiguous { artists },
                other => panic!("a {other} resolution"),
            };
            Ok(ResolvedCredit {
                credit: ArtistCredit {
                    credited_as: row.get("credited_as")?,
                    name: row.get("name")?,
                    sort_name: row.get("sort_name")?,
                    musicbrainz_artist_id: row.get("mb")?,
                    discogs_artist_id: row.get("dg")?,
                },
                resolution,
            })
        },
    )?)
}

fn resolution(resolved: &[bae_records::ResolvedCredit], at: usize) -> &CreditResolution {
    &resolved[at].resolution
}

#[tokio::test]
async fn a_name_is_the_library_artist_of_that_name_however_it_is_cased_or_accented() {
    let resolved = answers_alike(
        vec![artist("library-artist", "Ärtist Name", None, None)],
        vec![],
        vec![credit("artist name", None, None), credit("Somebody Else", None, None)],
        vec![],
    )
    .await;
    assert!(matches!(resolution(&resolved, 0), CreditResolution::Library { .. }));
    assert_eq!(resolution(&resolved, 1), &CreditResolution::New);
}

#[tokio::test]
async fn a_name_two_artists_share_is_ambiguous_unless_a_catalog_id_tells_them_apart() {
    let library = vec![
        artist("first", "Artist Name", None, None),
        artist("second", "Artist Name", None, Some("mb-2")),
    ];
    let resolved = answers_alike(
        library,
        vec![],
        vec![
            credit("Artist Name", None, None),
            credit("Artist Name", None, Some("mb-3")),
            credit("Artist Name", None, Some("mb-2")),
        ],
        vec![],
    )
    .await;
    assert!(matches!(resolution(&resolved, 0), CreditResolution::Ambiguous { .. }));
    // The second artist holds a different MusicBrainz id, so the credit
    // matches only the first artist.
    assert!(
        matches!(resolution(&resolved, 1), CreditResolution::Library { artist } if artist.artist_id == "first")
    );
    assert!(
        matches!(resolution(&resolved, 2), CreditResolution::Library { artist } if artist.artist_id == "second")
    );
}

#[tokio::test]
async fn a_catalog_id_wins_over_the_name_and_a_name_with_another_id_is_new() {
    let resolved = answers_alike(
        vec![
            artist("named", "Artist Name", None, None),
            artist("catalogued", "Other Name", Some("discogs-1"), None),
            artist("other", "Third Name", Some("discogs-x"), None),
        ],
        vec![],
        vec![
            credit("Artist Name", Some("discogs-1"), None),
            credit("Third Name", Some("discogs-y"), None),
        ],
        vec![],
    )
    .await;
    assert!(
        matches!(resolution(&resolved, 0), CreditResolution::Library { artist } if artist.artist_id == "catalogued")
    );
    assert_eq!(resolution(&resolved, 1), &CreditResolution::New);
}

#[tokio::test]
async fn ids_that_name_two_artists_leave_the_musicbrainz_one() {
    let resolved = answers_alike(
        vec![
            artist("by-discogs", "Artist One", Some("discogs-1"), None),
            artist("by-musicbrainz", "Artist Two", None, Some("mb-1")),
        ],
        vec![],
        vec![credit("Artist Name", Some("discogs-1"), Some("mb-1"))],
        vec![],
    )
    .await;
    assert!(
        matches!(resolution(&resolved, 0), CreditResolution::Library { artist } if artist.artist_id == "by-musicbrainz")
    );
}

#[tokio::test]
async fn a_musicbrainz_artist_holding_another_discogs_id_is_the_credit_and_a_discogs_artist_holding_another_musicbrainz_id_is_not(
) {
    let resolved = answers_alike(
        vec![
            artist("mb-artist", "Artist One", Some("discogs-other"), Some("mb-1")),
            artist("dg-artist", "Artist Two", Some("discogs-2"), Some("mb-other")),
        ],
        vec![],
        vec![
            credit("Artist Name", Some("discogs-1"), Some("mb-1")),
            credit("Artist Name", Some("discogs-2"), Some("mb-2")),
            credit("Artist Name", Some("discogs-2"), None),
            credit("Artist Name", None, Some("mb-other")),
        ],
        vec![],
    )
    .await;
    assert!(
        matches!(resolution(&resolved, 0), CreditResolution::Library { artist } if artist.artist_id == "mb-artist")
    );
    assert_eq!(resolution(&resolved, 1), &CreditResolution::New);
}

#[tokio::test]
async fn an_id_two_artists_hold_names_the_smaller_one_filled_with_what_the_other_has() {
    let resolved = answers_alike(
        vec![
            artist("first", "Artist One", Some("discogs-1"), None),
            artist("second", "Artist Two", Some("discogs-1"), Some("mb-2")),
        ],
        vec![],
        vec![
            credit("Artist Name", Some("discogs-1"), None),
            credit("Artist Name", None, Some("mb-2")),
        ],
        vec![],
    )
    .await;
    assert!(
        matches!(resolution(&resolved, 0), CreditResolution::Library { artist } if artist.artist_id == "first" && artist.musicbrainz_artist_id.as_deref() == Some("mb-2"))
    );
}

#[tokio::test]
async fn an_id_held_on_its_own_row_and_on_another_names_the_smaller_one() {
    let identity = bae_records::identity::artist_id(Some("mb-derived"), None).unwrap();
    let resolved = answers_alike(
        vec![
            artist("zzz-other", "Artist Other", None, Some("mb-derived")),
            artist(&identity, "Artist Named By Id", None, None),
        ],
        vec![],
        vec![credit("Whoever", None, Some("mb-derived"))],
        vec![],
    )
    .await;
    assert!(matches!(resolution(&resolved, 0), CreditResolution::Library { .. }));
}

#[tokio::test]
async fn an_absorbed_artist_is_the_artist_it_shows_as_and_not_matched_by_its_name() {
    let resolved = answers_alike(
        vec![
            artist("first", "Other Name", Some("discogs-1"), None),
            artist("second", "Artist Name", Some("discogs-1"), None),
        ],
        vec![artist("merging", "Other Name", Some("discogs-1"), None)],
        vec![
            credit("Artist Name", None, None),
            credit("Other Name", None, None),
            credit("Whoever", Some("discogs-1"), None),
        ],
        vec![],
    )
    .await;
    assert_eq!(resolution(&resolved, 0), &CreditResolution::New);
    assert!(matches!(resolution(&resolved, 1), CreditResolution::Library { .. }));
    assert!(matches!(resolution(&resolved, 2), CreditResolution::Library { .. }));
}

#[tokio::test]
async fn various_artists_is_one_artist_in_both_catalogs() {
    let va = bae_records::VARIOUS_ARTISTS;
    let resolved = answers_alike(
        vec![
            artist(
                "va-musicbrainz",
                "Various Artists",
                Some("discogs-other"),
                Some(va.musicbrainz),
            ),
            artist("not-va", "Somebody", Some("discogs-3"), Some("mb-3")),
        ],
        vec![],
        vec![
            credit("Various", Some(va.discogs), None),
            credit("Various", None, Some(va.musicbrainz)),
            credit("Various", Some(va.discogs), Some(va.musicbrainz)),
            credit("Whoever", None, Some("mb-3")),
        ],
        vec![],
    )
    .await;
    assert!(matches!(resolution(&resolved, 1), CreditResolution::Library { .. }));
}

#[tokio::test]
async fn various_artists_by_discogs_alone_places_nobody_when_no_artist_holds_either_id() {
    let va = bae_records::VARIOUS_ARTISTS;
    let resolved = answers_alike(
        vec![artist("va-named", "Various", None, None)],
        vec![],
        vec![credit("Various", Some(va.discogs), None)],
        vec![],
    )
    .await;
    assert!(matches!(resolution(&resolved, 0), CreditResolution::Library { .. }));
}

#[tokio::test]
async fn a_track_s_own_credits_follow_the_album_s_and_a_repeated_credit_is_read_once() {
    let resolved = answers_alike(
        vec![artist("library-artist", "Artist Name", None, None)],
        vec![],
        vec![credit("Artist Name", None, None), credit("New Person", None, None)],
        vec![credit("Artist Name", None, None), credit("Guest", None, None)],
    )
    .await;
    assert_eq!(resolved.len(), 3, "the album's two credits and the track's guest");
    assert_eq!(resolved[2].credit.name, "Guest");
}

/// The standings of the candidate `hash`'s album artists and of its first
/// track's own artists.
async fn standings_of(
    db: &bae_store::Store,
    hash: &str,
) -> (Option<ArtistFieldStanding>, Option<ArtistFieldStanding>) {
    let hash = hash.to_string();
    db.read(move |sql| {
        let source =
            CreditSource::Candidate(crate::tables::import_state::OneCandidate::Hash(&hash));
        let album = album_artist_standing_on(&sql, source)?;
        let track: Option<String> = sql
            .query_row(
                &format!(
                    "WITH {} SELECT standing FROM artist_field_standing WHERE part = 1",
                    artist_standing_clauses(source)
                ),
                source.params().as_slice(),
                |row| row.get(0),
            )
            .optional()?;
        Ok((album, track.as_deref().map(artist_field_standing_of_json).transpose()?))
    })
    .await
    .unwrap()
}

/// A picked artist stands in the library; each credit stands as it
/// resolved, in the field's order; and the field counts its artists: all in
/// the library, all new, some new, one with choices, or some to choose.
#[tokio::test]
async fn each_field_counts_how_its_artists_stand() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let records = bae_records::test_fixtures::test_records(&db);
    for artist in [
        artist("library-artist", "Library Name", None, None),
        artist("shared-1", "Shared Name", None, None),
        artist("shared-2", "Shared Name", None, None),
    ] {
        records.insert_artist(&artist).await.unwrap();
    }
    let (_, hash) = stored_pane_candidate(&db).await;
    let controls = PaneControls::of_pane_candidate(&db);
    let picked = || {
        Credited::plain(ArtistAssignment::picked(ExistingArtist {
            artist_id: "library-artist".to_string(),
            name: "Library Name".to_string(),
            sort_name: None,
            musicbrainz_artist_id: None,
            discogs_artist_id: None,
        }))
    };
    let named = |name: &str| Credited::plain(ArtistAssignment::named(name));
    let shared = || ArtistStanding::Choose {
        choices: ["shared-1", "shared-2"]
            .map(|id| ExistingArtist {
                artist_id: id.to_string(),
                name: "Shared Name".to_string(),
                sort_name: id.len().is_multiple_of(2).then_some("Shared Name, sorted".to_string()),
                musicbrainz_artist_id: None,
                discogs_artist_id: None,
            })
            .to_vec(),
    };

    for (album, field, artists) in [
        (
            vec![picked(), named("library name")],
            ArtistsStanding::Library,
            vec![ArtistStanding::Library, ArtistStanding::Library],
        ),
        (
            vec![named("New One"), named("New Two")],
            ArtistsStanding::New,
            vec![ArtistStanding::New, ArtistStanding::New],
        ),
        (
            vec![picked(), named("New One")],
            ArtistsStanding::SomeNew { count: 1 },
            vec![ArtistStanding::Library, ArtistStanding::New],
        ),
        (vec![named("Shared Name")], ArtistsStanding::Choose { choices: 2 }, vec![shared()]),
        (
            vec![picked(), named("Shared Name"), named("New One")],
            ArtistsStanding::SomeToChoose { count: 1 },
            vec![ArtistStanding::Library, shared(), ArtistStanding::New],
        ),
    ] {
        controls.set_album_artists(&hash, &album).await.unwrap();
        let (album_standing, track_standing) = standings_of(&db, &hash).await;
        assert_eq!(album_standing, Some(ArtistFieldStanding { field, artists }));
        assert_eq!(track_standing, None, "the track takes its album's artists");
    }
}

/// A track's own artists are a field of their own, apart from the album's.
#[tokio::test]
async fn a_track_s_own_artists_stand_apart_from_the_album_s() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let (_, hash) = stored_pane_candidate(&db).await;
    let controls = PaneControls::of_pane_candidate(&db);
    controls
        .set_album_artists(&hash, &[Credited::plain(ArtistAssignment::named("Album Artist"))])
        .await
        .unwrap();
    let stored =
        ImportTables::new(db.clone()).load_import_candidate_pane_rows(&hash).await.unwrap();
    controls
        .edit_track(
            &hash,
            &RawTrackEdit {
                id: stored.draft.tracks[0].edit.id.clone(),
                title: stored.draft.tracks[0].edit.title.clone(),
                artist_assignments: TrackArtistAssignments::Explicit(Credited::plain_list([
                    ArtistAssignment::named("Guest One"),
                    ArtistAssignment::named("Guest Two"),
                ])),
                file: Some(stored.draft.tracks[0].edit.file.clone()),
            },
        )
        .await
        .unwrap();

    let (album, track) = standings_of(&db, &hash).await;
    assert_eq!(
        album,
        Some(ArtistFieldStanding {
            field: ArtistsStanding::New,
            artists: vec![ArtistStanding::New]
        })
    );
    assert_eq!(
        track,
        Some(ArtistFieldStanding {
            field: ArtistsStanding::New,
            artists: vec![ArtistStanding::New, ArtistStanding::New]
        })
    );
}
