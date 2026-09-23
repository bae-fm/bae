use super::*;
use crate::musicbrainz::{
    MbArtistCredit, MbArtistRef, MbMedium, MbRecording, MbReleaseResponse, MbTrack, MbWork,
};
use coven::FixedClock;
use coven::SequentialIdProvider;

/// Project archived source documents through the same metadata and mapping
/// path as import, with only clock and ID generation replaced.
fn map(
    response: &MbReleaseResponse,
    supporting: Vec<crate::import::SourcePayload>,
) -> Result<ParsedAlbum, ImportError> {
    let payloads: crate::import::payloads::ReleasePayloads =
        serde_json::from_value(serde_json::json!({
            "release": MetadataRef::new(Catalog::MusicBrainz, &response.id),
            "anchor": serde_json::to_string(response).expect("MusicBrainz fixture serializes"),
            "supporting": supporting,
        }))
        .expect("archived fixture deserializes");
    let clock = FixedClock(
        chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc),
    );
    let ids = SequentialIdProvider::new("mb");
    payloads.parsed(&[], &clock, &ids)
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

/// [`map`] with a folder to fit: the measured lengths choose which mediums
/// the draft is read from.
fn map_for_audio(
    response: &MbReleaseResponse,
    audio_durations_ms: &[u64],
) -> Result<ParsedAlbum, ImportError> {
    let payloads: crate::import::payloads::ReleasePayloads =
        serde_json::from_value(serde_json::json!({
            "release": MetadataRef::new(Catalog::MusicBrainz, &response.id),
            "anchor": serde_json::to_string(response).expect("MusicBrainz fixture serializes"),
            "supporting": [],
        }))
        .expect("archived fixture deserializes");
    let clock = FixedClock(
        chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc),
    );
    let ids = SequentialIdProvider::new("mb");
    payloads.parsed(audio_durations_ms, &clock, &ids)
}

fn timed_mb_track(number: &str, title: &str, length_ms: u64) -> MbTrack {
    MbTrack {
        length: Some(length_ms),
        ..make_mb_track(number, title)
    }
}

/// A hybrid SACD is one disc MusicBrainz lists as two mediums, a CD layer
/// and an SACD layer of the same tracks. A rip of it is the CD layer: the
/// draft is that medium's tracks, all on side 1, not both layers' twelve.
#[test]
fn a_hybrid_sacd_rip_is_read_from_its_cd_layer() {
    let layer = |format: &str| MbMedium {
        discs: vec![],
        format: Some(format.to_string()),
        tracks: vec![
            timed_mb_track("1", "Track 1", 527_000),
            timed_mb_track("2", "Track 2", 284_000),
            timed_mb_track("3", "Track 3", 333_000),
        ],
    };
    let response = make_response(vec![
        layer("Hybrid SACD (CD layer)"),
        layer("Hybrid SACD (SACD layer, 2 channels)"),
    ]);

    let parsed = map_for_audio(&response, &[527_200, 284_000, 332_900]).unwrap();

    assert_eq!(parsed.tracks.len(), 3);
    assert!(parsed.tracks.iter().all(|track| track.side == Some(1)));
    assert_eq!(
        parsed
            .tracks
            .iter()
            .map(|track| track.track_number)
            .collect::<Vec<_>>(),
        vec![Some(1), Some(2), Some(3)]
    );
}

/// One disc of a box is read from the medium its lengths match, and the
/// draft's sides start at 1 there rather than at that disc's position.
#[test]
fn one_disc_of_a_box_is_read_from_its_own_medium() {
    let disc = |titles: [&str; 2], lengths: [u64; 2]| MbMedium {
        discs: vec![],
        format: Some("CD".to_string()),
        tracks: vec![
            timed_mb_track("1", titles[0], lengths[0]),
            timed_mb_track("2", titles[1], lengths[1]),
        ],
    };
    let response = make_response(vec![
        disc(["Track 1", "Track 2"], [300_000, 300_000]),
        disc(["Track 3", "Track 4"], [200_000, 400_000]),
        disc(["Track 5", "Track 6"], [250_000, 350_000]),
    ]);

    let parsed = map_for_audio(&response, &[250_100, 349_800]).unwrap();

    assert_eq!(
        parsed
            .tracks
            .iter()
            .map(|track| track.title.as_str())
            .collect::<Vec<_>>(),
        vec!["Track 5", "Track 6"]
    );
    assert!(parsed.tracks.iter().all(|track| track.side == Some(1)));
}

/// Audio no set of mediums adds up to is read as the whole release. The
/// refusal stays where it always was, when the metadata is applied to a
/// draft, where both counts are named.
#[test]
fn audio_no_mediums_hold_is_read_as_the_whole_release() {
    let response = make_response(vec![
        MbMedium {
            discs: vec![],
            format: Some("CD".to_string()),
            tracks: vec![timed_mb_track("1", "Track 1", 300_000)],
        },
        MbMedium {
            discs: vec![],
            format: Some("CD".to_string()),
            tracks: vec![timed_mb_track("1", "Track 2", 300_000)],
        },
    ]);

    let parsed = map_for_audio(&response, &[100_000, 100_000, 100_000]).unwrap();

    assert_eq!(
        parsed
            .tracks
            .iter()
            .map(|track| track.title.as_str())
            .collect::<Vec<_>>(),
        vec!["Track 1", "Track 2"]
    );
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

    let parsed = map(&response, vec![]).unwrap();
    let tracks = &parsed.tracks;

    assert_eq!(tracks.len(), 4);

    // Medium 1 = side 1
    assert_eq!(tracks[0].side, Some(1));
    assert_eq!(tracks[0].track_number, Some(1));
    assert_eq!(tracks[1].side, Some(1));
    assert_eq!(tracks[1].track_number, Some(2));

    // Medium 2 = side 2
    assert_eq!(tracks[2].side, Some(2));
    assert_eq!(tracks[2].track_number, Some(1));
    assert_eq!(tracks[3].side, Some(2));
    assert_eq!(tracks[3].track_number, Some(2));
}

