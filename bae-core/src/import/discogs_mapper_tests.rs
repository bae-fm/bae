use super::*;
use crate::import::source_release::SourceRelease;
use crate::discogs::models::{DiscogsArtist, DiscogsRoleArtist, DiscogsTrack};
use crate::import::MetadataRef;

/// A lone Discogs release document as bae keeps it: its own facts and
/// tracklist, no other document behind it.
fn stored(release: &DiscogsRelease) -> SourceRelease {
    SourceRelease {
        release: MetadataRef::new(Catalog::Discogs, &release.id),
        source_group_id: release.master_id.clone(),
        metadata: metadata(release),
        other_records: Vec::new(),
        covers: crate::import::source_release::ReleaseCovers {
            release: Vec::new(),
            album: Vec::new(),
        },
        archive_release: None,
        archive_groups: Vec::new(),
        mediums: mediums(release, &pressing(release).1),
        catalog: CatalogFacts::Discogs {
            media: pressing(release).1,
            release_roles: release_roles(release),
        },
        notes: crate::import::search::discogs_release_notes(release),
        unfetched: Vec::new(),
    }
}

/// Every disc of a release: what a test with no folder to fit reads.
fn all_discs(release: &SourceRelease) -> MediumCoverage {
    MediumCoverage::all(release.mediums.len())
}

/// The rows the release's reading commits as.
fn map_with(
    release: &DiscogsRelease,
    audio_durations_ms: Option<&[u64]>,
) -> Result<crate::import::ReleaseRows, ImportError> {
    let stored = stored(release);
    super::values(&stored, &all_discs(&stored), audio_durations_ms)
        .map(crate::import::release_values::committed_rows)
}

/// Exercise track and credit mapping with the release's own metadata.
fn map(release: &DiscogsRelease) -> Result<crate::import::ReleaseRows, ImportError> {
    map_with(release, None)
}

fn map_for_audio(
    release: &DiscogsRelease,
    audio_durations_ms: &[u64],
) -> Result<crate::import::ReleaseRows, ImportError> {
    map_with(release, Some(audio_durations_ms))
}

use crate::import::release_values::committed_artists;

/// A tracklist's rows as a stored medium holds them.
fn entries(rows: &[DiscogsTrack]) -> Vec<TracklistEntry> {
    tracklist_entries("1", rows)
}

fn make_track(position: &str, title: &str) -> DiscogsTrack {
    DiscogsTrack {
        position: position.to_string(),
        title: title.to_string(),
        duration: None,
        artists: vec![],
        type_: "track".to_string(),
        extraartists: None,
        sub_tracks: vec![],
    }
}

fn make_heading(title: &str) -> DiscogsTrack {
    DiscogsTrack {
        position: String::new(),
        title: title.to_string(),
        duration: None,
        artists: vec![],
        type_: "heading".to_string(),
        extraartists: None,
        sub_tracks: vec![],
    }
}

fn make_release(tracklist: Vec<DiscogsTrack>) -> DiscogsRelease {
    DiscogsRelease {
        id: "test-123".to_string(),
        title: "Album Title A".to_string(),
        year: Some(2024),
        formats: vec![],
        country: None,
        labels: vec![],
        covers: vec![],
        barcode: None,
        artists: vec![DiscogsArtist {
            anv: None,
            join: None,
            name: "Artist Name A".to_string(),
            id: "artist-1".to_string(),
        }],
        tracklist,
        extraartists: Some(vec![]),
        master_id: None,
        companies: vec![],
        identifiers: vec![],
        notes: None,
    }
}

/// With no artists list, the release artist is derived by splitting the
/// title on " - ". A title that yields no artist (no separator, or an empty
/// artist half) leaves the release unattributed and errors.
#[test]
fn release_without_artists_errors_when_title_yields_no_artist() {
    // (title, why it yields no artist)
    for title in ["Album Title Without Artist", " - Album Title"] {
        let mut release = make_release(vec![]);
        release.artists = vec![];
        release.title = title.to_string();

        let err =
            map(&release).expect_err(&format!("expected error for unattributed title {title:?}"));

        assert!(
            matches!(&err, ImportError::SourceData { detail, .. } if detail.contains("has no album artist")),
            "unexpected error message for {title:?}: {err}"
        );
    }
}

/// With no artists list but a "Artist - Album" title, the release artist is
/// taken from the artist half of the split.
#[test]
fn release_without_artists_derives_release_artist_from_title() {
    let mut release = make_release(vec![make_track("1", "Track 1")]);
    release.artists = vec![];
    release.title = "Artist Name A - Album Title".to_string();

    let parsed = map(&release).unwrap();

    assert_eq!(parsed.artists.len(), 1);
    assert_eq!(parsed.artists[0].name, "Artist Name A");
    assert_eq!(parsed.artists[0].discogs_artist_id, None);
}

