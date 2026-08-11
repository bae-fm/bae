use super::*;
use crate::musicbrainz::{
    MbArtistCredit, MbArtistRef, MbMedium, MbRecording, MbReleaseResponse, MbTrack, MbWork,
};
use coven::FixedClock;
use coven::SequentialIdProvider;

/// Run the mapper with deterministic fakes. Exercises the real
/// `map_mb_response_to_db`; only the clock/id inputs are faked.
fn map(
    response: &MbReleaseResponse,
    master_year: Option<u32>,
    discogs_release: Option<crate::discogs::DiscogsRelease>,
) -> Result<ParsedAlbum, ImportError> {
    let clock = FixedClock(
        chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc),
    );
    let ids = SequentialIdProvider::new("mb");
    map_mb_response_to_db(response, master_year, discogs_release, &clock, &ids)
}

/// The id of the `works` row the parsed release minted for a MusicBrainz
/// work. Row ids are minted, so a link is checked against this, never
/// against the MBID.
fn work_row_id(parsed: &ParsedAlbum, musicbrainz_work_id: &str) -> String {
    parsed
        .work_graph
        .works
        .iter()
        .find(|work| work.musicbrainz_work_id == musicbrainz_work_id)
        .unwrap_or_else(|| panic!("no works row for {musicbrainz_work_id}"))
        .id
        .clone()
}

fn make_mb_track(number: &str, title: &str) -> MbTrack {
    MbTrack {
        position: None,
        number: Some(number.to_string()),
        title: None,
        length: None,
        recording: Some(MbRecording {
            id: None,
            title: Some(title.to_string()),
            artist_credit: vec![],
            relations: vec![],
        }),
        artist_credit: vec![],
    }
}

fn make_response(media: Vec<MbMedium>) -> MbReleaseResponse {
    MbReleaseResponse {
        id: "test-release".to_string(),
        title: "Album Title A".to_string(),
        date: Some("2024".to_string()),
        country: None,
        barcode: None,
        artist_credit: vec![MbArtistCredit {
            name: "Artist Name A".to_string(),
            artist: Some(MbArtistRef {
                id: Some("artist-1".to_string()),
                name: Some("Artist Name A".to_string()),
                sort_name: Some("Artist Name A".to_string()),
            }),
        }],
        release_group: Some(crate::musicbrainz::MbReleaseGroupRef {
            id: "rg-test".to_string(),
            first_release_date: Some("2024".to_string()),
            relations: None,
        }),
        label_info: vec![],
        media,
        relations: vec![],
        cover_art_archive: crate::musicbrainz::MbCoverArtArchive {
            front: false,
            darkened: false,
        },
    }
}

#[test]
fn test_cd_two_media_each_one_side() {
    let response = make_response(vec![
        MbMedium {
            format: Some("CD".to_string()),
            tracks: vec![make_mb_track("1", "Track 1"), make_mb_track("2", "Track 2")],
        },
        MbMedium {
            format: Some("CD".to_string()),
            tracks: vec![make_mb_track("1", "Track 3"), make_mb_track("2", "Track 4")],
        },
    ]);

    let parsed = map(&response, Some(2024), None).unwrap();
    let tracks = &parsed.tracks;

    assert_eq!(tracks.len(), 4);

    // Medium 1 = side 1
    assert_eq!(tracks[0].side, 1);
    assert_eq!(tracks[0].track_number, Some(1));
    assert_eq!(tracks[1].side, 1);
    assert_eq!(tracks[1].track_number, Some(2));

    // Medium 2 = side 2
    assert_eq!(tracks[2].side, 2);
    assert_eq!(tracks[2].track_number, Some(1));
    assert_eq!(tracks[3].side, 2);
    assert_eq!(tracks[3].track_number, Some(2));
}

#[test]
fn test_vinyl_one_medium_two_sides() {
    let response = make_response(vec![MbMedium {
        format: Some("12\" Vinyl".to_string()),
        tracks: vec![
            make_mb_track("A1", "Track A1"),
            make_mb_track("A2", "Track A2"),
            make_mb_track("B1", "Track B1"),
            make_mb_track("B2", "Track B2"),
        ],
    }]);

    let parsed = map(&response, Some(2024), None).unwrap();
    let tracks = &parsed.tracks;

    assert_eq!(tracks.len(), 4);

    // A tracks = side 1
    assert_eq!(tracks[0].side, 1);
    assert_eq!(tracks[0].track_number, Some(1));
    assert_eq!(tracks[1].side, 1);
    assert_eq!(tracks[1].track_number, Some(2));

    // B tracks = side 2
    assert_eq!(tracks[2].side, 2);
    assert_eq!(tracks[2].track_number, Some(1));
    assert_eq!(tracks[3].side, 2);
    assert_eq!(tracks[3].track_number, Some(2));
}