#[test]
fn numeric_vinyl_tracks_are_usable_without_side_boundaries() {
    let response = make_response(vec![MbMedium {
        discs: vec![],
        format: Some("12\" Vinyl".to_string()),
        tracks: (1..=12)
            .map(|number| make_mb_track(&number.to_string(), &format!("Track {number}")))
            .collect(),
    }]);
    let parsed =
        map(&response, vec![]).expect("absent side information does not invalidate a release");
    assert_eq!(parsed.tracks.len(), 12);
    assert!(parsed.tracks.iter().all(|track| track.side.is_none()));
    assert_eq!(
        parsed.release.pressing.format.as_deref(),
        Some("12\" Vinyl")
    );
    assert_eq!(
        parsed
            .tracks
            .iter()
            .map(|track| track.track_number)
            .collect::<Vec<_>>(),
        (1..=12).map(Some).collect::<Vec<_>>()
    );
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

    let parsed = map(&response, vec![]).unwrap();
    let tracks = &parsed.tracks;

    assert_eq!(tracks.len(), 4);

    // A tracks = side 1
    assert_eq!(tracks[0].side, Some(1));
    assert_eq!(tracks[0].track_number, Some(1));
    assert_eq!(tracks[1].side, Some(1));
    assert_eq!(tracks[1].track_number, Some(2));

    // B tracks = side 2
    assert_eq!(tracks[2].side, Some(2));
    assert_eq!(tracks[2].track_number, Some(1));
    assert_eq!(tracks[3].side, Some(2));
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

    let parsed = map(&response, vec![]).unwrap();
    let tracks = &parsed.tracks;

    assert_eq!(tracks.len(), 8);

    // Medium 1: A = side 1, B = side 2
    assert_eq!(tracks[0].side, Some(1));
    assert_eq!(tracks[1].side, Some(1));
    assert_eq!(tracks[2].side, Some(2));
    assert_eq!(tracks[3].side, Some(2));

    // Medium 2: C = side 3, D = side 4
    assert_eq!(tracks[4].side, Some(3));
    assert_eq!(tracks[5].side, Some(3));
    assert_eq!(tracks[6].side, Some(4));
    assert_eq!(tracks[7].side, Some(4));
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

    let parsed = map(&response, vec![]).unwrap();
    let tracks = &parsed.tracks;

    assert_eq!(tracks.len(), 3);

    // All tracks on side 1
    assert_eq!(tracks[0].side, Some(1));
    assert_eq!(tracks[0].track_number, Some(1));
    assert_eq!(tracks[1].side, Some(1));
    assert_eq!(tracks[1].track_number, Some(2));
    assert_eq!(tracks[2].side, Some(1));
    assert_eq!(tracks[2].track_number, Some(3));
}

/// Known and unknown side assignments coexist without guessed boundaries.
#[test]
fn vinyl_track_without_a_number_keeps_its_side_unknown() {
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

    let parsed = map(&response, vec![]).unwrap();
    assert_eq!(parsed.tracks[0].side, Some(1));
    assert_eq!(parsed.tracks[1].side, None);
}

/// A printed number does not establish a vinyl side.
#[test]
fn vinyl_numeric_track_keeps_its_side_unknown() {
    let response = make_response(vec![MbMedium {
        discs: vec![],
        format: Some("12\" Vinyl".to_string()),
        tracks: vec![
            make_mb_track("A1", "Track A1"),
            make_mb_track("1", "Numeric-only Track"),
        ],
    }]);

    let parsed = map(&response, vec![]).unwrap();
    assert_eq!(parsed.tracks[0].side, Some(1));
    assert_eq!(parsed.tracks[1].side, None);
}

#[test]
fn medium_with_no_tracks_returns_err() {
    let response = make_response(vec![MbMedium {
        discs: vec![],
        format: Some("CD".to_string()),
        tracks: vec![],
    }]);

    let result = map(&response, vec![]);
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

    let parsed = map(&response, vec![]).unwrap();

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

    let err = map(&response, vec![])
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
    let parsed = map(&response, vec![]).unwrap();
    assert_eq!(parsed.release.pressing, pressing);
    assert_eq!(parsed.album.year, Some(1969));
}

fn discogs_artist_document(name: &str) -> crate::import::SourcePayload {
    crate::import::SourcePayload::new(
        crate::import::PayloadSource::Discogs,
        "99".to_string(),
        serde_json::json!({
            "id": 99,
            "title": "Album Title A",
            "artists": [{"id": 7, "name": name}],
        })
        .to_string(),
    )
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
        map(&response, vec![]).expect_err("expected missing artist credits to return an error");

    assert!(
        matches!(&err, ImportError::SourceData { detail, .. } if detail.contains("has no album artist")),
        "unexpected error message: {err}"
    );
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

    let parsed = map(&response, vec![]).unwrap();

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

    let parsed = map(&response, vec![]).unwrap();

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