#[test]
fn extraartist_roles_import_as_role_credits_not_works_or_display_artists() {
    let mut release = make_release(vec![make_track("1", "Track Title 1")]);
    release.extraartists = Some(vec![
        DiscogsRoleArtist {
            id: Some("discogs-composer-a".to_string()),
            name: "Composer Name A".to_string(),
            role: "Composed By".to_string(),
            credited_name: Some("Composer Credit A".to_string()),
        },
        DiscogsRoleArtist {
            id: Some("discogs-producer-a".to_string()),
            name: "Producer Name A".to_string(),
            role: "Producer".to_string(),
            credited_name: Some("Producer Credit A".to_string()),
        },
    ]);
    release.tracklist[0].extraartists = Some(vec![
        DiscogsRoleArtist {
            id: Some("discogs-composer-a".to_string()),
            name: "Composer Name A".to_string(),
            role: "Written-By".to_string(),
            credited_name: Some("Composer Credit A".to_string()),
        },
        DiscogsRoleArtist {
            id: Some("discogs-engineer-a".to_string()),
            name: "Engineer Name A".to_string(),
            role: "Engineer".to_string(),
            credited_name: Some("Engineer Credit A".to_string()),
        },
    ]);

    let parsed = map(&release).unwrap();

    assert!(parsed.work_graph.works.is_empty());
    assert!(parsed.work_graph.work_artists.is_empty());
    assert!(parsed.work_graph.work_parts.is_empty());
    assert!(parsed.work_graph.track_works.is_empty());

    let committed = committed_artists(&parsed);
    let composer = parsed
        .artists
        .iter()
        .find(|artist| artist.discogs_artist_id.as_deref() == Some("discogs-composer-a"))
        .map(|artist| committed[&artist.id].clone())
        .expect("composer role artist imported");
    let release_artist_roles = &parsed.release_artist_roles;
    let track_artist_roles = &parsed.track_artist_roles;
    assert!(release_artist_roles.iter().any(|role| {
        committed[&role.artist_id] == composer && role.source_credit.as_deref() == Some("Composed By")
    }));
    assert!(track_artist_roles.iter().any(|role| {
        committed[&role.artist_id] == composer && role.source_credit.as_deref() == Some("Written-By")
    }));
    assert!(!parsed
        .artists
        .iter()
        .any(|artist| artist.discogs_artist_id.as_deref() == Some("discogs-producer-a")));
    assert!(!parsed
        .artists
        .iter()
        .any(|artist| artist.discogs_artist_id.as_deref() == Some("discogs-engineer-a")));
    assert!(!parsed
        .album_artists
        .iter()
        .any(|link| committed[&link.artist_id] == composer));
    assert!(committed[&parsed.album_artists[0].artist_id] != composer);
    assert!(!parsed
        .track_artists
        .iter()
        .any(|link| committed[&link.artist_id] == composer));
}

#[test]
fn test_cd_multi_disc() {
    let release = make_release(vec![
        make_track("1-1", "Disc 1 Track 1"),
        make_track("1-2", "Disc 1 Track 2"),
        make_track("1-3", "Disc 1 Track 3"),
        make_track("2-1", "Disc 2 Track 1"),
        make_track("2-2", "Disc 2 Track 2"),
    ]);

    let release = with_formats(release, &[("CD", "2")]);
    let parsed = map(&release).unwrap();
    assert_eq!(
        crate::import::release_values::row_labels(&parsed),
        ["1-1", "1-2", "1-3", "2-1", "2-2"],
        "each disc numbers its own tracks"
    );
}

fn with_formats(mut release: DiscogsRelease, formats: &[(&str, &str)]) -> DiscogsRelease {
    release.formats = formats
        .iter()
        .map(|(name, qty)| crate::discogs::DiscogsFormat {
            name: (*name).into(),
            qty: (*qty).into(),
            descriptions: Vec::new(),
            text: None,
        })
        .collect();
    release
}

#[test]
fn test_vinyl_sides() {
    let release = make_release(vec![
        make_track("A1", "Side A Track 1"),
        make_track("A2", "Side A Track 2"),
        make_track("B1", "Side B Track 1"),
        make_track("B2", "Side B Track 2"),
        make_track("C1", "Side C Track 1"),
        make_track("D1", "Side D Track 1"),
    ]);

    let release = with_formats(release, &[("Vinyl", "2")]);
    let parsed = map(&release).unwrap();
    assert_eq!(
        crate::import::release_values::row_labels(&parsed),
        ["A1", "A2", "B1", "B2", "C1", "D1"],
        "the second record's sides letter on from the first's"
    );
}

