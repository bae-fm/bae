use super::*;
use crate::import::{
    ArtistCredit, CandidateTrack, CreditedRole, ExistingArtist, GraphWork, RawPressingEdit,
    RawTrackEdit, WorkGraph, WorkPart,
};

fn clock() -> coven::FixedClock {
    coven::FixedClock::new(
        chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc)
            .into(),
    )
}

fn credit(name: &str) -> ArtistCredit {
    ArtistCredit {
        credited_as: None,
        name: name.to_string(),
        sort_name: None,
        musicbrainz_artist_id: None,
        discogs_artist_id: None,
    }
}

fn place(medium: u16, number: u32) -> crate::mediums::TrackPlace {
    crate::mediums::TrackPlace {
        medium: std::num::NonZeroU16::new(medium).expect("mediums count from one"),
        side: std::num::NonZeroU8::MIN,
        number: std::num::NonZeroU32::new(number).expect("numbers count from one"),
    }
}

/// Track `position + 1` of the draft's first disc.
fn track(position: usize, title: &str) -> CandidateTrack {
    CandidateTrack::unsourced(
        RawTrackEdit {
            id: format!("candidate-track-{position}"),
            title: title.to_string(),
            artist_assignments: TrackArtistAssignments::AlbumArtists,
            file: AudioFile::Standalone {
                file_id: format!("{position:02}.flac"),
            },
        },
        place(1, u32::try_from(position + 1).expect("a test position fits u32")),
    )
}

fn draft() -> CandidateDraft {
    CandidateDraft {
        album_title: " Album Title ".to_string(),
        album_artist_assignments: vec![crate::import::Credited::plain(ArtistAssignment::named("Artist Name"))],
        album_year: "1971".to_string(),
        pressing: RawPressingEdit {
            year: "2005".to_string(),
            labels: Vec::new(),
            barcode: String::new(),
            facts: Default::default(),
        },
        mediums: vec![crate::mediums::ReleaseMedium::one_sided(Some(
            crate::pressing::Medium::Cd,
        ))],
        tracks: vec![track(0, "First Track"), track(1, "Second Track")],
        release_roles: Vec::new(),
        works: WorkGraph::default(),
    }
}

fn assemble(draft: &CandidateDraft) -> AssembledRelease {
    assemble_release(draft, None, &clock(), &coven::SequentialIds::new()).unwrap()
}

/// The rows are the draft's values as the form shapes them, each track bound
/// to its own audio.
#[test]
fn the_rows_are_the_drafts_values() {
    let assembled = assemble(&draft());
    let parsed = &assembled.parsed;
    assert_eq!(parsed.album.title, "Album Title");
    assert_eq!(parsed.album.year, Some(1971));
    assert_eq!(parsed.release.pressing.year, Some(2005));
    assert!(!parsed.release.draft_from_tags);
    assert_eq!(
        parsed
            .tracks
            .iter()
            .map(|track| (track.title.as_str(), track.track_number.get()))
            .collect::<Vec<_>>(),
        [("First Track", 1), ("Second Track", 2)]
    );
    assert_eq!(
        assembled.bindings,
        [
            AudioFile::Standalone {
                file_id: "00.flac".into()
            },
            AudioFile::Standalone {
                file_id: "01.flac".into()
            },
        ]
    );
    let album_artist = parsed
        .artists
        .iter()
        .find(|artist| artist.id == parsed.album_artists[0].artist_id)
        .expect("the album credits its artist");
    assert_eq!(album_artist.name, "Artist Name");
}

/// A draft read from the files' tags says so on the release.
#[test]
fn a_draft_read_from_the_tags_marks_its_release() {
    let assembled = assemble_release(
        &draft(),
        Some(&MetadataProvenance::FileMetadata),
        &clock(),
        &coven::SequentialIds::new(),
    )
    .unwrap();
    assert!(assembled.parsed.release.draft_from_tags);
}

