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
            discs: vec![],
            format: Some("CD".to_string()),
            tracks: vec![make_mb_track("1", "Track 1"), make_mb_track("2", "Track 2")],
        },
        MbMedium {
            discs: vec![],
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
        discs: vec![],
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
            discs: vec![],
            format: Some("12\" Vinyl".to_string()),
            tracks: vec![
                make_mb_track("A1", "Track A1"),
                make_mb_track("A2", "Track A2"),
                make_mb_track("B1", "Track B1"),
                make_mb_track("B2", "Track B2"),
            ],
        },
        MbMedium {
            discs: vec![],
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
        discs: vec![],
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
        discs: vec![],
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
        discs: vec![],
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
        discs: vec![],
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
        discs: vec![],
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
        discs: vec![],
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
        discs: vec![],
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
        covers: vec![],
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
        discs: vec![],
        format: Some("CD".to_string()),
        tracks: vec![make_mb_track("1", "Track 1")],
    }]);

    let parsed = map(&response, None, None).unwrap();

    assert_eq!(parsed.identities.len(), 1);
    let mb = &parsed.identities[0];
    assert_eq!(mb.source, MetadataSource::MusicBrainz);
    assert_eq!(mb.source_group_id, "rg-test");
    assert_eq!(mb.source_release_id, "test-release");
}

#[test]
fn release_with_no_artist_credits_returns_err() {
    let mut response = make_response(vec![MbMedium {
        discs: vec![],
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
        discs: vec![],
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
fn test_map_mb_cross_ref_no_master_id_yields_discogs_release_as_its_own_group() {
    // Cross-ref hit and the linked Discogs release has no master — it is its
    // own group, so the Discogs row is still emitted.
    let response = make_response(vec![MbMedium {
        discs: vec![],
        format: Some("CD".to_string()),
        tracks: vec![make_mb_track("1", "Track 1")],
    }]);
    let discogs_release = discogs_release_with_master(None);

    let parsed = map(&response, None, Some(discogs_release)).unwrap();

    assert_eq!(parsed.identities.len(), 2);
    assert_eq!(parsed.identities[0].source, MetadataSource::MusicBrainz);

    let discogs = &parsed.identities[1];
    assert_eq!(discogs.source, MetadataSource::Discogs);
    assert_eq!(discogs.source_group_id, "d-rel-99");
    assert_eq!(discogs.source_release_id, "d-rel-99");
}

#[test]
fn test_map_mb_cross_ref_with_master_id_yields_two_identity_rows() {
    // Cross-ref hit AND the linked Discogs release carries a master_id
    // — two rows: MB + Discogs. Both Exact (release IDs present).
    let response = make_response(vec![MbMedium {
        discs: vec![],
        format: Some("CD".to_string()),
        tracks: vec![make_mb_track("1", "Track 1")],
    }]);
    let discogs_release = discogs_release_with_master(Some("d-master-123".to_string()));

    let parsed = map(&response, None, Some(discogs_release)).unwrap();

    assert_eq!(parsed.identities.len(), 2);

    let mb = &parsed.identities[0];
    assert_eq!(mb.source, MetadataSource::MusicBrainz);
    assert_eq!(mb.source_group_id, "rg-test");
    assert_eq!(mb.source_release_id, "test-release");

    let discogs = &parsed.identities[1];
    assert_eq!(discogs.source, MetadataSource::Discogs);
    assert_eq!(discogs.source_group_id, "d-master-123");
    assert_eq!(discogs.source_release_id, "d-rel-99");
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
        discs: vec![],
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
            discs: vec![],
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