#[test]
/// 2LP vinyl with headings and sub-tracks: heading collapses sub-tracks
/// into one track, each letter is its own side, track numbers per-side.
fn test_2lp_vinyl_with_headings() {
    let release = make_release(vec![
        make_track("A1", "Track One"),
        make_track("A2", "Track Two"),
        make_heading("Medley"),
        DiscogsTrack {
            position: "B1i".to_string(),
            title: "Part I".to_string(),
            duration: None,
            artists: vec![],
            type_: "track".to_string(),
            extraartists: None,
            sub_tracks: vec![],
        },
        DiscogsTrack {
            position: "B1ii".to_string(),
            title: "Part II".to_string(),
            duration: None,
            artists: vec![],
            type_: "track".to_string(),
            extraartists: None,
            sub_tracks: vec![],
        },
        DiscogsTrack {
            position: "B1iii".to_string(),
            title: "Part III".to_string(),
            duration: None,
            artists: vec![],
            type_: "track".to_string(),
            extraartists: None,
            sub_tracks: vec![],
        },
        make_heading("-"),
        make_track("B2", "Track Three"),
        make_track("C1", "Track Four"),
        make_track("C2", "Track Five"),
        make_track("C3", "Track Six"),
        make_track("D1", "Track Seven"),
        make_track("D2", "Track Eight"),
    ]);

    let release = with_formats(release, &[("Vinyl", "2")]);
    let parsed = map_for_audio(&release, &[1; 9]).unwrap();
    let tracks = &parsed.tracks;

    // 9 tracks: 2 on A, 2 on B (heading collapsed + Track Three), 3 on C, 2 on D
    assert_eq!(
        crate::import::release_values::row_labels(&parsed),
        ["A1", "A2", "B1", "B2", "C1", "C2", "C3", "D1", "D2"]
    );
    assert_eq!(tracks[0].title, "Track One");
    assert_eq!(tracks[1].title, "Track Two");
    assert_eq!(
        tracks[2].title,
        "Medley: Part I \u{2013} Part II \u{2013} Part III"
    );
    assert_eq!(tracks[3].title, "Track Three");
    assert_eq!(tracks[4].title, "Track Four");
    assert_eq!(tracks[8].title, "Track Eight");
}

#[test]
fn test_single_disc() {
    let mut release = make_release(vec![
        make_track("1", "Track 1"),
        make_track("2", "Track 2"),
        make_track("3", "Track 3"),
    ]);

    release.formats = vec![crate::discogs::DiscogsFormat {
        name: "CD".into(),
        qty: "1".into(),
        descriptions: vec!["Album".into()],
        text: None,
    }];
    let parsed = map(&release).unwrap();
    assert_eq!(
        crate::import::release_values::row_labels(&parsed),
        ["1", "2", "3"]
    );
}

#[test]
fn test_collapsed_sub_tracks_preserve_per_track_artists() {
    let sub_artist = DiscogsArtist {
        anv: None,
        join: None,
        name: "Sub Artist".to_string(),
        id: "sub-artist-1".to_string(),
    };

    let release = make_release(vec![
        make_track("A1", "Normal Track"),
        make_heading("Suite"),
        DiscogsTrack {
            position: "B1i".to_string(),
            title: "Movement 1".to_string(),
            duration: None,
            artists: vec![sub_artist.clone()],
            type_: "track".to_string(),
            extraartists: None,
            sub_tracks: vec![],
        },
        DiscogsTrack {
            position: "B1ii".to_string(),
            title: "Movement 2".to_string(),
            duration: None,
            artists: vec![sub_artist.clone()],
            type_: "track".to_string(),
            extraartists: None,
            sub_tracks: vec![],
        },
    ]);

    let parsed = map_for_audio(&release, &[1; 2]).unwrap();
    let tracks = &parsed.tracks;
    let artists = &parsed.artists;
    let track_artists = &parsed.track_artists;

    // 2 tracks: A1 and collapsed B1
    assert_eq!(tracks.len(), 2);
    assert_eq!(tracks[1].title, "Suite: Movement 1 \u{2013} Movement 2");

    // Sub artist should be in the artists list
    assert!(
        artists.iter().any(|a| a.name == "Sub Artist"),
        "Sub artist not found in artists list"
    );

    // The collapsed track should have a track-artist link to "Sub Artist"
    let collapsed_track_id = &tracks[1].id;
    let linked_artists: Vec<_> = track_artists
        .iter()
        .filter(|ta| &ta.track_id == collapsed_track_id)
        .collect();
    assert_eq!(
        linked_artists.len(),
        1,
        "Expected exactly 1 track-artist link for collapsed track (deduped), got {}",
        linked_artists.len()
    );

    let linked_artist_id = &linked_artists[0].artist_id;
    let linked_artist = artists.iter().find(|a| &a.id == linked_artist_id).unwrap();
    assert_eq!(linked_artist.name, "Sub Artist");
}

