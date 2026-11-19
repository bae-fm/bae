//! Each credit a draft holds has the records' rule's answer stored after
//! every write that can change it: the credit's own writes, an import's
//! artists, an artist's other printed names, ids it gains, merges, a deleted
//! artist, and another device's artist write as this device applies it. A
//! credit no draft holds has no answer.

use crate::identification::tests::fixed_now;
use crate::tables::import_state::tests::*;
use crate::tables::ImportTables;
use crate::{ArtistAssignment, Credited, RawTrackEdit, TrackArtistAssignments};
use bae_catalogs::credits::ArtistCredit;
use bae_records::{DbArtist, ExistingArtist};
use bae_store::test_fixtures::empty_db;
use bae_store::{DbError, Store};

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

/// A credit that a write of the records' own makes into the artist it names.
fn crediting(name: &str, discogs: Option<&str>, musicbrainz: Option<&str>) -> DbArtist {
    DbArtist { id: format!("credit-{name}"), ..artist("", name, discogs, musicbrainz) }
}

fn credited(credits: &[ArtistCredit]) -> Vec<Credited> {
    credits
        .iter()
        .map(|credit| Credited::plain(ArtistAssignment::Credit { credit: credit.clone() }))
        .collect()
}

fn key_of(credit: &ArtistCredit) -> CreditKey {
    CreditKey {
        name_key: bae_types::text_match::artist_name_key(&credit.name),
        musicbrainz_artist_id: credit.musicbrainz_artist_id.clone(),
        discogs_artist_id: credit.discogs_artist_id.clone(),
    }
}

/// A credit as the rule reads it: its folded name and its catalog ids.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CreditKey {
    name_key: String,
    musicbrainz_artist_id: Option<String>,
    discogs_artist_id: Option<String>,
}

/// How a test reads an answer: the library artists it names, as none, one or
/// several.
#[derive(Debug, Clone, PartialEq, Eq)]
enum CreditResolution {
    Library { artist: ExistingArtist },
    New,
    Ambiguous { artists: Vec<ExistingArtist> },
}

fn resolution_of(mut artists: Vec<ExistingArtist>) -> CreditResolution {
    match artists.len() {
        0 => CreditResolution::New,
        1 => CreditResolution::Library { artist: artists.remove(0) },
        _ => CreditResolution::Ambiguous { artists },
    }
}

/// Each credit's stored answer, after checking in one read that every credit
/// a draft holds has the rule's answer for it stored, that no
/// credit none holds has one, and that none is left due.
async fn stored_answers(db: &Store) -> Answers {
    db.read(|sql| {
        let held = sql.query(
            "SELECT MIN(name), musicbrainz_artist_id, discogs_artist_id FROM ( \
                 SELECT name, musicbrainz_artist_id, discogs_artist_id \
                 FROM import_candidate_album_artist_assignment WHERE assignment_kind = 'credit' \
                 UNION ALL SELECT name, musicbrainz_artist_id, discogs_artist_id \
                 FROM import_candidate_track_artist_assignment WHERE assignment_kind = 'credit') \
             GROUP BY bae_artist_name_key(name), musicbrainz_artist_id, discogs_artist_id",
            [],
            |row| {
                let (name, musicbrainz, discogs): (String, Option<String>, Option<String>) =
                    (row.get(0)?, row.get(1)?, row.get(2)?);
                Ok(credit(&name, discogs.as_deref(), musicbrainz.as_deref()))
            },
        )?;
        let stored = sql.query(
            "SELECT name_key, musicbrainz_artist_id, discogs_artist_id, \
                    (SELECT json_group_array(json_object( \
                         'artist_id', a.artist_id, 'name', a.name, 'sort_name', a.sort_name, \
                         'musicbrainz_artist_id', a.musicbrainz_artist_id, \
                         'discogs_artist_id', a.discogs_artist_id) ORDER BY a.position) \
                     FROM import_credit_resolution_artist AS a WHERE a.resolution = r.id) \
             FROM import_credit_resolution AS r",
            [],
            |row| {
                let key = CreditKey {
                    name_key: row.get(0)?,
                    musicbrainz_artist_id: row.get(1)?,
                    discogs_artist_id: row.get(2)?,
                };
                let artists: Vec<ExistingArtist> =
                    serde_json::from_str(&row.get::<_, String>(3)?).unwrap();
                Ok((key, resolution_of(artists)))
            },
        )?;
        let due: i64 =
            sql.query_row("SELECT COUNT(*) FROM import_credit_resolution_due", [], |row| {
                row.get(0)
            })?;
        assert_eq!(due, 0, "no credit is left due after a commit");
        assert_eq!(stored.len(), held.len(), "one answer per credit held: {stored:?}");
        let mut answers = Vec::new();
        for credit in &held {
            let ruled = resolution_of(bae_records::credit_artists_checked_on(&sql, credit)?);
            let (_, answer) = stored
                .iter()
                .find(|(key, _)| *key == key_of(credit))
                .unwrap_or_else(|| panic!("an answer stored for {credit:?}"));
            assert_eq!(answer, &ruled, "the stored answer for {credit:?} is the rule's");
            answers.push((key_of(credit), ruled));
        }
        Ok::<_, DbError>(Answers(answers))
    })
    .await
    .unwrap()
}

