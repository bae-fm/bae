//! Each field holds its credits, each with its artist's standing: a picked
//! artist stands in the library and a credit as its stored answer says, and
//! the field counts them.

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

/// The candidate `hash`'s album artists and its first track's own artists,
/// each with its standing.
async fn fields_of(
    db: &bae_store::Store,
    hash: &str,
) -> (Option<ArtistField>, Option<ArtistField>) {
    let hash = hash.to_string();
    db.read(move |sql| {
        let source =
            CreditSource::Candidate(crate::tables::import_state::OneCandidate::Hash(&hash));
        let album = album_artist_field_on(&sql, source)?;
        let track: Option<String> = sql
            .query_row(
                &format!(
                    "WITH {} SELECT field FROM artist_field WHERE part = 1",
                    artist_field_clauses(source)
                ),
                source.params().as_slice(),
                |row| row.get(0),
            )
            .optional()?;
        Ok((album, track.as_deref().map(artist_field_of_json).transpose()?))
    })
    .await
    .unwrap()
}

/// The field `standing` of `credits`, each beside its artist's standing.
fn field_of(standing: ArtistsStanding, credits: Vec<(Credited, ArtistStanding)>) -> ArtistField {
    ArtistField {
        standing,
        credits: credits
            .into_iter()
            .map(|(credited, standing)| StandingCredit { credited, standing })
            .collect(),
    }
}

/// A picked artist stands in the library; each credit stands as it
/// resolved, beside its credit in the field's order; and the field counts its
/// artists: all in the library, all new, some new, one with choices, or some
/// to choose.
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
            sort_name: Some("Library Name, sorted".to_string()),
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

    use ArtistStanding::{Library, New};
    for (standing, credits) in [
        (ArtistsStanding::Library, vec![(picked(), Library), (named("library name"), Library)]),
        (ArtistsStanding::New, vec![(named("New One"), New), (named("New Two"), New)]),
        (ArtistsStanding::SomeNew { count: 1 }, vec![(picked(), Library), (named("New One"), New)]),
        (ArtistsStanding::Choose { choices: 2 }, vec![(named("Shared Name"), shared())]),
        (
            ArtistsStanding::SomeToChoose { count: 1 },
            vec![(picked(), Library), (named("Shared Name"), shared()), (named("New One"), New)],
        ),
    ] {
        let album: Vec<Credited> = credits.iter().map(|(credited, _)| credited.clone()).collect();
        controls.set_album_artists(&hash, &album).await.unwrap();
        let (album_field, track_field) = fields_of(&db, &hash).await;
        assert_eq!(album_field, Some(field_of(standing, credits)));
        assert_eq!(track_field, None, "the track takes its album's artists");
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

    let (album, track) = fields_of(&db, &hash).await;
    let named = |name: &str| Credited::plain(ArtistAssignment::named(name));
    assert_eq!(
        album,
        Some(field_of(ArtistsStanding::New, vec![(named("Album Artist"), ArtistStanding::New)]))
    );
    assert_eq!(
        track,
        Some(field_of(
            ArtistsStanding::New,
            vec![
                (named("Guest One"), ArtistStanding::New),
                (named("Guest Two"), ArtistStanding::New)
            ]
        ))
    );
}