#[test]
fn process_tracklist_empty_yields_no_tracks() {
    assert!(process_tracklist(&[], None).is_empty());
}

#[test]
fn process_tracklist_drops_headings_with_no_subtracks() {
    // Headings only contribute a track once sub-tracks accumulate under
    // them. Headings with nothing beneath (here, back-to-back) are dropped.
    let tracklist = vec![make_heading("Disc One"), make_heading("Disc Two")];
    assert!(process_tracklist(&entries(&tracklist), None).is_empty());
}

#[test]
fn heading_subtracks_expand_to_matching_audio_rows() {
    let mut first = make_track("1a", "Part One");
    first.duration = Some("1:00".to_string());
    let mut second = make_track("1b", "Part Two");
    second.duration = Some("2:00".to_string());
    let tracklist = vec![make_heading("Suite Title"), first, second];

    let rows = entries(&tracklist);
    let tracks = process_tracklist(&rows, Some(&[60_000, 120_000]));

    assert_eq!(tracks.len(), 2);
    assert_eq!(tracks[0].title, "Suite Title: Part One");
    assert_eq!(tracks[1].title, "Suite Title: Part Two");
}

#[test]
fn process_tracklist_filters_index_entries() {
    // Discogs "index" rows are structural, not playable tracks.
    let index = DiscogsTrack {
        position: String::new(),
        title: "Side A".to_string(),
        duration: None,
        artists: vec![],
        type_: "index".to_string(),
        extraartists: None,
        sub_tracks: vec![],
    };
    let tracklist = [index, make_track("A1", "Real Track")];
    let rows = entries(&tracklist);
    let result = process_tracklist(&rows, None);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].title, "Real Track");
}

#[test]
fn discogs_role_is_composer_matches_composing_roles() {
    // Bracket-wrapped and case/separator variants all normalize before the
    // substring check; non-composing roles never match.
    for role in [
        "Composed By",
        "Written-By",
        "Written By",
        "Music By",
        "Composer",
        "[Composed By]",
        "[Written-By]",
        "composed_by",
    ] {
        assert!(
            discogs_role_is_composer(role),
            "expected {role:?} to be a composer role"
        );
    }
    for role in ["Producer", "Engineer", "Vocals", "Remix", ""] {
        assert!(
            !discogs_role_is_composer(role),
            "expected {role:?} not to be a composer role"
        );
    }
}

/// A composer role artist with no credited_name falls back to its canonical
/// name and logs the fallback rather than dropping the credit silently.
#[test]
fn role_artist_without_credited_name_falls_back_to_canonical_name() {
    let mut release = make_release(vec![make_track("1", "Track 1")]);
    release.extraartists = Some(vec![DiscogsRoleArtist {
        id: Some("discogs-composer-a".to_string()),
        name: "Composer Canonical Name".to_string(),
        role: "Composed By".to_string(),
        credited_name: None,
    }]);

    let mut parsed = None;
    let logs = crate::test_logs::capture_warn_logs(|| {
        parsed = Some(map(&release).unwrap());
    });
    let parsed = parsed.unwrap();

    let composer = parsed
        .artists
        .iter()
        .find(|a| a.discogs_artist_id.as_deref() == Some("discogs-composer-a"))
        .expect("composer role artist imported");
    assert_eq!(composer.name, "Composer Canonical Name");
    assert!(
        logs.contains("has no credited name"),
        "expected a fallback warning, got: {logs}"
    );
}

/// An id-less composer role credit whose name case-insensitively matches
/// the release artist (which carries a Discogs id) commits as that artist
/// rather than a duplicate: a credit with no id is matched by its name,
/// whether or not the artist it names bears a source id.
#[test]
fn id_less_role_credit_reuses_release_artist_by_name() {
    let mut release = make_release(vec![make_track("1", "Track 1")]);
    // Release artist: name "Artist Name A", discogs id "artist-1".
    release.extraartists = Some(vec![DiscogsRoleArtist {
        id: None,
        name: "ARTIST NAME A".to_string(),
        role: "Composed By".to_string(),
        credited_name: None,
    }]);

    let parsed = map(&release).unwrap();

    let committed = committed_artists(&parsed);
    let role = parsed
        .release_artist_roles
        .first()
        .expect("composer role imported");
    assert_eq!(
        committed[&role.artist_id], committed[&parsed.album_artists[0].artist_id],
        "id-less role credit must commit as the release artist, not a duplicate"
    );
}

