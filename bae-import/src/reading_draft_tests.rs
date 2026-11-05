//! What a reading's values commit as: which credits one import makes one
//! library artist, and the work rows its tracks perform.

use super::*;
use bae_catalogs::catalog::Catalog;
use bae_catalogs::credits::{ArtistCredit, CreditedRole, SourceCredit};
use bae_catalogs::release_values::TrackValues;
use bae_catalogs::test_releases::{mb_credit, mb_release_values, mb_vinyl_release};
use bae_catalogs::works::{GraphWork, WorkGraph, WorkPart};

/// A credit to `name`, with the ids given.
fn artist(name: &str, musicbrainz: Option<&str>, discogs: Option<&str>) -> ArtistCredit {
    ArtistCredit {
        musicbrainz_artist_id: musicbrainz.map(str::to_string),
        discogs_artist_id: discogs.map(str::to_string),
        ..ArtistCredit::named(name)
    }
}

/// `artist` credited by `source` in `role`.
fn role(artist: ArtistCredit, source: Catalog, role: &str) -> CreditedRole {
    CreditedRole { artist, source, role: Some(role.to_string()) }
}

/// Track `number` of one CD, credited as `credits`, in `roles`, performing
/// `works`.
fn track(
    number: u32,
    credits: TrackCredits,
    roles: Vec<CreditedRole>,
    works: &[&str],
) -> TrackValues {
    TrackValues {
        title: format!("Track Title {number}"),
        place: bae_types::mediums::TrackPlace {
            medium: std::num::NonZeroU16::MIN,
            side: std::num::NonZeroU8::MIN,
            number: std::num::NonZeroU32::new(number).expect("tracks count from one"),
        },
        stated_length_ms: None,
        credits,
        roles,
        works: works.iter().map(|work| work.to_string()).collect(),
    }
}

/// A reading of one CD credited to `album`, with `tracks`.
fn reading(album: ArtistCredit, tracks: Vec<TrackValues>) -> ReleaseValues {
    ReleaseValues {
        album_title: "Album Title".to_string(),
        album_credits: vec![SourceCredit::plain(album)],
        album_year: None,
        pressing: bae_types::pressing::Pressing::blank(),
        mediums: vec![bae_types::mediums::ReleaseMedium::one_sided(Some(
            bae_types::pressing::Medium::Cd,
        ))],
        release_roles: Vec::new(),
        works: WorkGraph::default(),
        tracks,
    }
}

/// The library artist each of `rows`' artist rows commits as, by row id.
fn library_artists(rows: &bae_records::ReleaseRows) -> std::collections::HashMap<String, String> {
    committed_artists(rows)
}

/// The library artist the album's first credit commits as.
fn album_artist(rows: &bae_records::ReleaseRows) -> String {
    library_artists(rows)[&rows.album_artists[0].artist_id].clone()
}

/// A Discogs artist credited in a role on the release and in another on a
/// track commits as one artist, apart from the album artist, and as no
/// album or track artist.
#[test]
fn one_artist_credited_in_roles_on_the_release_and_a_track_commits_once() {
    let composer = artist("Composer Credit A", None, Some("discogs-composer-a"));
    let mut values = reading(
        artist("Artist Name A", None, Some("artist-1")),
        vec![track(
            1,
            TrackCredits::AlbumArtists,
            vec![role(composer.clone(), Catalog::Discogs, "Written-By")],
            &[],
        )],
    );
    values.release_roles = vec![role(composer, Catalog::Discogs, "Composed By")];

    let rows = committed_rows(values);

    let committed = library_artists(&rows);
    let release_role = &rows.release_artist_roles[0];
    let track_role = &rows.track_artist_roles[0];
    assert_eq!(release_role.source_credit.as_deref(), Some("Composed By"));
    assert_eq!(track_role.source_credit.as_deref(), Some("Written-By"));
    let composer = committed[&release_role.artist_id].clone();
    assert_eq!(committed[&track_role.artist_id], composer);
    assert_ne!(album_artist(&rows), composer);
    assert!(!rows.album_artists.iter().any(|link| committed[&link.artist_id] == composer));
    assert!(!rows.track_artists.iter().any(|link| committed[&link.artist_id] == composer));
}