/// 2LP vinyl: two media, each with two sides (A/B and C/D).
/// Sides must be 1,2,3,4 — not 1,2,3+2,4+2.
#[test]
fn test_vinyl_two_media_four_sides() {
    let response = make_response(vec![
        MbMedium {
            format: Some("12\" Vinyl".to_string()),
            tracks: vec![
                make_mb_track("A1", "Track A1"),
                make_mb_track("A2", "Track A2"),
                make_mb_track("B1", "Track B1"),
                make_mb_track("B2", "Track B2"),
            ],
        },
        MbMedium {
            format: Some("12\" Vinyl".to_string()),
            tracks: vec![
                make_mb_track("C1", "Track C1"),
                make_mb_track("C2", "Track C2"),
                make_mb_track("D1", "Track D1"),
                make_mb_track("D2", "Track D2"),
            ],
        },
    ]);

    let parsed = map(&response, Some(2024), None).unwrap();
    let tracks = &parsed.tracks;

    assert_eq!(tracks.len(), 8);

    // Medium 1: A = side 1, B = side 2
    assert_eq!(tracks[0].side, 1);
    assert_eq!(tracks[1].side, 1);
    assert_eq!(tracks[2].side, 2);
    assert_eq!(tracks[3].side, 2);

    // Medium 2: C = side 3, D = side 4
    assert_eq!(tracks[4].side, 3);
    assert_eq!(tracks[5].side, 3);
    assert_eq!(tracks[6].side, 4);
    assert_eq!(tracks[7].side, 4);
}

#[test]
fn test_single_medium_cd_all_side_one() {
    let response = make_response(vec![MbMedium {
        format: Some("CD".to_string()),
        tracks: vec![
            make_mb_track("1", "Track 1"),
            make_mb_track("2", "Track 2"),
            make_mb_track("3", "Track 3"),
        ],
    }]);

    let parsed = map(&response, Some(2024), None).unwrap();
    let tracks = &parsed.tracks;

    assert_eq!(tracks.len(), 3);

    // All tracks on side 1
    assert_eq!(tracks[0].side, 1);
    assert_eq!(tracks[0].track_number, Some(1));
    assert_eq!(tracks[1].side, 1);
    assert_eq!(tracks[1].track_number, Some(2));
    assert_eq!(tracks[2].side, 1);
    assert_eq!(tracks[2].track_number, Some(3));
}

/// A vinyl medium track without a leading side letter is malformed MB data:
/// there's no way to assign it to a side. Surface the error instead of
/// silently bucketing it onto side 1.
#[test]
fn test_vinyl_track_missing_side_letter_errors() {
    let response = make_response(vec![MbMedium {
        format: Some("12\" Vinyl".to_string()),
        tracks: vec![
            make_mb_track("A1", "Track A1"),
            MbTrack {
                position: None,
                number: None,
                title: None,
                length: None,
                recording: Some(MbRecording {
                    id: None,
                    title: Some("Side-less Track".to_string()),
                    artist_credit: vec![],
                    relations: vec![],
                }),
                artist_credit: vec![],
            },
        ],
    }]);

    let err = map(&response, Some(2024), None)
        .expect_err("expected error for vinyl track without side letter");
    assert!(
        matches!(&err, ImportError::SourceData { detail, .. } if detail.contains("no side letter")),
        "unexpected error message: {}",
        err
    );
}

/// A track number like "1" on a vinyl medium has no side letter to derive
/// offset from. Same failure mode as a missing number.
#[test]
fn test_vinyl_track_numeric_only_errors() {
    let response = make_response(vec![MbMedium {
        format: Some("12\" Vinyl".to_string()),
        tracks: vec![
            make_mb_track("A1", "Track A1"),
            make_mb_track("1", "Numeric-only Track"),
        ],
    }]);

    let err = map(&response, Some(2024), None)
        .expect_err("expected error for vinyl track with numeric-only number");
    assert!(
        matches!(&err, ImportError::SourceData { detail, .. } if detail.contains("no side letter")),
        "unexpected error message: {}",
        err
    );
}

#[test]
fn medium_with_no_tracks_returns_err() {
    let response = make_response(vec![MbMedium {
        format: Some("CD".to_string()),
        tracks: vec![],
    }]);

    let result = map(&response, Some(2024), None);
    assert!(matches!(
        result.unwrap_err(),
        ImportError::SourceData { detail, .. } if detail.contains("no tracks")
    ));
}

#[test]
fn track_title_is_used_when_recording_title_is_missing() {
    let response = make_response(vec![MbMedium {
        format: Some("CD".to_string()),
        tracks: vec![MbTrack {
            position: None,
            number: Some("1".to_string()),
            title: Some("Track Title From Track".to_string()),
            length: None,
            recording: Some(MbRecording {
                id: None,
                title: None,
                artist_credit: vec![],
                relations: vec![],
            }),
            artist_credit: vec![],
        }],
    }]);

    let parsed = map(&response, Some(2024), None).unwrap();

    assert_eq!(parsed.tracks[0].title, "Track Title From Track");
}

#[test]
fn track_without_recording_or_track_title_returns_err() {
    let response = make_response(vec![MbMedium {
        format: Some("CD".to_string()),
        tracks: vec![MbTrack {
            position: None,
            number: Some("1".to_string()),
            title: None,
            length: None,
            recording: Some(MbRecording {
                id: None,
                title: None,
                artist_credit: vec![],
                relations: vec![],
            }),
            artist_credit: vec![],
        }],
    }]);

    let err = map(&response, Some(2024), None)
        .expect_err("expected missing MusicBrainz track title to return an error");

    assert!(
        matches!(&err, ImportError::SourceData { detail, .. } if detail.contains("has no track title")),
        "unexpected error message: {err}"
    );
}

