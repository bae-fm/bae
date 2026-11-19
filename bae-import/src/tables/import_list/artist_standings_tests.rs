//! Each field's standing follows its artists': a picked artist stands in the
//! library and a credit as its stored answer says, and the field counts them.

use super::*;
use crate::identification::tests::fixed_now;
use crate::tables::import_state::tests::*;
use crate::{ArtistAssignment, Credited, RawTrackEdit, TrackArtistAssignments};
use bae_records::{DbArtist, ExistingArtist};
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