/// An id-less role credit whose name matches the album artist's commits as
/// that artist rather than a duplicate, however it is cased, accented or
/// spaced: a credit with no id is matched by its name, whether or not the
/// artist it names bears a source id.
#[test]
fn an_id_less_role_credit_commits_as_the_album_artist_of_its_name() {
    for printed in ["ARTIST NAME A", "Ärtist  Name A"] {
        let mut values = reading(
            artist("Artist Name A", None, Some("artist-1")),
            vec![track(1, TrackCredits::AlbumArtists, Vec::new(), &[])],
        );
        values.release_roles =
            vec![role(artist(printed, None, None), Catalog::Discogs, "Composed By")];

        let rows = committed_rows(values);

        assert_eq!(
            library_artists(&rows)[&rows.release_artist_roles[0].artist_id],
            album_artist(&rows),
            "{printed:?} commits as the album artist, not a duplicate"
        );
    }
}

/// Two id-less role credits of one name commit as one artist of that name.
#[test]
fn id_less_role_credits_of_one_name_commit_as_one_artist() {
    let shared = artist("Shared Credited Name", None, None);
    let mut values = reading(
        artist("Artist Name A", None, Some("artist-1")),
        vec![track(1, TrackCredits::AlbumArtists, Vec::new(), &[])],
    );
    values.release_roles = vec![
        role(shared.clone(), Catalog::Discogs, "Composed By"),
        role(shared, Catalog::Discogs, "Written-By"),
    ];

    let rows = committed_rows(values);

    let committed = library_artists(&rows);
    let roles: Vec<_> = rows
        .release_artist_roles
        .iter()
        .map(|role| {
            let row = rows
                .artists
                .iter()
                .find(|artist| artist.id == role.artist_id)
                .expect("a role names its artist row");
            assert_eq!(row.name, "Shared Credited Name");
            committed[&row.id].clone()
        })
        .collect();
    assert_eq!(roles.len(), 2);
    assert_eq!(roles[0], roles[1], "both credits commit as one artist");
}

/// A track credit naming the album artist's MusicBrainz id commits as the
/// album artist rather than a duplicate.
#[test]
fn a_track_credit_by_the_album_artist_s_id_commits_as_the_album_artist() {
    let values = reading(
        artist("Artist Name A", Some("artist-1"), None),
        vec![track(
            1,
            TrackCredits::Own(vec![SourceCredit::plain(artist(
                "Test Artist",
                Some("artist-1"),
                None,
            ))]),
            Vec::new(),
            &[],
        )],
    );

    let rows = committed_rows(values);

    let committed = library_artists(&rows);
    assert!(
        rows.track_artists.iter().any(|link| link.track_id == rows.tracks[0].id
            && committed[&link.artist_id] == album_artist(&rows)),
        "the album artist is not duplicated by a track credit"
    );
}