/// A picked library artist is linked as itself, once however often the
/// draft credits it.
#[test]
fn a_picked_artist_is_one_row_linked_as_itself() {
    let picked = ArtistAssignment::picked(ExistingArtist {
        artist_id: "library-artist".to_string(),
        name: "Library Artist".to_string(),
        sort_name: None,
        musicbrainz_artist_id: None,
        discogs_artist_id: None,
    });
    let mut draft = draft();
    draft.album_artist_assignments = vec![crate::import::Credited::plain(picked.clone())];
    draft.tracks[0].edit.artist_assignments = TrackArtistAssignments::Explicit(vec![crate::import::Credited::plain(picked)]);
    let assembled = assemble(&draft);
    assert_eq!(assembled.picked, HashSet::from(["library-artist".to_string()]));
    assert_eq!(
        assembled
            .parsed
            .artists
            .iter()
            .filter(|artist| artist.id == "library-artist")
            .count(),
        1
    );
    assert_eq!(assembled.parsed.album_artists[0].artist_id, "library-artist");
    assert_eq!(assembled.parsed.track_artists[0].artist_id, "library-artist");
}

/// The album is a compilation when its one credit is the various-artists
/// artist, however the draft came to credit it, and not once another artist
/// stands beside it.
#[test]
fn an_album_credited_to_various_artists_alone_is_a_compilation() {
    let mut draft = draft();
    draft.album_artist_assignments = vec![crate::import::Credited::plain(ArtistAssignment::named("Various Artists"))];
    assert!(assemble(&draft).parsed.album.is_compilation);

    draft
        .album_artist_assignments
        .push(crate::import::Credited::plain(ArtistAssignment::named("Artist Name")));
    assert!(!assemble(&draft).parsed.album.is_compilation);

    draft.album_artist_assignments = vec![crate::import::Credited::plain(ArtistAssignment::named("Artist Name"))];
    assert!(!assemble(&draft).parsed.album.is_compilation);
}

/// Roles, works and the release's own roles are written from the draft's
/// values: each work's composers numbered in its order, each part in its
/// parent's, and each track's works in the order it performs them.
#[test]
fn roles_and_works_are_written_from_the_draft() {
    let mut draft = draft();
    draft.release_roles = vec![CreditedRole {
        artist: credit("Release Composer"),
        source: Catalog::Discogs,
        role: Some("Written-By".to_string()),
    }];
    draft.tracks[0].roles = vec![CreditedRole {
        artist: credit("Track Composer"),
        source: Catalog::MusicBrainz,
        role: Some("composer".to_string()),
    }];
    draft.tracks[0].works = vec!["mb-movement-1".to_string()];
    draft.tracks[1].works = vec!["mb-movement-2".to_string()];
    let work = |id: &str, composers: Vec<ArtistCredit>| GraphWork {
        musicbrainz_work_id: id.to_string(),
        title: id.to_string(),
        disambiguation: None,
        work_type: None,
        composers,
    };
    draft.works = WorkGraph {
        works: vec![
            work("mb-movement-1", vec![credit("Composer A"), credit("Composer B")]),
            work("mb-suite", Vec::new()),
            work("mb-movement-2", vec![credit("Composer A")]),
        ],
        parts: vec![
            WorkPart {
                parent: "mb-suite".to_string(),
                child: "mb-movement-1".to_string(),
            },
            WorkPart {
                parent: "mb-suite".to_string(),
                child: "mb-movement-2".to_string(),
            },
        ],
    };

    let parsed = assemble(&draft).parsed;
    let name_of = |id: &str| {
        parsed
            .artists
            .iter()
            .find(|artist| artist.id == id)
            .map(|artist| artist.name.as_str())
            .expect("a linked artist has a row")
    };
    assert_eq!(parsed.release_artist_roles.len(), 1);
    assert_eq!(
        name_of(&parsed.release_artist_roles[0].artist_id),
        "Release Composer"
    );
    assert_eq!(parsed.track_artist_roles.len(), 1);
    assert_eq!(parsed.track_artist_roles[0].track_id, parsed.tracks[0].id);
    assert_eq!(
        name_of(&parsed.track_artist_roles[0].artist_id),
        "Track Composer"
    );

    let graph = &parsed.work_graph;
    let work_id = |id: &str| crate::db::identity::work_id(id);
    assert_eq!(graph.works.len(), 3);
    assert_eq!(
        graph
            .work_artists
            .iter()
            .map(|link| (link.work_id.clone(), name_of(&link.artist_id), link.position))
            .collect::<Vec<_>>(),
        [
            (work_id("mb-movement-1"), "Composer A", 0),
            (work_id("mb-movement-1"), "Composer B", 1),
            (work_id("mb-movement-2"), "Composer A", 0),
        ]
    );
    assert_eq!(
        graph
            .work_parts
            .iter()
            .map(|part| (part.child_work_id.clone(), part.position))
            .collect::<Vec<_>>(),
        [(work_id("mb-movement-1"), 0), (work_id("mb-movement-2"), 1)]
    );
    assert_eq!(
        graph
            .track_works
            .iter()
            .map(|link| (link.track_id.clone(), link.work_id.clone()))
            .collect::<Vec<_>>(),
        [
            (parsed.tracks[0].id.clone(), work_id("mb-movement-1")),
            (parsed.tracks[1].id.clone(), work_id("mb-movement-2")),
        ]
    );
}

