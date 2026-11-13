use super::*;
use crate::catalog::LookupCatalog;
use crate::discogs::models::{DiscogsArtist, DiscogsRoleArtist, DiscogsTrack};
use crate::records::LookupRef;
use crate::source_release::SourceRelease;

/// A lone Discogs release document as bae keeps it: its own facts and
/// tracklist, no other document behind it.
fn stored(release: &DiscogsRelease) -> SourceRelease {
    SourceRelease {
        release: LookupRef::new(LookupCatalog::Discogs, &release.id),
        source_group_id: release.master_id.clone(),
        metadata: metadata(release),
        other_records: Vec::new(),
        covers: crate::source_release::ReleaseCovers { release: Vec::new(), album: Vec::new() },
        archive_release: None,
        archive_groups: Vec::new(),
        mediums: mediums(release, &pressing(release).1),
        catalog: CatalogFacts::Discogs {
            media: pressing(release).1,
            release_roles: release_roles(release),
        },
        notes: crate::search::discogs_release_notes(release),
        unfetched: Vec::new(),
    }
}

/// Every disc of a release: what a test with no folder to fit reads.
fn all_discs(release: &SourceRelease) -> MediumCoverage {
    MediumCoverage::all(release.mediums.len())
}

/// What the release reads as, every disc of it, laid against
/// `audio_durations_ms` where a folder is given.
fn map_with(
    release: &DiscogsRelease,
    audio_durations_ms: Option<&[u64]>,
) -> Result<ReleaseValues, CatalogError> {
    let stored = stored(release);
    super::values(&stored, &all_discs(&stored), audio_durations_ms)
}

/// Exercise track and credit mapping with the release's own metadata.
fn map(release: &DiscogsRelease) -> Result<ReleaseValues, CatalogError> {
    map_with(release, None)
}

fn map_for_audio(
    release: &DiscogsRelease,
    audio_durations_ms: &[u64],
) -> Result<ReleaseValues, CatalogError> {
    map_with(release, Some(audio_durations_ms))
}

/// Whether any of `roles` credits the Discogs artist `id`.
fn credits_role_to(roles: &[CreditedRole], id: &str) -> bool {
    roles.iter().any(|role| role.artist.discogs_artist_id.as_deref() == Some(id))
}

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
        barcodes: Vec::new(),
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
            matches!(&err, CatalogError::SourceData { detail, .. } if detail.contains("has no album artist")),
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

    let values = map(&release).unwrap();

    // The album artist is the one artist the release credits anywhere.
    assert_eq!(values.album_credits.len(), 1);
    assert_eq!(values.album_credits[0].credit.name, "Artist Name A");
    assert_eq!(values.album_credits[0].credit.discogs_artist_id, None);
    assert_eq!(values.tracks[0].credits, TrackCredits::AlbumArtists);
    assert!(values.release_roles.is_empty());
    assert!(values.tracks[0].roles.is_empty());
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
            tracks: None,
        },
        DiscogsRoleArtist {
            id: Some("discogs-producer-a".to_string()),
            name: "Producer Name A".to_string(),
            role: "Producer".to_string(),
            credited_name: Some("Producer Credit A".to_string()),
            tracks: None,
        },
    ]);
    release.tracklist[0].extraartists = Some(vec![
        DiscogsRoleArtist {
            id: Some("discogs-composer-a".to_string()),
            name: "Composer Name A".to_string(),
            role: "Written-By".to_string(),
            credited_name: Some("Composer Credit A".to_string()),
            tracks: None,
        },
        DiscogsRoleArtist {
            id: Some("discogs-engineer-a".to_string()),
            name: "Engineer Name A".to_string(),
            role: "Engineer".to_string(),
            credited_name: Some("Engineer Credit A".to_string()),
            tracks: None,
        },
    ]);

    let values = map(&release).unwrap();

    assert!(values.works.works.is_empty());
    assert!(values.works.parts.is_empty());
    assert!(values.tracks[0].works.is_empty());

    let composer = Some("discogs-composer-a");
    assert!(values.release_roles.iter().any(|role| {
        role.artist.discogs_artist_id.as_deref() == composer
            && role.role.as_deref() == Some("Composed By")
    }));
    assert!(values.tracks[0].roles.iter().any(|role| {
        role.artist.discogs_artist_id.as_deref() == composer
            && role.role.as_deref() == Some("Written-By")
    }));
    for producer_or_engineer in ["discogs-producer-a", "discogs-engineer-a"] {
        assert!(!credits_role_to(&values.release_roles, producer_or_engineer));
        assert!(!credits_role_to(&values.tracks[0].roles, producer_or_engineer));
    }
    assert!(values
        .album_credits
        .iter()
        .all(|credit| credit.credit.discogs_artist_id.as_deref() != composer));
    assert_eq!(values.tracks[0].credits, TrackCredits::AlbumArtists);
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
        crate::release_values::value_labels(&parsed),
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
        crate::release_values::value_labels(&parsed),
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
        crate::release_values::value_labels(&parsed),
        ["A1", "A2", "B1", "B2", "C1", "C2", "C3", "D1", "D2"]
    );
    assert_eq!(tracks[0].title, "Track One");
    assert_eq!(tracks[1].title, "Track Two");
    assert_eq!(tracks[2].title, "Medley: Part I \u{2013} Part II \u{2013} Part III");
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
    assert_eq!(crate::release_values::value_labels(&parsed), ["1", "2", "3"]);
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

    let values = map_for_audio(&release, &[1; 2]).unwrap();
    let tracks = &values.tracks;

    // 2 tracks: A1 and collapsed B1
    assert_eq!(tracks.len(), 2);
    assert_eq!(tracks[1].title, "Suite: Movement 1 \u{2013} Movement 2");
    assert_eq!(tracks[0].credits, TrackCredits::AlbumArtists);

    // The collapsed track credits the sub-tracks' artist once, however many
    // sub-tracks credit it.
    let TrackCredits::Own(credits) = &tracks[1].credits else {
        panic!("the collapsed track credits its own artist: {:?}", tracks[1].credits);
    };
    assert_eq!(credits.len(), 1, "one credit for the collapsed track: {credits:?}");
    assert_eq!(credits[0].credit.name, "Sub Artist");
    assert_eq!(credits[0].credit.discogs_artist_id.as_deref(), Some("sub-artist-1"));
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
        assert!(discogs_role_is_composer(role), "expected {role:?} to be a composer role");
    }
    for role in ["Producer", "Engineer", "Vocals", "Remix", ""] {
        assert!(!discogs_role_is_composer(role), "expected {role:?} not to be a composer role");
    }
}