/// The one MB → pressing projection: the release's own year (not the release
/// group's), the first medium's format, the first label's name and catalog
/// number, the country and the barcode. The mapper, the picker detail, and a
/// search result all read it.
#[test]
fn pressing_reads_year_format_first_label_country_and_barcode() {
    let mut response = make_response(vec![MbMedium {
        format: Some("12\" Vinyl".to_string()),
        tracks: vec![make_mb_track("A1", "Track A1")],
    }]);
    response.date = Some("1971-03-01".to_string());
    response.country = Some("GB".to_string());
    response.barcode = Some("012345678905".to_string());
    response.label_info = vec![
        crate::musicbrainz::MbLabelInfo {
            label: Some(crate::musicbrainz::MbLabel {
                name: Some("Island".to_string()),
            }),
            catalog_number: Some("ILPS 9145".to_string()),
        },
        crate::musicbrainz::MbLabelInfo {
            label: Some(crate::musicbrainz::MbLabel {
                name: Some("Reissue Label".to_string()),
            }),
            catalog_number: Some("RE-2".to_string()),
        },
    ];
    // The release group's first release predates this pressing; the pressing
    // year is the release's own date, and only the album year follows the group.
    response.release_group.as_mut().unwrap().first_release_date = Some("1969".to_string());

    let pressing = pressing(&response);

    assert_eq!(
        pressing,
        Pressing {
            year: Some(1971),
            format: Some("12\" Vinyl".to_string()),
            label: Some("Island".to_string()),
            catalog_number: Some("ILPS 9145".to_string()),
            country: Some("GB".to_string()),
            barcode: Some("012345678905".to_string()),
        }
    );

    // What the mapper commits is what the projection says.
    let parsed = map(&response, None, None).unwrap();
    assert_eq!(parsed.release.pressing, pressing);
    assert_eq!(parsed.album.year, Some(1969));
}

#[test]
fn extract_discogs_release_id_cases() {
    // The leading numeric segment after `/release/` is the id: bare,
    // trailing-slash, and slug-suffixed forms all yield it; a non-numeric
    // segment, an empty path, or an unrelated host yield None.
    let cases = [
        ("https://www.discogs.com/release/12345", Some("12345")),
        ("https://www.discogs.com/release/12345/", Some("12345")),
        (
            "https://www.discogs.com/release/12345-Album-Title",
            Some("12345"),
        ),
        (
            "https://www.discogs.com/release/12345-Album-Title/",
            Some("12345"),
        ),
        ("https://www.discogs.com/release/abc", None),
        ("https://www.discogs.com/release/", None),
        ("https://example.com/something/abc", None),
    ];
    for (url, expected) in cases {
        assert_eq!(
            extract_discogs_release_id(url),
            expected.map(str::to_string),
            "url: {url}"
        );
    }
}

// ── identities (parsed.identities) ─────────────────────────────────

fn discogs_release_with_master(master_id: Option<String>) -> crate::discogs::DiscogsRelease {
    crate::discogs::DiscogsRelease {
        id: "d-rel-99".to_string(),
        title: "Album Title A".to_string(),
        year: Some(2024),
        format: vec![],
        country: None,
        label: vec![],
        cover_image: None,
        thumb: None,
        catno: None,
        artists: vec![],
        tracklist: vec![],
        extraartists: Some(vec![]),
        master_id,
    }
}

#[test]
fn test_map_mb_no_cross_ref_yields_only_mb_identity() {
    let response = make_response(vec![MbMedium {
        format: Some("CD".to_string()),
        tracks: vec![make_mb_track("1", "Track 1")],
    }]);

    let parsed = map(&response, None, None).unwrap();

    assert_eq!(parsed.identities.len(), 1);
    let mb = &parsed.identities[0];
    assert_eq!(mb.source, MetadataSource::MusicBrainz);
    assert_eq!(mb.source_group_id, "rg-test");
    assert_eq!(mb.source_release_id.as_deref(), Some("test-release"));
}

#[test]
fn release_with_no_artist_credits_returns_err() {
    let mut response = make_response(vec![MbMedium {
        format: Some("CD".to_string()),
        tracks: vec![make_mb_track("1", "Track 1")],
    }]);
    response.artist_credit = vec![];

    let err =
        map(&response, None, None).expect_err("expected missing artist credits to return an error");

    assert!(
        matches!(&err, ImportError::SourceData { detail, .. } if detail.contains("has no artist credits")),
        "unexpected error message: {err}"
    );
}

#[test]
fn release_with_no_release_group_returns_err() {
    let mut response = make_response(vec![MbMedium {
        format: Some("CD".to_string()),
        tracks: vec![make_mb_track("1", "Track 1")],
    }]);
    response.release_group = None;

    let err =
        map(&response, None, None).expect_err("expected missing release group to return an error");

    assert!(
        matches!(&err, ImportError::SourceData { detail, .. } if detail.contains("missing release_group")),
        "unexpected error message: {err}"
    );
}

#[test]
fn test_map_mb_cross_ref_no_master_id_yields_only_mb_identity() {
    // Cross-ref hit but the linked Discogs release has no master_id —
    // the parser doesn't fabricate a group; only the MB row is emitted.
    let response = make_response(vec![MbMedium {
        format: Some("CD".to_string()),
        tracks: vec![make_mb_track("1", "Track 1")],
    }]);
    let discogs_release = discogs_release_with_master(None);

    let parsed = map(&response, None, Some(discogs_release)).unwrap();

    assert_eq!(parsed.identities.len(), 1);
    assert_eq!(parsed.identities[0].source, MetadataSource::MusicBrainz);
}

