use super::*;
use crate::discogs::client::DiscogsSearchResult;
use crate::import::discogs_mapper::parse_duration_to_ms;
use crate::musicbrainz::{
    MbArtistCredit, MbMedium, MbRecording, MbReleaseGroupRef, MbReleaseResponse, MbTrack,
};
use coven::{FixedClock, SequentialIdProvider};

/// The release a MusicBrainz document extracts to, read by itself.
fn mb_release(response: &MbReleaseResponse) -> crate::import::source_release::SourceRelease {
    crate::import::payloads::ReleasePayloads::for_test(
        crate::import::MetadataRef::new(Catalog::MusicBrainz, &response.id),
        serde_json::to_string(response).expect("MusicBrainz fixture serializes"),
        Vec::new(),
    )
    .extract()
    .expect("the MusicBrainz fixture extracts")
}

/// The picker's detail for a MusicBrainz document with no folder to fit.
fn mb_detail(response: &MbReleaseResponse) -> Result<ImportSearchReleaseDetail, ImportError> {
    mb_release(response).detail_for_audio(&[], &[])
}

/// The picker's detail for a Discogs document laid out against `audio`.
fn discogs_detail(json: &str, audio: &[u64]) -> ImportSearchReleaseDetail {
    let release = crate::discogs::client::parse_discogs_release_json(json)
        .expect("the Discogs fixture parses");
    crate::import::payloads::ReleasePayloads::for_test(
        crate::import::MetadataRef::new(Catalog::Discogs, &release.id),
        json.to_string(),
        Vec::new(),
    )
    .extract()
    .expect("the Discogs fixture extracts")
    .detail_for_audio(audio, &[])
    .expect("the Discogs fixture reads")
}

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
        barcode: Vec::new(),
        cover_image: None,
        thumb: None,
        master_id: None,
        result_type: "release".to_string(),
    }
}

/// A Discogs search result states every barcode Discogs holds, and a
/// MusicBrainz record printing the second of them is the same pressing.
#[test]
fn discogs_search_result_keeps_every_barcode_for_pairing() {
    let mut result = result_with_title("Artist Name - Album Title");
    result.year = Some("1992".to_string());
    result.master_id = Some(7);
    result.barcode = vec!["0 12345 67890 5".to_string(), "5051961234567".to_string()];
    let discogs = discogs_search_result_to_metadata(result);

    let mut musicbrainz = MetadataResult::for_test(Catalog::MusicBrainz, "mb-1", Some("group-x"));
    musicbrainz.title = "Album Title".to_string();
    musicbrainz.artist = Some("Artist Name".to_string());
    musicbrainz.year = Some(1992);
    musicbrainz.barcodes = vec!["5051961234567".to_string()];

    let groups =
        crate::import::release_group::group_results(crate::import::release_group::unranked(vec![
            musicbrainz,
            discogs,
        ]));
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].pressings().count(), 1);
    assert_eq!(
        groups[0].sections[0].pressings[0]
            .releases
            .iter()
            .map(|release| release.release_id.as_str())
            .collect::<Vec<_>>(),
        vec!["mb-1", "1"]
    );
}

/// Discogs writes zero where a release has no master. That is absence, not a
/// shared group: two such results are two albums, whatever the titles say.
#[test]
fn a_zero_master_id_is_no_group() {
    let results: Vec<DiscogsSearchResult> = serde_json::from_value(serde_json::json!([
        { "id": 11, "title": "Artist Name - Album Title", "master_id": 0, "type": "release" },
        { "id": 12, "title": "Artist Name - Album Title", "master_id": 0, "type": "release" }
    ]))
    .expect("Discogs search results parse");
    let converted: Vec<MetadataResult> = results
        .into_iter()
        .map(discogs_search_result_to_metadata)
        .collect();
    assert!(converted
        .iter()
        .all(|result| result.source_group_id.is_none()));

    let groups = crate::import::release_group::group_results(
        crate::import::release_group::unranked(converted),
    );
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].id, "11");
    assert_eq!(groups[1].id, "12");
}