/// A role credit is its artist by the artist's own name, printing the name
/// variation the release gives it; one with none prints the artist's name.
#[test]
fn role_credit_names_its_artist_and_prints_the_release_s_variation() {
    let mut release = make_release(vec![make_track("1", "Track 1")]);
    release.extraartists = Some(vec![
        DiscogsRoleArtist {
            id: Some("discogs-composer-a".to_string()),
            name: "Composer Canonical Name (2)".to_string(),
            role: "Composed By".to_string(),
            credited_name: Some("Composer Printed Name".to_string()),
            tracks: None,
        },
        DiscogsRoleArtist {
            id: Some("discogs-composer-b".to_string()),
            name: "Composer Name B".to_string(),
            role: "Composed By".to_string(),
            credited_name: None,
            tracks: None,
        },
    ]);
    release.tracklist[0].extraartists = Some(vec![DiscogsRoleArtist {
        id: Some("discogs-composer-a".to_string()),
        name: "Composer Canonical Name (2)".to_string(),
        role: "Written-By".to_string(),
        credited_name: Some("Composer Printed Name".to_string()),
        tracks: None,
    }]);

    let values = map(&release).unwrap();

    let printed = &values.release_roles[0].artist;
    assert_eq!(printed.name, "Composer Canonical Name (2)");
    assert_eq!(printed.credited_as.as_deref(), Some("Composer Printed Name"));
    let plain = &values.release_roles[1].artist;
    assert_eq!(plain.name, "Composer Name B");
    assert_eq!(plain.credited_as, None);
    let track_role = &values.tracks[0].roles[0].artist;
    assert_eq!(track_role.name, "Composer Canonical Name (2)");
    assert_eq!(track_role.credited_as.as_deref(), Some("Composer Printed Name"));
}

/// An id-less composer role credit reads as the name it states, with no id:
/// which library artist it is, by that name, is the commit's to decide.
#[test]
fn id_less_role_credit_keeps_its_name_and_no_id() {
    let mut release = make_release(vec![make_track("1", "Track 1")]);
    // Release artist: name "Artist Name A", discogs id "artist-1".
    release.extraartists = Some(vec![DiscogsRoleArtist {
        id: None,
        name: "ARTIST NAME A".to_string(),
        role: "Composed By".to_string(),
        credited_name: None,
        tracks: None,
    }]);

    let values = map(&release).unwrap();

    let role = values.release_roles.first().expect("composer role read");
    assert_eq!(role.artist.name, "ARTIST NAME A");
    assert_eq!(role.artist.discogs_artist_id, None);
    assert_eq!(values.album_credits[0].credit.discogs_artist_id.as_deref(), Some("artist-1"));
}