#[test]
fn test_map_mb_cross_ref_with_master_id_yields_two_identity_rows() {
    // Cross-ref hit AND the linked Discogs release carries a master_id
    // — two rows: MB + Discogs. Both Exact (release IDs present).
    let response = make_response(vec![MbMedium {
        format: Some("CD".to_string()),
        tracks: vec![make_mb_track("1", "Track 1")],
    }]);
    let discogs_release = discogs_release_with_master(Some("d-master-123".to_string()));

    let parsed = map(&response, None, Some(discogs_release)).unwrap();

    assert_eq!(parsed.identities.len(), 2);

    let mb = &parsed.identities[0];
    assert_eq!(mb.source, MetadataSource::MusicBrainz);
    assert_eq!(mb.source_group_id, "rg-test");
    assert_eq!(mb.source_release_id.as_deref(), Some("test-release"));

    let discogs = &parsed.identities[1];
    assert_eq!(discogs.source, MetadataSource::Discogs);
    assert_eq!(discogs.source_group_id, "d-master-123");
    assert_eq!(discogs.source_release_id.as_deref(), Some("d-rel-99"));
}

fn credit(id: &str, name: &str) -> MbArtistCredit {
    MbArtistCredit {
        name: name.to_string(),
        artist: Some(MbArtistRef {
            id: Some(id.to_string()),
            name: Some(name.to_string()),
            sort_name: None,
        }),
    }
}

#[test]
fn release_artist_credit_name_is_used_when_artist_payload_name_is_missing() {
    let mut response = make_response(vec![MbMedium {
        format: Some("CD".to_string()),
        tracks: vec![make_mb_track("1", "Track 1")],
    }]);
    let mut name_only_credit = credit("artist-credit-name-only", "Credit Artist Name");
    name_only_credit.artist.as_mut().unwrap().name = None;
    response.artist_credit = vec![name_only_credit];

    let parsed = map(&response, Some(2024), None).unwrap();

    let artist = parsed
        .artists
        .iter()
        .find(|artist| artist.musicbrainz_artist_id.as_deref() == Some("artist-credit-name-only"))
        .expect("release artist imported from credit name");
    assert_eq!(artist.name, "Credit Artist Name");
    assert_eq!(parsed.album.artist_id, artist.id);
}

#[test]
fn missing_mb_sort_names_remain_absent() {
    let mut response = make_response(vec![{
        let mut track = make_mb_track("1", "Track 1");
        track.recording.as_mut().unwrap().relations = vec![MbRelation {
            target_type: Some("artist".to_string()),
            relation_type: Some("composer".to_string()),
            artist: Some(MbArtistRef {
                id: Some("composer-artist-a".to_string()),
                name: Some("Composer Name A".to_string()),
                sort_name: None,
            }),
            ..MbRelation::default()
        }];
        MbMedium {
            format: Some("CD".to_string()),
            tracks: vec![track],
        }
    }]);
    response.artist_credit[0].artist.as_mut().unwrap().sort_name = None;

    let parsed = map(&response, Some(2024), None).unwrap();

    let release_artist = parsed
        .artists
        .iter()
        .find(|artist| artist.musicbrainz_artist_id.as_deref() == Some("artist-1"))
        .expect("release artist imported");
    let composer = parsed
        .artists
        .iter()
        .find(|artist| artist.musicbrainz_artist_id.as_deref() == Some("composer-artist-a"))
        .expect("composer artist imported");

    assert_eq!(release_artist.sort_name, None);
    assert_eq!(composer.sort_name, None);
}

