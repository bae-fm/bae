use super::*;
use crate::discogs::client::DiscogsSearchResult;
use crate::musicbrainz::{
    MbArtistCredit, MbMedium, MbRecording, MbReleaseGroupRef, MbReleaseResponse, MbTrack,
};
use coven::{FixedClock, SequentialIdProvider};

fn test_clock() -> FixedClock {
    FixedClock(
        chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc),
    )
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

fn response_with_media(media: Vec<MbMedium>) -> MbReleaseResponse {
    MbReleaseResponse {
        id: "mb-release-1".to_string(),
        title: "Album Title".to_string(),
        date: None,
        country: None,
        barcode: None,
        artist_credit: vec![MbArtistCredit {
            name: "Artist Name".to_string(),
            artist: None,
        }],
        release_group: Some(MbReleaseGroupRef {
            id: "mb-group-1".to_string(),
            first_release_date: None,
            relations: None,
        }),
        label_info: vec![],
        media,
        relations: vec![],
        cover_art_archive: crate::musicbrainz::MbCoverArtArchive {
            front: true,
            darkened: false,
        },
    }
}

fn result_with_title(title: &str) -> DiscogsSearchResult {
    DiscogsSearchResult {
        id: 1,
        title: title.to_string(),
        year: None,
        format: None,
        country: None,
        label: None,
        catno: None,
        cover_image: None,
        thumb: None,
        master_id: None,
        result_type: "release".to_string(),
    }
}

#[test]
fn discogs_title_splits_into_artist_and_album() {
    let m = discogs_search_result_to_metadata(result_with_title("Artist Name - Album Title"));
    assert_eq!(m.artist.as_deref(), Some("Artist Name"));
    assert_eq!(m.title, "Album Title");
}

#[test]
fn discogs_title_without_separator_is_all_album() {
    let m = discogs_search_result_to_metadata(result_with_title("Just A Title"));
    assert_eq!(m.artist, None);
    assert_eq!(m.title, "Just A Title");
}

#[test]
fn discogs_title_with_empty_artist_drops_to_none() {
    let m = discogs_search_result_to_metadata(result_with_title(" - Album Title"));
    assert_eq!(m.artist, None);
    assert_eq!(m.title, "Album Title");
}

#[test]
fn discogs_search_result_carries_remote_cover_pair() {
    let mut result = result_with_title("Artist Name - Album Title");
    result.cover_image = Some("https://discogs.example/full.jpg".to_string());
    result.thumb = Some("https://discogs.example/thumb.jpg".to_string());

    let metadata = discogs_search_result_to_metadata(result);

    assert_eq!(
        metadata.cover_art,
        Some(RemoteCover {
            url: "https://discogs.example/full.jpg".to_string(),
            thumbnail_url: "https://discogs.example/thumb.jpg".to_string(),
            label: MetadataSource::Discogs.cover_source_label().to_string(),
            source: MetadataSource::Discogs,
        })
    );
}

#[test]
fn mb_detail_uses_supplied_cover_art_archive_candidates() {
    let response = response_with_media(vec![MbMedium {
        format: Some("CD".to_string()),
        tracks: vec![make_mb_track("1", "Track Title")],
    }]);
    let cover_art = vec![RemoteCover {
        url: "https://caa.example/cover.jpg".to_string(),
        thumbnail_url: "https://caa.example/thumb.jpg".to_string(),
        label: MetadataSource::MusicBrainz.cover_source_label().to_string(),
        source: MetadataSource::MusicBrainz,
    }];

    let detail = build_mb_detail("mb-release-1", &response, cover_art.clone()).unwrap();

    assert_eq!(detail.cover_art, cover_art);
}

/// Vinyl side numbering runs continuously across media: medium 1 (A/B) is
/// sides 1-2, medium 2 (C/D) is sides 3-4. Same shared assignment the DB
/// mapper uses.
#[test]
fn mb_detail_numbers_vinyl_sides_across_media() {
    let response = response_with_media(vec![
        MbMedium {
            format: Some("12\" Vinyl".to_string()),
            tracks: vec![
                make_mb_track("A1", "Track A1"),
                make_mb_track("B1", "Track B1"),
            ],
        },
        MbMedium {
            format: Some("12\" Vinyl".to_string()),
            tracks: vec![
                make_mb_track("C1", "Track C1"),
                make_mb_track("D1", "Track D1"),
            ],
        },
    ]);

    let detail = build_mb_detail("mb-release-1", &response, vec![]).unwrap();
    let sides: Vec<u32> = detail.tracks.iter().map(|t| t.side).collect();
    assert_eq!(sides, vec![1, 2, 3, 4]);
}