/// An id-less credit keeps its name as printed, accents and spacing and all.
#[test]
fn id_less_role_credit_keeps_its_name_as_printed() {
    let mut release = make_release(vec![make_track("1", "Track 1")]);
    release.extraartists = Some(vec![DiscogsRoleArtist {
        id: None,
        name: "Ärtist  Name A".to_string(),
        role: "Composed By".to_string(),
        credited_name: None,
        tracks: None,
    }]);

    let values = map(&release).unwrap();

    let role = values.release_roles.first().expect("composer role read");
    assert_eq!(role.artist.name, "Ärtist  Name A");
    assert_eq!(role.artist.discogs_artist_id, None);
}

/// Two id-less role credits with different canonical names are two artists
/// that happen to print one name.
#[test]
fn id_less_role_credits_keep_their_own_names_apart() {
    let mut release = make_release(vec![make_track("1", "Track 1")]);
    release.extraartists = Some(vec![
        DiscogsRoleArtist {
            id: None,
            name: "Canonical One".to_string(),
            role: "Composed By".to_string(),
            credited_name: Some("Shared Printed Name".to_string()),
            tracks: None,
        },
        DiscogsRoleArtist {
            id: None,
            name: "Canonical Two".to_string(),
            role: "Written-By".to_string(),
            credited_name: Some("Shared Printed Name".to_string()),
            tracks: None,
        },
    ]);

    let values = map(&release).unwrap();

    let names: Vec<_> = values.release_roles.iter().map(|role| role.artist.name.as_str()).collect();
    assert_eq!(names, ["Canonical One", "Canonical Two"]);
    assert!(values
        .release_roles
        .iter()
        .all(|role| role.artist.credited_as.as_deref() == Some("Shared Printed Name")));
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
    assert_eq!(crate::release_values::value_labels(&parsed), ["1-1", "1-2", "2-1", "2-2", "2-3"]);
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
            (stated.credit.name.as_str(), stated.credit.printed(), stated.join.as_deref())
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
        crate::credits::SourceCredit::line(&credits).plain_text(),
        "A Variation & Artist B, Artist C"
    );
}

/// Every Barcode identifier reaches the pressing as printed, each code once:
/// an EAN and a UPC are both kept, and one code listed as printed and as
/// scanned is kept in its first spelling.
#[test]
fn release_barcodes_reach_pressing_metadata() {
    let json = r#"{"id":123,"title":"Album Title","identifiers":[
            {"type":"Matrix / Runout","value":"MATRIX-7"},
            {"type":"Barcode","value":" \t"},
            {"type":"Barcode","value":"0 12345 67890 5","description":"Text"},
            {"type":"Barcode","value":"012345678905","description":"Scanned"},
            {"type":"Barcode","value":"5 012345 678900","description":"EAN"}
        ]}"#;
    let release = crate::discogs::documents::parse_discogs_release_json(json).unwrap();
    assert_eq!(release.barcodes, ["0 12345 67890 5", "5 012345 678900"]);
    let payloads: crate::payloads::ReleasePayloads = crate::payloads::ReleasePayloads::for_test(
        crate::records::LookupRef::new(LookupCatalog::Discogs, "123"),
        json.to_string(),
        Vec::new(),
    );
    assert_eq!(payloads.extract().unwrap().detail(&[]).barcodes, release.barcodes);
    assert_eq!(metadata(&release).pressing.barcodes, release.barcodes);
}

#[test]
fn blank_pressing_fields_are_absent_in_discogs_documents() {
    for value in ["", " \t"] {
        let raw = serde_json::json!({
            "id":123, "title":"Album Title", "country":value,
            "formats":[{"name":value}], "labels":[{"name":value,"catno":value}],
            "identifiers":[{"type":"Barcode","value":value}]
        })
        .to_string();
        let release = crate::discogs::documents::parse_discogs_release_json(&raw).unwrap();
        assert!(release.country.is_none());
        let (pressing, media) = pressing(&release);
        assert!(pressing.facts.is_empty());
        assert_eq!(media, bae_types::pressing::StatedMedia::Undescribed);
        assert!(release.labels.is_empty());
        assert!(pressing.labels.is_empty());
        assert!(release.barcodes.is_empty());
    }
}

/// Every label is read with its own catalog number, and the pressing keeps
/// them all.
#[test]
fn every_label_and_its_catalog_number_is_read() {
    use bae_types::pressing::ReleaseLabel;
    let raw = serde_json::json!({
        "id": 123, "title": "Album",
        "labels": [
            {"name": "Label A", "catno": "AB 100"},
            {"name": "Label B", "catno": "CL 719"},
            {"name": "Label B", "catno": "CL 719"},
            {"name": "Label C", "catno": ""}
        ]
    })
    .to_string();
    let release = crate::discogs::documents::parse_discogs_release_json(&raw).unwrap();
    let expected = vec![
        ReleaseLabel::of(Some("Label A"), Some("AB 100")),
        ReleaseLabel::of(Some("Label B"), Some("CL 719")),
        ReleaseLabel::of(Some("Label C"), None),
    ];
    assert_eq!(release.labels, expected);
    assert_eq!(pressing(&release).0.labels, expected);
}