#[test]
fn nested_recording_work_imports_composer_work_graph() {
    let composer_ref = MbArtistRef {
        id: Some("composer-artist-a".to_string()),
        name: Some("Composer Name A".to_string()),
        sort_name: Some("Composer Name A".to_string()),
    };
    let lyricist_ref = MbArtistRef {
        id: Some("lyricist-artist-a".to_string()),
        name: Some("Lyricist Name A".to_string()),
        sort_name: Some("Lyricist Name A".to_string()),
    };
    let child_work = MbWork {
        id: "mb-work-child-a".to_string(),
        title: "Work Part A".to_string(),
        disambiguation: None,
        work_type: Some("part".to_string()),
        relations: vec![MbRelation {
            target_type: Some("artist".to_string()),
            relation_type: Some("composer".to_string()),
            artist: Some(composer_ref.clone()),
            target_credit: Some("Composer Name A".to_string()),
            ..MbRelation::default()
        }],
    };
    let parent_work = MbWork {
        id: "mb-work-parent-a".to_string(),
        title: "Work Title A".to_string(),
        disambiguation: Some("work disambiguation".to_string()),
        work_type: Some("work".to_string()),
        relations: vec![
            MbRelation {
                target_type: Some("artist".to_string()),
                relation_type: Some("composer".to_string()),
                artist: Some(composer_ref),
                target_credit: Some("Composer Name A".to_string()),
                ..MbRelation::default()
            },
            MbRelation {
                target_type: Some("artist".to_string()),
                relation_type: Some("lyricist".to_string()),
                artist: Some(lyricist_ref),
                target_credit: Some("Lyricist Name A".to_string()),
                ..MbRelation::default()
            },
            MbRelation {
                target_type: Some("work".to_string()),
                relation_type: Some("parts".to_string()),
                direction: Some("forward".to_string()),
                work: Some(child_work),
                ..MbRelation::default()
            },
        ],
    };
    let mut track_one = make_mb_track("1", "Track Title 1");
    track_one.recording.as_mut().unwrap().relations = vec![MbRelation {
        target_type: Some("work".to_string()),
        relation_type: Some("performance".to_string()),
        work: Some(parent_work.clone()),
        ..MbRelation::default()
    }];
    let mut track_two = make_mb_track("2", "Track Title 2");
    track_two.recording.as_mut().unwrap().relations = vec![MbRelation {
        target_type: Some("work".to_string()),
        relation_type: Some("performance".to_string()),
        work: Some(parent_work),
        ..MbRelation::default()
    }];
    let response = make_response(vec![MbMedium {
        format: Some("CD".to_string()),
        tracks: vec![track_one, track_two],
    }]);

    let parsed = map(&response, Some(2024), None).unwrap();

    let work_graph = &parsed.work_graph;
    assert_eq!(work_graph.works.len(), 2);
    let parent_id = work_row_id(&parsed, "mb-work-parent-a");
    let child_id = work_row_id(&parsed, "mb-work-child-a");
    assert!(work_graph
        .track_works
        .iter()
        .any(|link| { link.track_id == parsed.tracks[0].id && link.work_id == parent_id }));
    assert!(work_graph
        .work_parts
        .iter()
        .any(|part| { part.parent_work_id == parent_id && part.child_work_id == child_id }));

    let composer = parsed
        .artists
        .iter()
        .find(|artist| artist.musicbrainz_artist_id.as_deref() == Some("composer-artist-a"))
        .expect("composer artist imported");
    assert!(work_graph
        .work_artists
        .iter()
        .any(|link| { link.artist_id == composer.id }));
    assert_eq!(
        work_graph
            .work_artists
            .iter()
            .filter(|link| { link.artist_id == composer.id })
            .count(),
        2
    );
    assert!(!parsed
        .artists
        .iter()
        .any(|artist| artist.musicbrainz_artist_id.as_deref() == Some("lyricist-artist-a")));
    assert!(!parsed
        .album_artists
        .iter()
        .any(|link| link.artist_id == composer.id));
    assert!(!parsed
        .track_artists
        .iter()
        .any(|link| link.artist_id == composer.id));
}

#[test]
fn recording_linking_the_same_work_twice_produces_one_track_work_link() {
    // A MusicBrainz recording can carry more than one performance relation to
    // the same work (e.g. a full and a partial performance of one work). Each
    // relation would push a track_work link, and two links for the same
    // (track_id, work_id) violate track_works' UNIQUE(track_id, work_id) at
    // finalize. The mapper must emit that pair only once.
    let work = MbWork {
        id: "mb-work-a".to_string(),
        title: "Work Title A".to_string(),
        disambiguation: None,
        work_type: Some("work".to_string()),
        relations: vec![],
    };
    let mut track = make_mb_track("1", "Track Title 1");
    track.recording.as_mut().unwrap().relations = vec![
        MbRelation {
            target_type: Some("work".to_string()),
            relation_type: Some("performance".to_string()),
            work: Some(work.clone()),
            ..MbRelation::default()
        },
        MbRelation {
            target_type: Some("work".to_string()),
            relation_type: Some("performance".to_string()),
            work: Some(work),
            ..MbRelation::default()
        },
    ];
    let response = make_response(vec![MbMedium {
        format: Some("CD".to_string()),
        tracks: vec![track],
    }]);

    let parsed = map(&response, Some(2024), None).unwrap();

    let track_id = &parsed.tracks[0].id;
    let work_id = work_row_id(&parsed, "mb-work-a");
    let links: Vec<_> = parsed
        .work_graph
        .track_works
        .iter()
        .filter(|link| &link.track_id == track_id && link.work_id == work_id)
        .collect();
    assert_eq!(
        links.len(),
        1,
        "duplicate (track_id, work_id) links violate track_works' UNIQUE constraint"
    );
}

#[test]
fn track_level_artist_credit_creates_and_links_a_new_artist() {
    // Track 2 credits a guest distinct from the release artist. The
    // track-artist loop must create that artist once and link it to track 2
    // only; track 1 (no credits) gets no track-artist rows.
    let mut featured = make_mb_track("2", "Track 2 (feat. Guest)");
    featured.artist_credit = vec![credit("artist-guest", "Guest")];
    let response = make_response(vec![MbMedium {
        format: Some("CD".to_string()),
        tracks: vec![make_mb_track("1", "Track 1"), featured],
    }]);

    let parsed = map(&response, Some(2024), None).unwrap();

    let guest = parsed
        .artists
        .iter()
        .find(|a| a.musicbrainz_artist_id.as_deref() == Some("artist-guest"))
        .expect("guest artist created");
    assert_eq!(guest.name, "Guest");

    let track2 = &parsed.tracks[1];
    assert!(parsed
        .track_artists
        .iter()
        .any(|ta| ta.track_id == track2.id && ta.artist_id == guest.id));

    let track1 = &parsed.tracks[0];
    assert!(
        !parsed
            .track_artists
            .iter()
            .any(|ta| ta.track_id == track1.id),
        "a track with no credits gets no track-artist rows"
    );
}