#[test]
fn parse_duration_to_ms_handles_mm_ss_and_hh_mm_ss() {
    // (input, expected)
    let ok: &[(&str, u64)] = &[
        ("0:00", 0),
        ("3:45", 225_000),
        ("59:59", 3_599_000),
        ("1:02:03", 3_723_000),
        ("0:00:30", 30_000),
    ];
    for (input, expected) in ok {
        assert_eq!(parse_duration_to_ms(input), Some(*expected), "{input}");
    }

    // Wrong shape or non-numeric parts yield None.
    for input in ["", "45", "3:45:67:89", "a:b", "3:xy", ":", "1::2"] {
        assert_eq!(parse_duration_to_ms(input), None, "{input}");
    }
}

/// A multi-side medium track without a leading side letter is malformed MB
/// data. The search detail path propagates the error rather than bucketing
/// the track onto side 0.
#[test]
fn mb_detail_errors_on_multi_side_track_without_side_letter() {
    let response = response_with_media(vec![MbMedium {
        format: Some("12\" Vinyl".to_string()),
        tracks: vec![
            make_mb_track("A1", "Track A1"),
            make_mb_track("1", "Numeric-only Track"),
        ],
    }]);

    let err = build_mb_detail("mb-release-1", &response, vec![])
        .expect_err("expected error for vinyl track without side letter");
    assert!(
        matches!(&err, ImportError::SourceData { detail, .. } if detail.contains("no side letter")),
        "unexpected error: {}",
        err
    );
}

/// The picker's pressing fields are the pressing the commit stores: one
/// projection, read by both. They used to be re-derived side by side.
#[test]
fn mb_detail_pressing_matches_the_committed_pressing() {
    let mut response = response_with_media(vec![MbMedium {
        format: Some("CD".to_string()),
        tracks: vec![make_mb_track("1", "Track Title")],
    }]);
    response.date = Some("1996-05-04".to_string());
    response.country = Some("JP".to_string());
    response.barcode = Some("4988006757486".to_string());
    response.label_info = vec![crate::musicbrainz::MbLabelInfo {
        label: Some(crate::musicbrainz::MbLabel {
            name: Some("Toshiba EMI".to_string()),
        }),
        catalog_number: Some("TOCP-8556".to_string()),
    }];

    let detail = build_mb_detail("mb-release-1", &response, vec![]).unwrap();
    let parsed = crate::import::musicbrainz_mapper::map_mb_response_to_db(
        &response,
        None,
        None,
        &test_clock(),
        &SequentialIdProvider::new("mb"),
    )
    .unwrap();
    let committed = parsed.release.pressing;

    assert_eq!(detail.year, committed.year);
    assert_eq!(detail.format, committed.format);
    assert_eq!(detail.label, committed.label);
    assert_eq!(detail.catalog_number, committed.catalog_number);
    assert_eq!(detail.country, committed.country);
    assert_eq!(detail.barcode, committed.barcode);
}

/// The picker's track titles resolve exactly as the commit mapper's do —
/// recording title first, the track's own title only as the fallback. The two
/// used to read the pair in opposite orders, so a release whose track and
/// recording titles differ showed one title in the picker and committed the
/// other.
#[test]
fn mb_detail_track_title_prefers_the_recording_title() {
    let mut track = make_mb_track("1", "Recording Title");
    track.title = Some("Track Title".to_string());
    let fallback = MbTrack {
        position: None,
        number: Some("2".to_string()),
        title: Some("Only A Track Title".to_string()),
        length: None,
        recording: Some(MbRecording {
            id: None,
            title: None,
            artist_credit: vec![],
            relations: vec![],
        }),
        artist_credit: vec![],
    };
    let response = response_with_media(vec![MbMedium {
        format: Some("CD".to_string()),
        tracks: vec![track, fallback],
    }]);

    let detail = build_mb_detail("mb-release-1", &response, vec![]).unwrap();
    let titles: Vec<&str> = detail.tracks.iter().map(|t| t.title.as_str()).collect();
    assert_eq!(titles, vec!["Recording Title", "Only A Track Title"]);

    let parsed = crate::import::musicbrainz_mapper::map_mb_response_to_db(
        &response,
        None,
        None,
        &test_clock(),
        &SequentialIdProvider::new("mb"),
    )
    .unwrap();
    let committed: Vec<&str> = parsed.tracks.iter().map(|t| t.title.as_str()).collect();
    assert_eq!(titles, committed);
}