/// A release document's URL relations name its counterparts: a Discogs
/// release page is a link, a master page names the album and not this
/// pressing, and a page on no known catalog names nothing.
#[test]
fn discid_metadata_links_the_releases_its_document_names() {
    let response: MbReleaseResponse = serde_json::from_value(serde_json::json!({
        "id": "mb-release-1",
        "title": "Album Title",
        "artist-credit": [{ "name": "Artist Name" }],
        "release-group": { "id": "mb-group-1" },
        "label-info": [],
        "media": [
            { "format": "CD", "discs": [{ "id": "disc-1" }], "tracks": [{ "number": "1", "length": 240000, "title": "CD Track" }] }
        ],
        "relations": [
            { "url": { "resource": "https://www.discogs.com/release/42-Artist-Name-Album-Title" } },
            { "url": { "resource": "https://www.discogs.com/master/7-Artist-Name-Album-Title" } },
            { "url": { "resource": "https://example.test/pages/album" } },
            { "url": { "resource": "https://www.discogs.com/release/43" } }
        ],
        "cover-art-archive": { "front": false, "darkened": false }
    }))
    .expect("MusicBrainz DiscID response parses");

    let metadata = mb_discid_release_to_metadata("disc-1", response)
        .expect("the release contains the queried disc");

    assert_eq!(
        metadata.links,
        vec![
            crate::import::MetadataRef::new(Catalog::Discogs, "42"),
            crate::import::MetadataRef::new(Catalog::Discogs, "43"),
        ]
    );
    assert_eq!(
        metadata.media,
        StatedMedia::PerMedium(vec![Some("CD".to_string())])
    );
}

/// A disc ID names one medium of a release that has several. The matching
/// medium's tracks are what the Ready rule counts, but the pressing is made of
/// every medium the response lists: the Discogs record of the same two media
/// is this object and one naming a cassette is not, whichever medium the
/// response lists first.
#[test]
fn discid_metadata_carries_every_medium_into_pairing() {
    let barcode = "012345678905";
    let discid_release = |media: serde_json::Value| -> MetadataResult {
        let response: MbReleaseResponse = serde_json::from_value(serde_json::json!({
            "id": "mb-release-1",
            "title": "Album Title",
            "date": "1992",
            "barcode": barcode,
            "artist-credit": [{ "name": "Artist Name" }],
            "release-group": { "id": "mb-group-1" },
            "label-info": [],
            "media": media,
            "relations": [],
            "cover-art-archive": { "front": false, "darkened": false }
        }))
        .expect("MusicBrainz DiscID response parses");
        mb_discid_release_to_metadata("disc-1", response)
            .expect("the release contains the queried disc")
    };
    let vinyl_then_cd = discid_release(serde_json::json!([
        { "format": "12\" Vinyl", "discs": [], "tracks": [{ "number": "A1", "length": 180000, "title": "Vinyl Track" }, { "number": "A2", "length": 180000, "title": "Vinyl Track" }] },
        { "format": "CD", "discs": [{ "id": "disc-1" }], "tracks": [{ "number": "1", "length": 240000, "title": "CD Track" }] }
    ]));
    let cd_then_vinyl = discid_release(serde_json::json!([
        { "format": "CD", "discs": [{ "id": "disc-1" }], "tracks": [{ "number": "1", "length": 240000, "title": "CD Track" }] },
        { "format": "12\" Vinyl", "discs": [], "tracks": [{ "number": "A1", "length": 180000, "title": "Vinyl Track" }, { "number": "A2", "length": 180000, "title": "Vinyl Track" }] }
    ]));
    for release in [&vinyl_then_cd, &cd_then_vinyl] {
        assert_eq!(release.format.as_deref(), Some("CD"));
        assert_eq!(
            release.source_tracks,
            Some(SourceTracks::Listed { count: 1 })
        );
    }

    let discogs_of = |format: &[&str]| -> MetadataResult {
        let mut result = result_with_title("Artist Name - Album Title");
        result.year = Some("1992".to_string());
        result.master_id = Some(7);
        result.barcode = vec![barcode.to_string()];
        result.format = Some(format.iter().map(|f| f.to_string()).collect());
        discogs_search_result_to_metadata(result)
    };
    let cassette = discogs_of(&["Cassette", "Album"]);
    let both = discogs_of(&["Vinyl", "LP", "CD", "Album"]);

    for musicbrainz in [vinyl_then_cd, cd_then_vinyl] {
        assert_eq!(
            crate::import::release_group::pressing_count(vec![
                musicbrainz.clone(),
                cassette.clone()
            ]),
            2,
            "a cassette is not part of a CD-plus-vinyl object"
        );
        assert_eq!(
            crate::import::release_group::pressing_count(vec![musicbrainz.clone(), both.clone()]),
            1,
            "the record naming both media is the same object"
        );
    }
}

/// A typed query builds one request per provider from the same fields.
#[test]
fn a_general_query_builds_both_providers_requests() {
    let query = SearchQuery::General {
        artist: "Artist Name".to_string(),
        album: "Album Title".to_string(),
    };
    let musicbrainz = query.musicbrainz_params();
    assert_eq!(musicbrainz.artist.as_deref(), Some("Artist Name"));
    assert_eq!(musicbrainz.album.as_deref(), Some("Album Title"));
    let discogs = query.discogs_params();
    assert_eq!(
        discogs.text.as_deref(),
        Some("Artist Name"),
        "the artist is matched as words, not as Discogs's main artist name"
    );
    assert_eq!(discogs.release_title.as_deref(), Some("Album Title"));
}