/// A credit with a MusicBrainz id never merges into a same-name artist that
/// has none, whichever side holds the id: an id-less album artist stays apart
/// from a composer of its name with one, and an id-less track credit stays
/// apart from an album artist of its name with one.
#[test]
fn a_known_musicbrainz_id_keeps_a_same_name_artist_without_one_apart() {
    let values = reading(
        artist("Artist Name A", None, None),
        vec![track(
            1,
            TrackCredits::AlbumArtists,
            vec![role(
                artist("Artist Name A", Some("composer-artist-a"), None),
                Catalog::MusicBrainz,
                "composer",
            )],
            &[],
        )],
    );
    let rows = committed_rows(values);
    let composer = rows
        .artists
        .iter()
        .find(|artist| artist.musicbrainz_artist_id.as_deref() == Some("composer-artist-a"))
        .expect("the composer's row");
    let album_row = rows
        .artists
        .iter()
        .find(|artist| artist.musicbrainz_artist_id.is_none())
        .expect("the album artist's row, with no id");
    assert_ne!(album_row.id, composer.id, "the composer is a row of its own");
    assert!(rows.track_artist_roles.iter().any(|role| role.artist_id == composer.id));

    let values = reading(
        artist("Artist Name A", Some("artist-1"), None),
        vec![track(
            1,
            TrackCredits::Own(vec![SourceCredit::plain(artist("Artist Name A", None, None))]),
            Vec::new(),
            &[],
        )],
    );
    let rows = committed_rows(values);
    let named: Vec<_> =
        rows.artists.iter().filter(|artist| artist.name == "Artist Name A").collect();
    assert_eq!(named.len(), 2, "an id-less credit does not merge into the id-bearing artist");
    assert!(named.iter().any(|artist| artist.musicbrainz_artist_id.as_deref() == Some("artist-1")));
    assert!(named.iter().any(|artist| artist.musicbrainz_artist_id.is_none()));
}

/// Two MusicBrainz ids of one name commit as two artists.
#[test]
fn two_musicbrainz_ids_of_one_name_commit_as_two_artists() {
    let values = reading(
        artist("Artist Name B", Some("artist-b"), None),
        vec![track(
            1,
            TrackCredits::Own(vec![SourceCredit::plain(artist(
                "Artist Name A",
                Some("artist-1"),
                None,
            ))]),
            vec![role(
                artist("Artist Name A", Some("artist-2"), None),
                Catalog::MusicBrainz,
                "composer",
            )],
            &[],
        )],
    );

    let rows = committed_rows(values);

    let committed = library_artists(&rows);
    let named: std::collections::HashSet<_> = rows
        .artists
        .iter()
        .filter(|artist| artist.name == "Artist Name A")
        .map(|artist| &committed[&artist.id])
        .collect();
    assert_eq!(named.len(), 2, "two MusicBrainz ids commit as two artists");
}

/// A track crediting its own artist links that artist to it alone; a track
/// crediting the album's artists gets no track-artist rows.
#[test]
fn a_track_s_own_credit_links_to_that_track_only() {
    let values = reading(
        artist("Artist Name A", Some("artist-1"), None),
        vec![
            track(1, TrackCredits::AlbumArtists, Vec::new(), &[]),
            track(
                2,
                TrackCredits::Own(vec![SourceCredit::plain(artist(
                    "Guest",
                    Some("artist-guest"),
                    None,
                ))]),
                Vec::new(),
                &[],
            ),
        ],
    );

    let rows = committed_rows(values);

    let guest = rows
        .artists
        .iter()
        .find(|artist| artist.musicbrainz_artist_id.as_deref() == Some("artist-guest"))
        .expect("the guest's row");
    assert!(rows
        .track_artists
        .iter()
        .any(|link| link.track_id == rows.tracks[1].id && link.artist_id == guest.id));
    assert!(!rows.track_artists.iter().any(|link| link.track_id == rows.tracks[0].id));
}