#[test]
fn track_artist_credit_name_is_used_when_artist_payload_name_is_missing() {
    let mut track = make_mb_track("1", "Track 1");
    let mut name_only_credit = credit("track-credit-name-only", "Track Credit Artist Name");
    name_only_credit.artist.as_mut().unwrap().name = None;
    track.artist_credit = vec![name_only_credit];
    let response = make_response(vec![MbMedium {
        format: Some("CD".to_string()),
        tracks: vec![track],
    }]);

    let parsed = map(&response, Some(2024), None).unwrap();

    let artist = parsed
        .artists
        .iter()
        .find(|artist| artist.musicbrainz_artist_id.as_deref() == Some("track-credit-name-only"))
        .expect("track artist imported from credit name");
    assert_eq!(artist.name, "Track Credit Artist Name");
    assert!(parsed.track_artists.iter().any(|track_artist| {
        track_artist.track_id == parsed.tracks[0].id && track_artist.artist_id == artist.id
    }));
}

#[test]
fn track_level_artist_credit_dedupes_against_release_artist_by_mb_id() {
    // The track credits the release artist again by the same MB id; the loop
    // must reuse the existing artist rather than create a duplicate.
    let mut t = make_mb_track("1", "Track 1");
    t.artist_credit = vec![credit("artist-1", "Test Artist")];
    let response = make_response(vec![MbMedium {
        format: Some("CD".to_string()),
        tracks: vec![t],
    }]);

    let parsed = map(&response, Some(2024), None).unwrap();

    let matching: Vec<_> = parsed
        .artists
        .iter()
        .filter(|a| a.musicbrainz_artist_id.as_deref() == Some("artist-1"))
        .collect();
    assert_eq!(
        matching.len(),
        1,
        "release artist must not be duplicated by a track credit"
    );
    assert!(parsed
        .track_artists
        .iter()
        .any(|ta| ta.track_id == parsed.tracks[0].id && ta.artist_id == matching[0].id));
}

#[test]
fn known_mb_artist_id_does_not_merge_into_same_name_artist_without_id() {
    let mut response = make_response(vec![{
        let mut track = make_mb_track("1", "Track 1");
        track.recording.as_mut().unwrap().relations = vec![MbRelation {
            target_type: Some("artist".to_string()),
            relation_type: Some("composer".to_string()),
            artist: Some(MbArtistRef {
                id: Some("composer-artist-a".to_string()),
                name: Some("Artist Name A".to_string()),
                sort_name: None,
            }),
            ..MbRelation::default()
        }];
        MbMedium {
            format: Some("CD".to_string()),
            tracks: vec![track],
        }
    }]);
    response.artist_credit[0].artist = None;

    let parsed = map(&response, Some(2024), None).unwrap();

    let release_artist = parsed
        .artists
        .iter()
        .find(|artist| artist.musicbrainz_artist_id.is_none())
        .expect("release artist without MB id exists");
    let composer = parsed
        .artists
        .iter()
        .find(|artist| artist.musicbrainz_artist_id.as_deref() == Some("composer-artist-a"))
        .expect("known composer artist exists separately");

    assert_ne!(release_artist.id, composer.id);
    assert!(parsed
        .track_artist_roles
        .iter()
        .any(|role| role.artist_id == composer.id));
}

/// An id-less track credit sharing the release artist's name creates a
/// *separate* artist rather than merging into the id-bearing release
/// artist. The MB matcher's `(None, None)` name arm only fires when the
/// existing artist also lacks an MB id, so an id-bearing artist is never a
/// merge target for an id-less credit. (The mirror of
/// `known_mb_artist_id_does_not_merge_into_same_name_artist_without_id`,
/// which withholds the id on the existing side rather than the new side.)
#[test]
fn id_less_track_credit_does_not_merge_into_id_bearing_release_artist() {
    // Release artist "Artist Name A" carries MB id "artist-1".
    let mut track = make_mb_track("1", "Track 1");
    track.artist_credit = vec![MbArtistCredit {
        name: "Artist Name A".to_string(),
        artist: Some(MbArtistRef {
            id: None,
            name: Some("Artist Name A".to_string()),
            sort_name: None,
        }),
    }];
    let response = make_response(vec![MbMedium {
        format: Some("CD".to_string()),
        tracks: vec![track],
    }]);

    let parsed = map(&response, Some(2024), None).unwrap();

    let matching: Vec<_> = parsed
        .artists
        .iter()
        .filter(|a| a.name == "Artist Name A")
        .collect();
    assert_eq!(
        matching.len(),
        2,
        "an id-less credit must not merge into the id-bearing release artist"
    );
    assert!(matching
        .iter()
        .any(|a| a.musicbrainz_artist_id.as_deref() == Some("artist-1")));
    assert!(matching.iter().any(|a| a.musicbrainz_artist_id.is_none()));
}