/// The stored answers, by credit.
struct Answers(Vec<(CreditKey, CreditResolution)>);

impl Answers {
    /// The stored answer for `credit`.
    fn of(&self, credit: &ArtistCredit) -> &CreditResolution {
        let key = key_of(credit);
        self.0
            .iter()
            .find(|(stored, _)| *stored == key)
            .map(|(_, answer)| answer)
            .unwrap_or_else(|| panic!("an answer stored for {credit:?}"))
    }

    fn len(&self) -> usize {
        self.0.len()
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// The stored answer for the credit of `name` with no catalog id.
async fn answer(db: &Store, name: &str) -> CreditResolution {
    stored_answers(db).await.of(&credit(name, None, None)).clone()
}

/// Whether `answer` names the library artist `id` alone.
fn names(answer: &CreditResolution, id: &str) -> bool {
    matches!(answer, CreditResolution::Library { artist } if artist.artist_id == id)
}

/// The library holds `artists` and, when `merging` is given, each credit of it
/// written as an import writes it (which merges artists holding one id); the
/// draft's album credits `credits` and its one track `track_credits`. Returns
/// each credit's stored answer, which [`stored_answers`] checks is the
/// rule's.
async fn answers_after(
    artists: Vec<DbArtist>,
    merging: Vec<DbArtist>,
    credits: Vec<ArtistCredit>,
    track_credits: Vec<ArtistCredit>,
) -> Vec<CreditResolution> {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let records = bae_records::test_fixtures::test_records(&db);
    for artist in &artists {
        records.insert_artist(artist).await.unwrap();
    }
    let (_, hash) = stored_pane_candidate(&db).await;
    let controls = PaneControls::of_pane_candidate(&db);
    controls.set_album_artists(&hash, &credited(&credits)).await.unwrap();
    if !track_credits.is_empty() {
        set_track_credits(&db, &hash, &track_credits).await;
    }
    if !merging.is_empty() {
        records.find_or_create_artists(&merging).await.unwrap();
    }
    let answers = stored_answers(&db).await;
    credits.iter().chain(&track_credits).map(|credit| answers.of(credit).clone()).collect()
}

/// Give the pane candidate's one track its own credits `credits`, or the
/// album's when there are none.
async fn set_track_credits(db: &Store, hash: &str, credits: &[ArtistCredit]) {
    let stored = ImportTables::new(db.clone()).load_import_candidate_pane_rows(hash).await.unwrap();
    let track = &stored.draft.tracks[0].edit;
    PaneControls::of_pane_candidate(db)
        .edit_track(
            hash,
            &RawTrackEdit {
                id: track.id.clone(),
                title: track.title.clone(),
                artist_assignments: match credits {
                    [] => TrackArtistAssignments::AlbumArtists,
                    credits => TrackArtistAssignments::Explicit(credited(credits)),
                },
                file: Some(track.file.clone()),
            },
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn a_name_is_the_library_artist_of_that_name_however_it_is_cased_or_accented() {
    let answers = answers_after(
        vec![artist("library-artist", "Ärtist Name", None, None)],
        vec![],
        vec![credit("artist name", None, None), credit("Somebody Else", None, None)],
        vec![],
    )
    .await;
    assert!(names(&answers[0], "library-artist"));
    assert_eq!(answers[1], CreditResolution::New);
}

#[tokio::test]
async fn a_name_two_artists_share_is_ambiguous_unless_a_catalog_id_tells_them_apart() {
    let answers = answers_after(
        vec![
            artist("first", "Artist Name", None, None),
            artist("second", "Artist Name", None, Some("mb-2")),
        ],
        vec![],
        vec![
            credit("Artist Name", None, None),
            credit("Artist Name", None, Some("mb-3")),
            credit("Artist Name", None, Some("mb-2")),
        ],
        vec![],
    )
    .await;
    assert!(matches!(&answers[0], CreditResolution::Ambiguous { artists } if artists.len() == 2));
}

#[tokio::test]
async fn an_id_two_artists_hold_names_the_smaller_one_filled_with_what_the_other_has() {
    let answers = answers_after(
        vec![
            artist("first", "Artist One", Some("discogs-1"), None),
            artist("second", "Artist Two", Some("discogs-1"), Some("mb-2")),
        ],
        vec![],
        vec![credit("Artist Name", Some("discogs-1"), None)],
        vec![],
    )
    .await;
    assert!(
        matches!(&answers[0], CreditResolution::Library { artist } if artist.artist_id == "first" && artist.musicbrainz_artist_id.as_deref() == Some("mb-2"))
    );
}

/// An import's write that merges two artists holding one id leaves the
/// absorbed artist's name naming nobody, and the id the survivor.
#[tokio::test]
async fn a_merge_moves_the_answers_of_the_absorbed_artist() {
    let answers = answers_after(
        vec![
            artist("first", "Other Name", Some("discogs-1"), None),
            artist("second", "Artist Name", Some("discogs-1"), None),
        ],
        vec![crediting("Other Name", Some("discogs-1"), None)],
        vec![credit("Artist Name", None, None), credit("Whoever", Some("discogs-1"), None)],
        vec![],
    )
    .await;
    assert_eq!(answers[0], CreditResolution::New);
    assert!(names(&answers[1], "first"));
}

/// A credit naming a track's own artist is answered as the album's are, and
/// a credit the album and the track both hold keeps its answer until the
/// last of them goes, with the write that removes it.
#[tokio::test]
async fn a_credit_keeps_its_answer_while_a_draft_holds_it() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let (_, hash) = stored_pane_candidate(&db).await;
    let controls = PaneControls::of_pane_candidate(&db);
    let guest = credit("Guest", None, None);
    controls.set_album_artists(&hash, &credited(std::slice::from_ref(&guest))).await.unwrap();
    set_track_credits(&db, &hash, &[guest.clone(), credit("Other Guest", None, None)]).await;
    assert_eq!(stored_answers(&db).await.len(), 2);

    let album_artist = credit("Album Artist", None, None);
    controls
        .set_album_artists(&hash, &credited(std::slice::from_ref(&album_artist)))
        .await
        .unwrap();
    assert_eq!(answer(&db, "Guest").await, CreditResolution::New);

    set_track_credits(&db, &hash, &[]).await;
    let answers = stored_answers(&db).await;
    assert_eq!(answers.len(), 1, "the draft holds only the album's credit");
    assert_eq!(answers.of(&album_artist), &CreditResolution::New);
}

/// A draft's credit follows each change of its own: a name, then each
/// catalog id, then a picked artist in its place.
#[tokio::test]
async fn a_draft_s_credit_is_answered_again_as_it_changes() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let records = bae_records::test_fixtures::test_records(&db);
    for artist in [
        artist("by-name", "Artist Name", None, None),
        artist("by-musicbrainz", "Artist Two", None, Some("mb-1")),
        artist("by-discogs", "Artist Three", Some("discogs-1"), None),
    ] {
        records.insert_artist(&artist).await.unwrap();
    }
    let (_, hash) = stored_pane_candidate(&db).await;
    let controls = PaneControls::of_pane_candidate(&db);
    for (credit, named) in [
        (credit("Artist Name", None, None), Some("by-name")),
        (credit("Somebody", None, None), None),
        (credit("Somebody", None, Some("mb-1")), Some("by-musicbrainz")),
        (credit("Somebody", Some("discogs-1"), None), Some("by-discogs")),
    ] {
        controls.set_album_artists(&hash, &credited(std::slice::from_ref(&credit))).await.unwrap();
        let answers = stored_answers(&db).await;
        assert_eq!(answers.len(), 1, "only the draft's one credit is answered");
        match named {
            Some(id) => assert!(names(answers.of(&credit), id), "{credit:?}"),
            None => assert_eq!(answers.of(&credit), &CreditResolution::New),
        }
    }
    let picked = ExistingArtist {
        artist_id: "by-name".to_string(),
        name: "Artist Name".to_string(),
        sort_name: None,
        musicbrainz_artist_id: None,
        discogs_artist_id: None,
    };
    controls
        .set_album_artists(&hash, &[Credited::plain(ArtistAssignment::picked(picked))])
        .await
        .unwrap();
    assert!(stored_answers(&db).await.is_empty(), "a picked artist has no answer");
}

/// The library's own writes move the answers they reach: an import's new
/// artist, a printed name added and removed, catalog ids an artist gains,
/// and an artist whose own id a catalog id derives.
#[tokio::test]
async fn an_artist_write_answers_the_credits_it_reaches_again() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let records = bae_records::test_fixtures::test_records(&db);
    records.insert_artist(&artist("named", "Artist Name", None, None)).await.unwrap();
    let derived = bae_records::identity::artist_id(Some("mb-derived"), None).unwrap();
    let (_, hash) = stored_pane_candidate(&db).await;
    let controls = PaneControls::of_pane_candidate(&db);
    let album = [
        credit("New Person", None, None),
        credit("Printed Name", None, None),
        credit("Other", None, Some("mb-9")),
        credit("Artist Name", Some("discogs-9"), None),
        credit("Whoever", None, Some("mb-derived")),
    ];
    controls.set_album_artists(&hash, &credited(&album)).await.unwrap();
    let answers = stored_answers(&db).await;
    for (credit, named) in album.iter().zip([None, None, None, Some("named"), None]) {
        match named {
            Some(id) => assert!(names(answers.of(credit), id), "{credit:?}"),
            None => assert_eq!(answers.of(credit), &CreditResolution::New, "{credit:?}"),
        }
    }

    let met = records.find_or_create_artists(&[crediting("New Person", None, None)]).await.unwrap();
    assert!(names(&answer(&db, "New Person").await, &met[0]));

    records
        .add_artist_credit_name("named", &bae_store::EnteredName::parse("Printed Name").unwrap())
        .await
        .unwrap();
    assert!(names(&answer(&db, "Printed Name").await, "named"));
    records
        .remove_artist_credit_name("named", &bae_types::text_match::artist_name_key("Printed Name"))
        .await
        .unwrap();
    assert_eq!(answer(&db, "Printed Name").await, CreditResolution::New);

    records
        .find_or_create_artists(&[crediting("Artist Name", Some("discogs-9"), Some("mb-9"))])
        .await
        .unwrap();
    let answers = stored_answers(&db).await;
    assert!(names(answers.of(&album[2]), "named"), "the artist gained the MusicBrainz id");
    let CreditResolution::Library { artist: named } = answers.of(&album[3]) else {
        panic!("the credit names the artist");
    };
    assert_eq!(named.discogs_artist_id.as_deref(), Some("discogs-9"));

    records.insert_artist(&artist(&derived, "Derived Name", None, None)).await.unwrap();
    assert!(names(stored_answers(&db).await.of(&album[4]), &derived));
}

/// An artist holding one catalog's Various Artists id answers a credit
/// naming the other catalog's alone.
#[tokio::test]
async fn various_artists_by_one_catalog_follows_an_artist_holding_the_other() {
    let va = bae_records::VARIOUS_ARTISTS;
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let (_, hash) = stored_pane_candidate(&db).await;
    PaneControls::of_pane_candidate(&db)
        .set_album_artists(&hash, &credited(&[credit("Various", Some(va.discogs), None)]))
        .await
        .unwrap();
    assert_eq!(
        stored_answers(&db).await.of(&credit("Various", Some(va.discogs), None)),
        &CreditResolution::New
    );
    bae_records::test_fixtures::test_records(&db)
        .insert_artist(&artist("va", "Various Artists", None, Some(va.musicbrainz)))
        .await
        .unwrap();
    assert!(names(stored_answers(&db).await.of(&credit("Various", Some(va.discogs), None)), "va"));
}

/// Deleting the album that last credits an artist deletes the artist, and
/// the credit that named it names nobody.
#[tokio::test]
async fn a_deleted_artist_leaves_the_credits_naming_it() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    bae_records::test_fixtures::seed_artist_album_release(&db).await;
    let (_, hash) = stored_pane_candidate(&db).await;
    PaneControls::of_pane_candidate(&db)
        .set_album_artists(&hash, &credited(&[credit("Artist Name Primary", None, None)]))
        .await
        .unwrap();
    assert!(names(
        &answer(&db, "Artist Name Primary").await,
        bae_records::test_fixtures::ARTIST_PRIMARY
    ));
    let records = bae_records::test_fixtures::test_records(&db);
    let release = records
        .plan_release_deletion(bae_records::test_fixtures::RELEASE_A)
        .await
        .unwrap()
        .expect("the library holds the release");
    records
        .delete_album_with_cleanup(bae_records::test_fixtures::ALBUM_A, vec![release])
        .await
        .unwrap();
    assert_eq!(answer(&db, "Artist Name Primary").await, CreditResolution::New);
}

