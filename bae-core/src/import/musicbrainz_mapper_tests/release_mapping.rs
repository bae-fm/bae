use super::*;
use crate::musicbrainz::{
    MbArtistCredit, MbArtistRef, MbMedium, MbRecording, MbReleaseResponse, MbTrack, MbWork,
};
use crate::import::MetadataRef;

/// Read archived source documents through the same metadata and reading path
/// as import.
fn map(
    response: &MbReleaseResponse,
    supporting: Vec<crate::import::SourcePayload>,
) -> Result<ReleaseValues, ImportError> {
    let payloads: crate::import::payloads::ReleasePayloads =
        crate::import::payloads::ReleasePayloads::for_test(
        MetadataRef::new(Catalog::MusicBrainz, &response.id),
        serde_json::to_string(response).expect("MusicBrainz fixture serializes"),
        supporting,
    );
    payloads.extract().unwrap().every_medium_values()
}

use crate::import::release_values::committed_rows as rows;

/// The id of the `works` row the release's rows hold for a MusicBrainz work.
fn work_row_id(parsed: &crate::import::ReleaseRows, musicbrainz_work_id: &str) -> String {
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
        disambiguation: None,
        annotation: None,
        date: Some("2024".to_string()),
        country: None,
        status: None,
        packaging: None,
        barcode: None,
        artist_credit: vec![MbArtistCredit { joinphrase: None, 
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
) -> Result<ReleaseValues, ImportError> {
    let payloads: crate::import::payloads::ReleasePayloads =
        crate::import::payloads::ReleasePayloads::for_test(
        MetadataRef::new(Catalog::MusicBrainz, &response.id),
        serde_json::to_string(response).expect("MusicBrainz fixture serializes"),
        Vec::new(),
    );
    payloads
        .extract()
        .unwrap()
        .read(&crate::import::catalog_reading::ReadAudio::of_lengths(audio_durations_ms))
        .map_err(ImportError::from)
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

    assert_eq!(crate::import::release_values::value_labels(&parsed), ["1", "2", "3"], "one layer is one disc");
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
    assert_eq!(
        crate::import::release_values::value_labels(&parsed),
        ["1", "2"],
        "the disc read is the reading's only medium"
    );
}

/// Audio no set of mediums adds up to is refused, naming the folder's count
/// and every track the release lists.
#[test]
fn audio_no_mediums_hold_is_refused_with_both_counts() {
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

    let refused = map_for_audio(&response, &[100_000, 100_000, 100_000])
        .expect_err("no set of mediums holds three tracks");

    assert!(
        matches!(
            refused,
            ImportError::MetadataTrackCount {
                folder_tracks: 3,
                release_tracks: 2
            }
        ),
        "{refused}"
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
    assert_eq!(crate::import::release_values::value_labels(&parsed), ["1-1", "1-2", "2-1", "2-2"]);
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
    assert_eq!(parsed.mediums.len(), 1);
    assert_eq!(parsed.mediums[0].sides.get(), 1, "positions that name no letter are one side");
    assert_eq!(
        parsed.pressing.facts.media,
        crate::pressing::made_of(crate::pressing::Medium::Vinyl, 1).media
    );
    assert_eq!(
        crate::import::release_values::value_labels(&parsed),
        (1..=12).map(|number| format!("A{number}")).collect::<Vec<_>>()
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
    assert_eq!(crate::import::release_values::value_labels(&parsed), ["A1", "A2", "B1", "B2"]);
}

/// 2LP vinyl: two media, each with two sides of its own (A/B and C/D),
/// lettered on from the first record's.
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
    assert_eq!(crate::import::release_values::value_labels(&parsed), ["A1", "A2", "B1", "B2", "C1", "C2", "D1", "D2"]);
    assert!(
        parsed.mediums.iter().all(|medium| medium.sides.get() == 2),
        "each record has two sides of its own"
    );
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
    assert_eq!(crate::import::release_values::value_labels(&parsed), ["1", "2", "3"]);
}

/// A track whose position names no side is on the side of the track before
/// it, numbered by its place there.
#[test]
fn vinyl_track_without_a_position_is_on_the_side_before_it() {
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
    assert_eq!(crate::import::release_values::value_labels(&parsed), ["A1", "A2"]);
}

/// A printed number names no side: the track is on the side before it, with
/// the number it prints.
#[test]
fn vinyl_numeric_track_is_on_the_side_before_it() {
    let response = make_response(vec![MbMedium {
        discs: vec![],
        format: Some("12\" Vinyl".to_string()),
        tracks: vec![
            make_mb_track("A1", "Track A1"),
            make_mb_track("1", "Numeric-only Track"),
        ],
    }]);

    let parsed = map(&response, vec![]).unwrap();
    assert_eq!(crate::import::release_values::value_labels(&parsed), ["A1", "A1"]);
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
/// group's), every label with its catalog number, the barcode, and what it is.
#[test]
fn pressing_reads_year_every_label_barcode_and_every_fact() {
    let mut response = make_response(vec![
        MbMedium {
            discs: vec![],
            format: Some("12\" Vinyl".to_string()),
            tracks: vec![make_mb_track("A1", "Track A1")],
        },
        MbMedium {
            discs: vec![],
            format: Some("12\" Vinyl".to_string()),
            tracks: vec![make_mb_track("C1", "Track C1")],
        },
        MbMedium {
            discs: vec![],
            format: Some("CD".to_string()),
            tracks: vec![make_mb_track("1", "Track 1")],
        },
    ]);
    response.date = Some("1971-03-01".to_string());
    response.country = Some("GB".to_string());
    response.status = Some("Official".to_string());
    response.packaging = Some("Gatefold Cover".to_string());
    response.barcode = Some("012345678905".to_string());
    response.label_info = vec![
        crate::musicbrainz::MbLabelInfo {
            label: Some(crate::musicbrainz::MbLabel {
                name: Some("Label A".to_string()),
            }),
            catalog_number: Some("AB 100".to_string()),
        },
        crate::musicbrainz::MbLabelInfo {
            label: Some(crate::musicbrainz::MbLabel {
                name: Some("Label B".to_string()),
            }),
            catalog_number: Some("CL 719".to_string()),
        },
    ];
    // The release group's first release predates this pressing; the pressing
    // year is the release's own date, and only the album year follows the group.
    response.release_group.as_mut().unwrap().first_release_date = Some("1969".to_string());

    let (pressing, media) = pressing(&response);

    assert_eq!(
        pressing,
        Pressing {
            year: Some(1971),
            labels: vec![
                crate::pressing::ReleaseLabel::of(Some("Label A"), Some("AB 100")),
                crate::pressing::ReleaseLabel::of(Some("Label B"), Some("CL 719")),
            ],
            barcode: Some("012345678905".to_string()),
            facts: crate::pressing::PressingFacts {
                area: Some(crate::pressing::area("GB")),
                media: vec![
                    crate::pressing::MediaCount {
                        medium: Medium::Vinyl,
                        count: 2,
                    },
                    crate::pressing::MediaCount {
                        medium: Medium::Cd,
                        count: 1,
                    },
                ],
                status: Some(crate::pressing::ReleaseStatus::Official),
                packaging: Some(crate::pressing::Packaging::GatefoldCover),
                discogs_details: Vec::new(),
            },
        }
    );
    assert_eq!(
        media,
        crate::pressing::StatedMedia::PerMedium(vec![
            Some(Medium::Vinyl),
            Some(Medium::Vinyl),
            Some(Medium::Cd)
        ])
    );

    // What the mapper commits is what the projection says.
    let parsed = map(&response, vec![]).unwrap();
    assert_eq!(parsed.pressing, pressing);
    assert_eq!(parsed.album_year, Some(1969));
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
    MbArtistCredit { joinphrase: None, 
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

    let artist = &parsed.album_credits[0];
    assert_eq!(
        artist.credit.musicbrainz_artist_id.as_deref(),
        Some("artist-credit-name-only")
    );
    assert_eq!(artist.credit.name, "Credit Artist Name");
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
        .album_credits
        .iter()
        .find(|artist| artist.credit.musicbrainz_artist_id.as_deref() == Some("artist-1"))
        .expect("release artist read");
    let composer = &parsed.tracks[0].roles[0].artist;
    assert_eq!(
        composer.musicbrainz_artist_id.as_deref(),
        Some("composer-artist-a")
    );

    assert_eq!(release_artist.credit.sort_name, None);
    assert_eq!(composer.sort_name, None);
}