#[test]
fn known_mb_artist_ids_keep_same_name_artists_separate() {
    let mut track = make_mb_track("1", "Track 1");
    track.artist_credit = vec![credit("artist-1", "Artist Name A")];
    track.recording.as_mut().unwrap().relations = vec![MbRelation {
        target_type: Some("artist".to_string()),
        relation_type: Some("composer".to_string()),
        artist: Some(MbArtistRef {
            id: Some("artist-2".to_string()),
            name: Some("Artist Name A".to_string()),
            sort_name: None,
        }),
        ..MbRelation::default()
    }];
    let response = make_response(vec![MbMedium {
        format: Some("CD".to_string()),
        tracks: vec![track],
    }]);

    let parsed = map(&response, Some(2024), None).unwrap();

    let matching: Vec<_> = parsed
        .artists
        .iter()
        .filter(|artist| artist.name == "Artist Name A")
        .collect();
    assert_eq!(matching.len(), 2);
    assert!(matching
        .iter()
        .any(|artist| artist.musicbrainz_artist_id.as_deref() == Some("artist-1")));
    assert!(matching
        .iter()
        .any(|artist| artist.musicbrainz_artist_id.as_deref() == Some("artist-2")));
}

/// A "parts" work relation with direction "backward" names the current
/// work's *parent*, not a child: the related work becomes the parent in
/// work_parts (the forward direction is covered by the composer-work-graph
/// test above).
#[test]
fn backward_work_parts_relation_treats_related_work_as_parent() {
    let parent_work = MbWork {
        id: "mb-work-parent-a".to_string(),
        title: "Parent Work".to_string(),
        disambiguation: None,
        work_type: Some("work".to_string()),
        relations: vec![],
    };
    let child_work = MbWork {
        id: "mb-work-child-a".to_string(),
        title: "Child Work".to_string(),
        disambiguation: None,
        work_type: Some("part".to_string()),
        relations: vec![MbRelation {
            target_type: Some("work".to_string()),
            relation_type: Some("parts".to_string()),
            direction: Some("backward".to_string()),
            work: Some(parent_work),
            ..MbRelation::default()
        }],
    };
    let mut track = make_mb_track("1", "Track 1");
    track.recording.as_mut().unwrap().relations = vec![MbRelation {
        target_type: Some("work".to_string()),
        relation_type: Some("performance".to_string()),
        work: Some(child_work),
        ..MbRelation::default()
    }];
    let response = make_response(vec![MbMedium {
        format: Some("CD".to_string()),
        tracks: vec![track],
    }]);

    let parsed = map(&response, Some(2024), None).unwrap();
    let parent_id = work_row_id(&parsed, "mb-work-parent-a");
    let child_id = work_row_id(&parsed, "mb-work-child-a");
    assert!(
        parsed
            .work_graph
            .work_parts
            .iter()
            .any(|p| { p.parent_work_id == parent_id && p.child_work_id == child_id }),
        "backward relation should make the related work the parent"
    );
}

/// A release artist takes its discogs_artist_id from the cross-referenced
/// Discogs release, matched on name case-insensitively.
#[test]
fn release_artist_gets_discogs_id_by_case_insensitive_name() {
    let response = make_response(vec![MbMedium {
        format: Some("CD".to_string()),
        tracks: vec![make_mb_track("1", "Track 1")],
    }]);
    let mut discogs_release = discogs_release_with_master(None);
    discogs_release.artists = vec![crate::discogs::DiscogsArtist {
        id: "d-artist-7".to_string(),
        name: "ARTIST NAME A".to_string(),
    }];

    let parsed = map(&response, None, Some(discogs_release)).unwrap();

    let release_artist = parsed
        .artists
        .iter()
        .find(|a| a.musicbrainz_artist_id.as_deref() == Some("artist-1"))
        .expect("release artist mapped");
    assert_eq!(
        release_artist.discogs_artist_id.as_deref(),
        Some("d-artist-7")
    );
}

/// No Discogs artist name matches the release artist -> no cross-ref id.
#[test]
fn release_artist_discogs_id_is_none_when_no_name_matches() {
    let response = make_response(vec![MbMedium {
        format: Some("CD".to_string()),
        tracks: vec![make_mb_track("1", "Track 1")],
    }]);
    let mut discogs_release = discogs_release_with_master(None);
    discogs_release.artists = vec![crate::discogs::DiscogsArtist {
        id: "d-artist-7".to_string(),
        name: "Different Artist".to_string(),
    }];

    let parsed = map(&response, None, Some(discogs_release)).unwrap();

    let release_artist = parsed
        .artists
        .iter()
        .find(|a| a.musicbrainz_artist_id.as_deref() == Some("artist-1"))
        .expect("release artist mapped");
    assert_eq!(release_artist.discogs_artist_id, None);
}