#[test]
fn catalog_and_barcode_queries_fill_their_own_provider_fields() {
    let catalog = SearchQuery::CatalogNumber {
        catalog_number: "CAT-7".to_string(),
    };
    assert_eq!(
        catalog.musicbrainz_params().catalog_number.as_deref(),
        Some("CAT-7")
    );
    assert_eq!(catalog.discogs_params().catno.as_deref(), Some("CAT-7"));

    // A code is asked for in its one spelling: a UPC-A as its EAN-13,
    // however it was typed.
    for typed in ["012345678905", " 0 12345 67890 5 ", "0012345678905"] {
        let barcode = SearchQuery::Barcode {
            barcode: typed.to_string(),
        };
        assert_eq!(
            barcode.musicbrainz_params().barcode.as_deref(),
            Some("0012345678905"),
            "{typed}"
        );
        assert_eq!(
            barcode.discogs_params().barcode.as_deref(),
            Some("0012345678905"),
            "{typed}"
        );
    }

    // A value whose check digit fails is still asked for, by its digits.
    let mistyped = SearchQuery::Barcode {
        barcode: "0 12345 67890 6".to_string(),
    };
    assert_eq!(
        mistyped.musicbrainz_params().barcode.as_deref(),
        Some("012345678906")
    );
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
            image: crate::import::cover_art::RemoteImageSet::with_copies(
                "https://discogs.example/full.jpg".to_string(),
                vec![crate::import::cover_art::DownscaledCopy {
                    url: "https://discogs.example/thumb.jpg".to_string(),
                    max_edge: 150,
                }],
            ),
            label: Catalog::Discogs.cover_source_label().to_string(),
            source: Catalog::Discogs,
            standing: crate::import::cover_art::CoverStanding::Stated,
        })
    );
}

#[test]
fn discid_metadata_uses_the_medium_that_contains_the_disc() {
    let response: MbReleaseResponse = serde_json::from_value(serde_json::json!({
        "id": "mb-release-1",
        "title": "Album Title",
        "artist-credit": [{ "name": "Artist Name" }],
        "release-group": { "id": "mb-group-1" },
        "label-info": [],
        "media": [
            {
                "format": "12\" Vinyl",
                "discs": [],
                "tracks": [
                    { "number": "A1", "length": 180000, "title": "Vinyl Track" },
                    { "number": "A2", "length": 180000, "title": "Vinyl Track" }
                ]
            },
            {
                "format": "CD",
                "discs": [{ "id": "disc-1" }],
                "tracks": [
                    { "number": "1", "length": 240000, "title": "CD Track" }
                ]
            }
        ],
        "relations": [],
        "cover-art-archive": { "front": false, "darkened": false }
    }))
    .expect("MusicBrainz DiscID response parses");

    let metadata = mb_discid_release_to_metadata("disc-1", response)
        .expect("the release contains the queried disc");

    assert_eq!(metadata.format.as_deref(), Some("CD"));
    assert_eq!(
        metadata.source_tracks,
        Some(SourceTracks::Listed { count: 1 })
    );
}

#[test]
fn discid_metadata_skips_only_releases_without_one_matching_medium() {
    let no_match = response_with_media(vec![MbMedium {
        discs: vec![],
        format: Some("12\" Vinyl".to_string()),
        tracks: vec![make_mb_track("A1", "Vinyl Track")],
    }]);
    let multiple_matches = response_with_media(vec![
        MbMedium {
            discs: vec![crate::musicbrainz::MbDisc {
                id: "disc-1".to_string(),
            }],
            format: Some("CD".to_string()),
            tracks: vec![make_mb_track("1", "First CD Track")],
        },
        MbMedium {
            discs: vec![crate::musicbrainz::MbDisc {
                id: "disc-1".to_string(),
            }],
            format: Some("CD".to_string()),
            tracks: vec![make_mb_track("1", "Second CD Track")],
        },
    ]);
    let mut valid = response_with_media(vec![MbMedium {
        discs: vec![crate::musicbrainz::MbDisc {
            id: "disc-1".to_string(),
        }],
        format: Some("CD".to_string()),
        tracks: vec![make_mb_track("1", "CD Track")],
    }]);
    valid.id = "mb-release-2".to_string();

    let metadata =
        mb_discid_releases_to_metadata("disc-1", vec![no_match, multiple_matches, valid]);

    assert_eq!(metadata.len(), 1);
    assert_eq!(metadata[0].release_id, "mb-release-2");
    assert_eq!(metadata[0].format.as_deref(), Some("CD"));
}