/// Another device's import of an artist answers this device's draft credit
/// in the transaction that applies it, with no write of this device's.
#[test]
fn another_device_s_artist_answers_this_device_s_credit() {
    bae_store::test_devices::run_two_store_test(crate::TEST_SCHEMA, async |devices| {
        let (_, hash) = stored_pane_candidate(devices.a()).await;
        PaneControls::of_pane_candidate(devices.a())
            .set_album_artists(&hash, &credited(&[credit("Artist Name", None, None)]))
            .await
            .unwrap();
        assert_eq!(answer(devices.a(), "Artist Name").await, CreditResolution::New);

        let met = bae_records::test_fixtures::test_records(devices.b())
            .find_or_create_artists(&[crediting("Artist Name", None, None)])
            .await
            .unwrap();
        let met = met[0].clone();
        devices
            .settle("the draft's credit names the other device's artist", async |device, db| {
                device != "A" || names(&answer(db, "Artist Name").await, &met)
            })
            .await;
    });
}

#[tokio::test]
async fn a_catalog_id_wins_over_the_name_and_a_name_with_another_id_is_new() {
    let answers = answers_after(
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
        matches!(&answers[0], CreditResolution::Library { artist } if artist.artist_id == "catalogued")
    );
    assert_eq!(&answers[1], &CreditResolution::New);
}