/// A work composer relation with no artist payload is skipped with a
/// warning, not silently dropped; the work itself still imports.
#[test]
fn work_composer_relation_without_artist_payload_is_logged_and_skipped() {
    let work = MbWork {
        id: "mb-work-a".to_string(),
        title: "Work Title".to_string(),
        disambiguation: None,
        work_type: Some("work".to_string()),
        relations: vec![MbRelation {
            target_type: Some("artist".to_string()),
            relation_type: Some("composer".to_string()),
            artist: None,
            ..MbRelation::default()
        }],
    };
    let mut track = make_mb_track("1", "Track 1");
    track.recording.as_mut().unwrap().relations = vec![MbRelation {
        target_type: Some("work".to_string()),
        relation_type: Some("performance".to_string()),
        work: Some(work),
        ..MbRelation::default()
    }];
    let response = make_response(vec![MbMedium {
        format: Some("CD".to_string()),
        tracks: vec![track],
    }]);

    let mut parsed = None;
    let logs = crate::test_logs::capture_warn_logs(|| {
        parsed = Some(map(&response, Some(2024), None).unwrap());
    });
    let parsed = parsed.unwrap();

    assert_eq!(
        parsed.work_graph.works[0].musicbrainz_work_id, "mb-work-a",
        "the work row lands; only its malformed composer relation is dropped",
    );
    assert!(parsed.work_graph.work_artists.is_empty());
    assert!(
        logs.contains("work artist relation without artist payload"),
        "expected a skip warning, got: {logs}"
    );
}

/// A work reached from two tracks is converted once per release: its
/// malformed composer relation is logged a single time, and its sub-graph is
/// walked once, no matter how many tracks reference it. (Locks `mb_work_ref`'s
/// per-release memoization.)
#[test]
fn work_referenced_by_two_tracks_logs_skip_once() {
    let work = MbWork {
        id: "mb-work-a".to_string(),
        title: "Work Title".to_string(),
        disambiguation: None,
        work_type: Some("work".to_string()),
        relations: vec![MbRelation {
            target_type: Some("artist".to_string()),
            relation_type: Some("composer".to_string()),
            artist: None,
            ..MbRelation::default()
        }],
    };
    let performance = |work: MbWork| MbRelation {
        target_type: Some("work".to_string()),
        relation_type: Some("performance".to_string()),
        work: Some(work),
        ..MbRelation::default()
    };
    let mut track_one = make_mb_track("1", "Track 1");
    track_one.recording.as_mut().unwrap().relations = vec![performance(work.clone())];
    let mut track_two = make_mb_track("2", "Track 2");
    track_two.recording.as_mut().unwrap().relations = vec![performance(work)];
    let response = make_response(vec![MbMedium {
        format: Some("CD".to_string()),
        tracks: vec![track_one, track_two],
    }]);

    let mut parsed = None;
    let logs = crate::test_logs::capture_warn_logs(|| {
        parsed = Some(map(&response, Some(2024), None).unwrap());
    });
    let parsed = parsed.unwrap();

    // The work row lands once and both tracks link it.
    let work_id = work_row_id(&parsed, "mb-work-a");
    assert_eq!(
        parsed
            .work_graph
            .track_works
            .iter()
            .filter(|l| l.work_id == work_id)
            .count(),
        2
    );
    // The skip line is logged exactly once, not once per referencing track.
    assert_eq!(
        logs.matches("work artist relation without artist payload")
            .count(),
        1,
        "expected the work's skip to log once per release, got: {logs}"
    );
}

/// A work "parts" relation with no work payload is skipped with a warning;
/// no work-part link is produced.
#[test]
fn work_parts_relation_without_work_payload_is_logged_and_skipped() {
    let work = MbWork {
        id: "mb-work-a".to_string(),
        title: "Work Title".to_string(),
        disambiguation: None,
        work_type: Some("work".to_string()),
        relations: vec![MbRelation {
            target_type: Some("work".to_string()),
            relation_type: Some("parts".to_string()),
            direction: Some("forward".to_string()),
            work: None,
            ..MbRelation::default()
        }],
    };
    let mut track = make_mb_track("1", "Track 1");
    track.recording.as_mut().unwrap().relations = vec![MbRelation {
        target_type: Some("work".to_string()),
        relation_type: Some("performance".to_string()),
        work: Some(work),
        ..MbRelation::default()
    }];
    let response = make_response(vec![MbMedium {
        format: Some("CD".to_string()),
        tracks: vec![track],
    }]);

    let mut parsed = None;
    let logs = crate::test_logs::capture_warn_logs(|| {
        parsed = Some(map(&response, Some(2024), None).unwrap());
    });
    let parsed = parsed.unwrap();

    assert!(parsed.work_graph.work_parts.is_empty());
    assert!(
        logs.contains("work parts relation without work payload"),
        "expected a skip warning, got: {logs}"
    );
}

/// A track artist credit whose name resolves to nothing (empty credit and
/// no artist-payload name) is skipped with a warning; the track still maps
/// and no track-artist row is produced.
#[test]
fn track_artist_credit_without_resolvable_name_is_logged_and_skipped() {
    let mut track = make_mb_track("1", "Track 1");
    track.artist_credit = vec![MbArtistCredit {
        name: String::new(),
        artist: Some(MbArtistRef {
            id: Some("artist-nameless".to_string()),
            name: None,
            sort_name: None,
        }),
    }];
    let response = make_response(vec![MbMedium {
        format: Some("CD".to_string()),
        tracks: vec![track],
    }]);

    let mut parsed = None;
    let logs = crate::test_logs::capture_warn_logs(|| {
        parsed = Some(map(&response, Some(2024), None).unwrap());
    });
    let parsed = parsed.unwrap();

    assert_eq!(parsed.tracks.len(), 1);
    assert!(parsed.track_artists.is_empty());
    assert!(
        logs.contains("unresolvable artist name"),
        "expected a skip warning, got: {logs}"
    );
}