/// A release whose document says the archive holds its front image offers
/// that image, and not the release group's address, which nothing states.
#[test]
fn mb_detail_offers_the_stated_archive_front_alone() {
    let response = response_with_media(vec![MbMedium {
        discs: vec![],
        format: Some("CD".to_string()),
        tracks: vec![make_mb_track("1", "Track Title")],
    }]);

    let detail = mb_detail(&response).unwrap();

    assert_eq!(
        detail.cover_art,
        vec![RemoteCover {
            standing: crate::import::cover_art::CoverStanding::Stated,
            ..RemoteCover::musicbrainz_release("mb-release-1")
        }]
    );
}

/// Vinyl side numbering runs continuously across media: medium 1 (A/B) is
/// sides 1-2, medium 2 (C/D) is sides 3-4. Same shared assignment the DB
/// mapper uses.
#[test]
fn mb_detail_numbers_vinyl_sides_across_media() {
    let response = response_with_media(vec![
        MbMedium {
            discs: vec![],
            format: Some("12\" Vinyl".to_string()),
            tracks: vec![
                make_mb_track("A1", "Track A1"),
                make_mb_track("B1", "Track B1"),
            ],
        },
        MbMedium {
            discs: vec![],
            format: Some("12\" Vinyl".to_string()),
            tracks: vec![
                make_mb_track("C1", "Track C1"),
                make_mb_track("D1", "Track D1"),
            ],
        },
    ]);

    let detail = mb_detail(&response).unwrap();
    let sides: Vec<Option<u32>> = detail.tracks.iter().map(|t| t.side).collect();
    assert_eq!(sides, vec![Some(1), Some(2), Some(3), Some(4)]);
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

/// The search detail preserves both known and unknown side assignments.
#[test]
fn mb_detail_preserves_unknown_sides() {
    let response = response_with_media(vec![MbMedium {
        discs: vec![],
        format: Some("12\" Vinyl".to_string()),
        tracks: vec![
            make_mb_track("A1", "Track A1"),
            make_mb_track("1", "Numeric-only Track"),
        ],
    }]);

    let detail = mb_detail(&response).unwrap();
    assert_eq!(detail.tracks[0].side, Some(1));
    assert_eq!(detail.tracks[1].side, None);
}

/// The picker's pressing fields are the pressing the commit stores: one
/// projection, read by both. They used to be re-derived side by side.
#[test]
fn mb_detail_pressing_matches_the_committed_pressing() {
    let mut response = response_with_media(vec![MbMedium {
        discs: vec![],
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

    let detail = mb_detail(&response).unwrap();
    let parsed = mb_release(&response)
    .parsed(
        &[],
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
        discs: vec![],
        format: Some("CD".to_string()),
        tracks: vec![track, fallback],
    }]);

    let detail = mb_detail(&response).unwrap();
    let titles: Vec<&str> = detail.tracks.iter().map(|t| t.title.as_str()).collect();
    assert_eq!(titles, vec!["Recording Title", "Only A Track Title"]);

    let parsed = mb_release(&response)
    .parsed(
        &[],
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

    let err = mb_detail(&response)
        .expect_err("expected error for a title-less track");
    assert!(
        matches!(&err, ImportError::SourceData { detail, .. } if detail.contains("has no track title")),
        "unexpected error: {err}"
    );
}

fn nested_discogs_release() -> String {
    serde_json::json!({
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
        .to_string()
}

#[test]
fn discogs_detail_collapses_an_index_for_one_matching_audio_file() {
    let detail = discogs_detail(&nested_discogs_release(), &[300_000, 240_000]);
    let titles: Vec<&str> = detail
        .tracks
        .iter()
        .map(|track| track.title.as_str())
        .collect();

    assert_eq!(titles, vec!["Suite Title", "Track Title"]);
}

#[test]
fn discogs_detail_selects_each_index_layout_from_ordered_durations() {
    let release = serde_json::json!({
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
        .to_string();

    let detail = discogs_detail(&release, &[60_000, 120_000, 540_000]);
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
    let release = serde_json::json!({
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
        .to_string();

    let detail = discogs_detail(&release, &[600_000, 60_000, 120_000, 540_000]);
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

/// A MusicBrainz search response states each medium's format, and the result
/// carries it: the row shows the format, and pairing reads the medium.
#[test]
fn a_musicbrainz_search_result_states_its_media() {
    let release: crate::musicbrainz::SearchRelease = serde_json::from_value(serde_json::json!({
        "id": "mb-release-1",
        "title": "Album Title",
        "date": "1970",
        "country": "JM",
        "label-info": [],
        "media": [
            { "format": "Vinyl", "disc-count": 0, "track-count": 12 },
            { "format": "", "disc-count": 0, "track-count": 1 }
        ]
    }))
    .expect("search release parses");
    let result = search_release_to_metadata(release, None);
    assert_eq!(result.format.as_deref(), Some("Vinyl"));
    assert_eq!(
        result.media,
        StatedMedia::PerMedium(vec![Some("Vinyl".to_string()), None])
    );
}