/// A position that is a side letter alone names that side.
#[test]
fn a_side_letter_alone_names_its_side() {
    let release =
        make_release(vec![make_track("A", "Side A Track"), make_track("B", "Side B Track")]);

    let release = with_formats(release, &[("Vinyl", "1")]);
    let parsed = map(&release).unwrap();
    assert_eq!(crate::release_values::value_labels(&parsed), ["A1", "B1"]);
    assert_eq!(parsed.mediums[0].sides.get(), 2);
}

/// A Discogs name variation is kept only where it prints another name than
/// the artist's own, the spaces around either aside, as a MusicBrainz
/// credit name is.
#[test]
fn a_name_variation_printing_the_artist_s_own_name_is_not_kept() {
    let credit = |name: &str, anv: &str| {
        discogs_track_artist_ref(&DiscogsArtist {
            anv: Some(anv.to_string()),
            join: None,
            name: name.to_string(),
            id: "discogs-artist-a".to_string(),
        })
        .credited_as
    };
    assert_eq!(credit("Artist Name ", "Artist Name"), None);
    assert_eq!(credit("Artist Name", " Artist Name "), None);
    assert_eq!(credit("Artist Name", " Other Name "), Some("Other Name".to_string()));
}

/// A release-level credit naming tracks is those tracks' credit, not the
/// release's.
#[test]
fn a_release_credit_naming_tracks_is_those_tracks_credit() {
    let mut release = make_release(vec![
        make_track("1", "Track 1"),
        make_track("2", "Track 2"),
        make_track("3", "Track 3"),
        make_track("4", "Track 4"),
        make_track("5", "Track 5"),
    ]);
    release.extraartists = Some(vec![
        DiscogsRoleArtist {
            id: Some("discogs-composer-a".to_string()),
            name: "Composer Name A".to_string(),
            role: "Written-By".to_string(),
            credited_name: None,
            tracks: Some("2, 5".to_string()),
        },
        DiscogsRoleArtist {
            id: Some("discogs-composer-b".to_string()),
            name: "Composer Name B".to_string(),
            role: "Composed By".to_string(),
            credited_name: None,
            tracks: None,
        },
    ]);

    let values = map(&release).unwrap();

    let release_names: Vec<_> =
        values.release_roles.iter().map(|role| role.artist.name.as_str()).collect();
    assert_eq!(release_names, ["Composer Name B"], "the unscoped credit stays the release's");
    for (index, track) in values.tracks.iter().enumerate() {
        let names: Vec<_> = track.roles.iter().map(|role| role.artist.name.as_str()).collect();
        let expected: &[&str] = if index == 1 || index == 4 { &["Composer Name A"] } else { &[] };
        assert_eq!(names, expected, "track {}", index + 1);
    }
}

/// A credit naming the sub-tracks of a collapsed index reaches the collapsed
/// track once, however many sub-tracks it names.
#[test]
fn a_credit_on_the_sub_tracks_of_a_collapsed_index_reaches_the_collapsed_track_once() {
    let mut release = make_release(vec![
        make_track("A1", "Normal Track"),
        make_heading("Suite"),
        make_track("B1i", "Movement 1"),
        make_track("B1ii", "Movement 2"),
    ]);
    release.extraartists = Some(vec![DiscogsRoleArtist {
        id: Some("discogs-composer-a".to_string()),
        name: "Composer Name A".to_string(),
        role: "Written-By".to_string(),
        credited_name: None,
        tracks: Some("B1i to B1ii".to_string()),
    }]);

    let values = map_for_audio(&release, &[1; 2]).unwrap();

    assert_eq!(values.tracks.len(), 2);
    assert!(values.tracks[0].roles.is_empty());
    assert_eq!(values.tracks[1].roles.len(), 1, "{:?}", values.tracks[1].roles);
}

/// A credit naming only tracks the tracklist does not hold is dropped.
#[test]
fn a_release_credit_naming_no_track_of_the_tracklist_is_dropped() {
    let mut release = make_release(vec![make_track("1", "Track 1")]);
    release.extraartists = Some(vec![DiscogsRoleArtist {
        id: Some("discogs-composer-a".to_string()),
        name: "Composer Name A".to_string(),
        role: "Written-By".to_string(),
        credited_name: None,
        tracks: Some("7".to_string()),
    }]);

    let values = map(&release).unwrap();

    assert!(values.release_roles.is_empty());
    assert!(values.tracks[0].roles.is_empty());
}