/// An id-less credit names the release artist however it is accented or
/// spaced, as a library artist is found by its name.
#[test]
fn id_less_role_credit_reuses_release_artist_however_it_is_accented() {
    let mut release = make_release(vec![make_track("1", "Track 1")]);
    release.extraartists = Some(vec![DiscogsRoleArtist {
        id: None,
        name: "Ärtist  Name A".to_string(),
        role: "Composed By".to_string(),
        credited_name: None,
    }]);

    let parsed = map(&release).unwrap();

    let committed = committed_artists(&parsed);
    let role = parsed
        .release_artist_roles
        .first()
        .expect("composer role imported");
    assert_eq!(committed[&role.artist_id], committed[&parsed.album_artists[0].artist_id]);
}

/// An id-less Discogs role credit is its *credited* name — the name stored on
/// the artist row. Two id-less composer credits sharing a credited name but
/// with different canonical names commit as one artist: the name matched and
/// the name stored are the same field.
#[test]
fn id_less_role_credits_dedup_on_credited_name() {
    let mut release = make_release(vec![make_track("1", "Track 1")]);
    release.extraartists = Some(vec![
        DiscogsRoleArtist {
            id: None,
            name: "Canonical One".to_string(),
            role: "Composed By".to_string(),
            credited_name: Some("Shared Credited Name".to_string()),
        },
        DiscogsRoleArtist {
            id: None,
            name: "Canonical Two".to_string(),
            role: "Written-By".to_string(),
            credited_name: Some("Shared Credited Name".to_string()),
        },
    ]);

    let parsed = map(&release).unwrap();

    // The stored name is the credited name; neither canonical name lands.
    assert!(!parsed
        .artists
        .iter()
        .any(|a| a.name == "Canonical One" || a.name == "Canonical Two"));
    // Both release roles commit as one artist of the credited name.
    let committed = committed_artists(&parsed);
    let roles: Vec<_> = parsed
        .release_artist_roles
        .iter()
        .map(|role| {
            let row = parsed
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

/// A row of disc 2 printing no position is numbered by its place on disc 2,
/// not in the whole release.
#[test]
fn a_track_with_no_position_is_numbered_by_its_place_on_its_disc() {
    let release = with_formats(
        make_release(vec![
            make_track("1-1", "Disc One Track"),
            make_track("1-2", "Disc One Track Two"),
            make_track("2-1", "Disc Two Track"),
            make_track("2-2", "Disc Two Track Two"),
            make_track("", "Disc Two Unnumbered"),
        ]),
        &[("CD", "2")],
    );
    let parsed = map(&release).unwrap();
    assert_eq!(
        crate::import::release_values::row_labels(&parsed),
        ["1-1", "1-2", "2-1", "2-2", "2-3"]
    );
}

/// A Discogs album credit keeps its artist name variation as the name it
/// prints and its `join` with the spaces Discogs prints it with: a comma as
/// ", ", any other join between spaces. The last credit keeps no join.
#[test]
fn album_credits_keep_their_printed_names_and_joins() {
    let mut release = make_release(vec![make_track("1", "Track 1")]);
    release.artists = vec![
        DiscogsArtist {
            id: "artist-a".to_string(),
            name: "Artist A".to_string(),
            anv: Some("A Variation".to_string()),
            join: Some("&".to_string()),
        },
        DiscogsArtist {
            id: "artist-b".to_string(),
            name: "Artist B".to_string(),
            anv: None,
            join: Some(",".to_string()),
        },
        DiscogsArtist {
            id: "artist-c".to_string(),
            name: "Artist C".to_string(),
            anv: Some("Artist C".to_string()),
            join: Some("Feat.".to_string()),
        },
    ];

    let credits = metadata(&release).album.artists;

    let stated: Vec<(&str, &str, Option<&str>)> = credits
        .iter()
        .map(|stated| {
            (
                stated.credit.name.as_str(),
                stated.credit.printed(),
                stated.join.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        stated,
        [
            ("Artist A", "A Variation", Some(" & ")),
            ("Artist B", "Artist B", Some(", ")),
            ("Artist C", "Artist C", None),
        ]
    );
    assert_eq!(
        crate::import::SourceCredit::line(&credits),
        "A Variation & Artist B, Artist C"
    );
}