/// A track that performs a work the draft does not hold is a draft that
/// contradicts itself, and is refused.
#[test]
fn a_track_performing_a_work_the_draft_lacks_is_refused() {
    let mut draft = draft();
    draft.tracks[0].works = vec!["mb-missing".to_string()];
    assert!(matches!(
        assemble_release(&draft, None, &clock(), &coven::SequentialIds::new()),
        Err(ImportError::Internal { .. })
    ));
}

/// Dropping a track keeps only the works the remaining tracks perform and
/// the works those nest among.
#[test]
fn a_draft_keeps_only_the_works_its_tracks_perform() {
    let work = |id: &str| GraphWork {
        musicbrainz_work_id: id.to_string(),
        title: id.to_string(),
        disambiguation: None,
        work_type: None,
        composers: vec![credit(&format!("Composer of {id}"))],
    };
    let part = |parent: &str, child: &str| WorkPart {
        parent: parent.to_string(),
        child: child.to_string(),
    };
    let mut full = draft();
    full.tracks[0].works = vec!["kept".to_string()];
    full.tracks[1].works = vec!["dropped".to_string()];
    full.works = WorkGraph {
        works: vec![work("kept"), work("kept-parent"), work("dropped"), work("dropped-parent")],
        parts: vec![part("kept-parent", "kept"), part("dropped-parent", "dropped")],
    };
    let kept = full.with_tracks(vec![full.tracks[0].clone()]);
    assert_eq!(
        kept.works
            .works
            .iter()
            .map(|work| work.musicbrainz_work_id.as_str())
            .collect::<Vec<_>>(),
        ["kept", "kept-parent"]
    );
    assert_eq!(kept.works.parts, [part("kept-parent", "kept")]);
}

/// The draft's mediums become the release's medium rows, in order, and each
/// track row names the one its place is on.
#[test]
fn each_track_row_sits_on_the_medium_row_its_place_names() {
    let mut draft = draft();
    draft.mediums.push(crate::mediums::ReleaseMedium::one_sided(Some(
        crate::pressing::Medium::Cd,
    )));
    draft.tracks[1].place = place(2, 1);
    let parsed = assemble(&draft).parsed;

    assert_eq!(
        parsed
            .mediums
            .iter()
            .map(|medium| (medium.position.get(), medium.release_id.as_str()))
            .collect::<Vec<_>>(),
        [(1, parsed.release.id.as_str()), (2, parsed.release.id.as_str())]
    );
    assert_eq!(parsed.tracks[0].medium_id, parsed.mediums[0].id);
    assert_eq!(parsed.tracks[1].medium_id, parsed.mediums[1].id);
    assert_eq!(
        crate::import::release_values::row_labels(&parsed),
        ["1-1", "2-1"]
    );
}