/// A work and its part, each composed by one artist, commit as two work rows
/// linked parent to part, the track that performs the work linked to it, and
/// the composer one artist credited on each work — no album or track artist.
#[test]
fn one_composer_of_a_work_and_its_part_commits_as_one_artist() {
    let composer = artist("Composer Name A", Some("composer-artist-a"), None);
    let work = |id: &str, title: &str| GraphWork {
        musicbrainz_work_id: id.to_string(),
        title: title.to_string(),
        disambiguation: None,
        work_type: None,
        composers: vec![composer.clone()],
    };
    let mut values = reading(
        artist("Artist Name A", Some("artist-1"), None),
        vec![
            track(1, TrackCredits::AlbumArtists, Vec::new(), &["mb-work-parent-a"]),
            track(2, TrackCredits::AlbumArtists, Vec::new(), &["mb-work-parent-a"]),
        ],
    );
    values.works = WorkGraph {
        works: vec![
            work("mb-work-parent-a", "Work Title A"),
            work("mb-work-child-a", "Work Part A"),
        ],
        parts: vec![WorkPart {
            parent: "mb-work-parent-a".to_string(),
            child: "mb-work-child-a".to_string(),
        }],
    };

    let rows = committed_rows(values);

    let graph = &rows.work_graph;
    assert_eq!(graph.works.len(), 2);
    let row_of = |id: &str| {
        graph
            .works
            .iter()
            .find(|work| work.musicbrainz_work_id == id)
            .unwrap_or_else(|| panic!("no work row for {id}"))
            .id
            .clone()
    };
    let (parent, child) = (row_of("mb-work-parent-a"), row_of("mb-work-child-a"));
    assert!(graph
        .track_works
        .iter()
        .any(|link| link.track_id == rows.tracks[0].id && link.work_id == parent));
    assert!(graph
        .work_parts
        .iter()
        .any(|part| part.parent_work_id == parent && part.child_work_id == child));
    let committed = library_artists(&rows);
    let composer = committed[&graph.work_artists[0].artist_id].clone();
    assert_eq!(
        graph.work_artists.iter().filter(|link| committed[&link.artist_id] == composer).count(),
        2
    );
    assert!(!rows.album_artists.iter().any(|link| committed[&link.artist_id] == composer));
    assert!(!rows.track_artists.iter().any(|link| committed[&link.artist_id] == composer));
}

/// The editor seed for a release: its reading's editable values.
fn seed_for(response: &bae_catalogs::musicbrainz::MbReleaseResponse) -> crate::ReleaseUserEdit {
    user_edit(&mb_release_values(response))
}

/// Every artist the release credits reaches the editor, in credit order. The
/// picker's display shape collapses them to one name; the seed must not, or the
/// commit's artist comparison reads an untouched list as an edit and drops the
/// junction rows.
#[test]
fn seed_carries_every_album_artist() {
    let mut response = mb_vinyl_release("vinyl-release");
    response.artist_credit =
        vec![mb_credit("mb-artist-a", "Artist A"), mb_credit("mb-artist-b", "Artist B")];

    let edit = seed_for(&response);

    let names: Vec<&str> = edit
        .album_artist_assignments
        .iter()
        .map(|assignment| match assignment {
            crate::Credited { artist: crate::ArtistAssignment::Credit { credit }, .. } => {
                credit.name.as_str()
            }
            crate::Credited { artist: crate::ArtistAssignment::Picked { .. }, .. } => {
                panic!("a parsed source artist is not a library selection")
            }
        })
        .collect();
    assert_eq!(names, vec!["Artist A", "Artist B"]);
}

/// A track crediting a guest seeds that name; a track with no credit of its own
/// seeds an empty list — the editor's "share the album artist" convention.
#[test]
fn seed_per_track_artist_override() {
    let mut response = mb_vinyl_release("vinyl-release");
    response.media[0].tracks[2].artist_credit = vec![mb_credit("mb-guest", "Guest Artist")];

    let edit = seed_for(&response);

    assert_eq!(edit.tracks[0].artist_assignments, crate::TrackArtistAssignments::AlbumArtists);
    assert_eq!(edit.tracks[1].artist_assignments, crate::TrackArtistAssignments::AlbumArtists);
    let crate::TrackArtistAssignments::Explicit(assignments) = &edit.tracks[2].artist_assignments
    else {
        panic!("the guest credit is explicit")
    };
    assert!(matches!(
        assignments.as_slice(),
        [crate::Credited { artist: crate::ArtistAssignment::Credit { credit }, .. }]
            if credit.name == "Guest Artist"
                && credit.musicbrainz_artist_id.as_deref() == Some("mb-guest")
    ));
}