/// A track with no title anywhere fails the prefetch rather than rendering as
/// an empty row the user can't tell from a real one — the same error the
/// commit mapper raises.
#[test]
fn mb_detail_errors_on_track_without_any_title() {
    let response = response_with_media(vec![MbMedium {
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

    let err = build_mb_detail("mb-release-1", &response, vec![])
        .expect_err("expected error for a title-less track");
    assert!(
        matches!(&err, ImportError::SourceData { detail, .. } if detail.contains("has no track title")),
        "unexpected error: {err}"
    );
}

fn nested_discogs_release() -> crate::discogs::DiscogsRelease {
    crate::discogs::client::parse_discogs_release_json(
        &serde_json::json!({
            "id": 123,
            "title": "Album Title",
            "artists": [{ "id": 1, "name": "Artist Name" }],
            "tracklist": [
                {
                    "position": "",
                    "type_": "index",
                    "title": "Suite Title",
                    "duration": "5:00",
                    "sub_tracks": [
                        {
                            "position": "1a",
                            "type_": "track",
                            "title": "Movement One",
                            "duration": "2:00"
                        },
                        {
                            "position": "1b",
                            "type_": "track",
                            "title": "Movement Two",
                            "duration": "3:00"
                        }
                    ]
                },
                {
                    "position": "2",
                    "type_": "track",
                    "title": "Track Title",
                    "duration": "4:00"
                }
            ]
        })
        .to_string(),
    )
    .expect("nested Discogs tracklist parses")
}

#[test]
fn discogs_detail_includes_nested_index_tracks() {
    let release = nested_discogs_release();

    let detail = build_discogs_detail(&release, Vec::new());
    let titles: Vec<&str> = detail
        .tracks
        .iter()
        .map(|track| track.title.as_str())
        .collect();

    assert_eq!(
        titles,
        vec![
            "Suite Title: Movement One",
            "Suite Title: Movement Two",
            "Track Title"
        ]
    );
}

#[test]
fn discogs_detail_collapses_an_index_for_one_matching_audio_file() {
    let release = nested_discogs_release();

    let detail = build_discogs_detail_for_audio(&release, Vec::new(), &[300_000, 240_000]);
    let titles: Vec<&str> = detail
        .tracks
        .iter()
        .map(|track| track.title.as_str())
        .collect();

    assert_eq!(titles, vec!["Suite Title", "Track Title"]);
}

#[test]
fn discogs_detail_selects_each_index_layout_from_ordered_durations() {
    let release = crate::discogs::client::parse_discogs_release_json(
        &serde_json::json!({
            "id": 456,
            "title": "Album Title",
            "artists": [{ "id": 1, "name": "Artist Name" }],
            "tracklist": [
                {
                    "position": "",
                    "type_": "index",
                    "title": "Suite One",
                    "duration": "3:00",
                    "sub_tracks": [
                        { "position": "1a", "type_": "track", "title": "Part One", "duration": "1:00" },
                        { "position": "1b", "type_": "track", "title": "Part Two", "duration": "2:00" }
                    ]
                },
                {
                    "position": "",
                    "type_": "index",
                    "title": "Suite Two",
                    "duration": "9:00",
                    "sub_tracks": [
                        { "position": "2a", "type_": "track", "title": "Part Three", "duration": "4:00" },
                        { "position": "2b", "type_": "track", "title": "Part Four", "duration": "5:00" }
                    ]
                }
            ]
        })
        .to_string(),
    )
    .expect("two nested Discogs indexes parse");

    let detail = build_discogs_detail_for_audio(&release, Vec::new(), &[60_000, 120_000, 540_000]);
    let titles: Vec<&str> = detail
        .tracks
        .iter()
        .map(|track| track.title.as_str())
        .collect();

    assert_eq!(
        titles,
        vec!["Suite One: Part One", "Suite One: Part Two", "Suite Two"]
    );
}

#[test]
fn nested_index_durations_align_after_preceding_tracks() {
    let release = crate::discogs::client::parse_discogs_release_json(
        &serde_json::json!({
            "id": 789,
            "title": "Album Title",
            "artists": [{ "id": 1, "name": "Artist Name" }],
            "tracklist": [
                { "position": "1", "type_": "track", "title": "Opening Track", "duration": "10:00" },
                {
                    "position": "",
                    "type_": "index",
                    "title": "Grouped Work",
                    "sub_tracks": [
                        {
                            "position": "",
                            "type_": "index",
                            "title": "Suite One",
                            "duration": "3:00",
                            "sub_tracks": [
                                { "position": "2a", "type_": "track", "title": "Part One", "duration": "1:00" },
                                { "position": "2b", "type_": "track", "title": "Part Two", "duration": "2:00" }
                            ]
                        },
                        {
                            "position": "",
                            "type_": "index",
                            "title": "Suite Two",
                            "duration": "9:00",
                            "sub_tracks": [
                                { "position": "3a", "type_": "track", "title": "Part Three", "duration": "4:00" },
                                { "position": "3b", "type_": "track", "title": "Part Four", "duration": "5:00" }
                            ]
                        }
                    ]
                }
            ]
        })
        .to_string(),
    )
    .expect("nested Discogs indexes parse");

    let detail =
        build_discogs_detail_for_audio(&release, Vec::new(), &[600_000, 60_000, 120_000, 540_000]);
    let titles: Vec<&str> = detail
        .tracks
        .iter()
        .map(|track| track.title.as_str())
        .collect();

    assert_eq!(
        titles,
        vec![
            "Opening Track",
            "Grouped Work: Suite One: Part One",
            "Grouped Work: Suite One: Part Two",
            "Grouped Work: Suite Two"
        ]
    );
}