#[tokio::test]
async fn ids_that_name_two_artists_leave_the_musicbrainz_one() {
    let answers = answers_after(
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
        matches!(&answers[0], CreditResolution::Library { artist } if artist.artist_id == "by-musicbrainz")
    );
}

#[tokio::test]
async fn a_musicbrainz_artist_holding_another_discogs_id_is_the_credit_and_a_discogs_artist_holding_another_musicbrainz_id_is_not(
) {
    let answers = answers_after(
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
        matches!(&answers[0], CreditResolution::Library { artist } if artist.artist_id == "mb-artist")
    );
    assert_eq!(&answers[1], &CreditResolution::New);
}

#[tokio::test]
async fn an_id_held_on_its_own_row_and_on_another_names_the_smaller_one() {
    let identity = bae_records::identity::artist_id(Some("mb-derived"), None).unwrap();
    let answers = answers_after(
        vec![
            artist("zzz-other", "Artist Other", None, Some("mb-derived")),
            artist(&identity, "Artist Named By Id", None, None),
        ],
        vec![],
        vec![credit("Whoever", None, Some("mb-derived"))],
        vec![],
    )
    .await;
    assert!(matches!(&answers[0], CreditResolution::Library { .. }));
}

#[tokio::test]
async fn various_artists_is_one_artist_in_both_catalogs() {
    let va = bae_records::VARIOUS_ARTISTS;
    let answers = answers_after(
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
    assert!(matches!(&answers[1], CreditResolution::Library { .. }));
}

#[tokio::test]
async fn various_artists_by_discogs_alone_places_nobody_when_no_artist_holds_either_id() {
    let va = bae_records::VARIOUS_ARTISTS;
    let answers = answers_after(
        vec![artist("va-named", "Various", None, None)],
        vec![],
        vec![credit("Various", Some(va.discogs), None)],
        vec![],
    )
    .await;
    assert!(matches!(&answers[0], CreditResolution::Library { .. }));
}
