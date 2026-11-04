use super::*;
use crate::discogs::client::DiscogsSearchResult;
use crate::discogs_mapper::parse_duration_to_ms;
use crate::musicbrainz::{
    MbArtistCredit, MbMedium, MbRecording, MbReleaseGroupRef, MbReleaseResponse, MbTrack,
};
use crate::test_fixtures::make_mb_track;
use bae_types::pressing::{MediaCount, Medium, StatedMedia};

/// The release a MusicBrainz document extracts to, read by itself.
fn mb_release(response: &MbReleaseResponse) -> crate::source_release::SourceRelease {
    crate::payloads::ReleasePayloads::for_test(
        crate::records::LookupRef::new(LookupCatalog::MusicBrainz, &response.id),
        serde_json::to_string(response).expect("MusicBrainz fixture serializes"),
        Vec::new(),
    )
    .extract()
    .expect("the MusicBrainz fixture extracts")
}

/// The picker's detail for a MusicBrainz document.
fn mb_detail(response: &MbReleaseResponse) -> ImportSearchReleaseDetail {
    mb_release(response).detail(&[])
}

/// The titles a Discogs document reads for the folder's `audio`.
fn discogs_titles(json: &str, audio: &[u64]) -> Vec<String> {
    let release = crate::discogs::documents::parse_discogs_release_json(json)
        .expect("the Discogs fixture parses");
    crate::payloads::ReleasePayloads::for_test(
        crate::records::LookupRef::new(LookupCatalog::Discogs, &release.id),
        json.to_string(),
        Vec::new(),
    )
    .extract()
    .expect("the Discogs fixture extracts")
    .read(&crate::reading::ReadAudio::of_lengths(audio))
    .expect("the Discogs fixture reads")
    .tracks
    .into_iter()
    .map(|track| track.title)
    .collect()
}

fn response_with_media(media: Vec<MbMedium>) -> MbReleaseResponse {
    MbReleaseResponse {
        id: "mb-release-1".to_string(),
        title: "Album Title".to_string(),
        disambiguation: None,
        annotation: None,
        date: None,
        country: None,
        status: None,
        packaging: None,
        barcode: None,
        artist_credit: vec![MbArtistCredit {
            joinphrase: None,
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
        cover_art_archive: crate::musicbrainz::MbCoverArtArchive { front: true, darkened: false },
    }
}

fn result_with_title(title: &str) -> DiscogsSearchResult {
    DiscogsSearchResult {
        id: 1,
        title: title.to_string(),
        year: None,
        formats: Vec::new(),
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

/// A Discogs search result states every barcode Discogs holds, in its
/// order, under the master it names.
#[test]
fn discogs_search_result_keeps_every_barcode() {
    let mut result = result_with_title("Artist Name - Album Title");
    result.year = Some("1992".to_string());
    result.master_id = Some(7);
    result.barcode = vec!["0 12345 67890 5".to_string(), "5051961234567".to_string()];
    let discogs = discogs_search_result_to_metadata(result);

    assert_eq!(discogs.barcodes, vec!["0 12345 67890 5", "5051961234567"]);
    assert_eq!(discogs.source_group_id.as_deref(), Some("7"));
    assert_eq!(discogs.title, "Album Title");
    assert_eq!(discogs.artist.as_deref(), Some("Artist Name"));
    assert_eq!(discogs.year, Some(1992));
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
    let converted: Vec<MetadataResult> =
        results.into_iter().map(discogs_search_result_to_metadata).collect();
    assert!(converted.iter().all(|result| result.source_group_id.is_none()));
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
            crate::records::MetadataRef::new(Catalog::Discogs, "42"),
            crate::records::MetadataRef::new(Catalog::Discogs, "43"),
        ]
    );
    assert_eq!(metadata.media, StatedMedia::PerMedium(vec![Some(Medium::Cd)]));
}

/// A disc ID names one medium of a release that has several. The pressing is
/// made of every medium the response lists, whichever medium the response
/// lists first, as a Discogs record's pressing is made of every format it
/// names.
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
        let mut media = release.facts().media;
        media.sort_by_key(|counted| counted.medium);
        assert_eq!(
            media,
            vec![
                MediaCount { medium: Medium::Cd, count: 1 },
                MediaCount { medium: Medium::Vinyl, count: 1 },
            ],
            "the pressing is every medium the release lists, not the one the disc is"
        );
    }

    let discogs_of = |formats: &[(&str, &[&str])]| -> MetadataResult {
        let mut result = result_with_title("Artist Name - Album Title");
        result.year = Some("1992".to_string());
        result.master_id = Some(7);
        result.barcode = vec![barcode.to_string()];
        result.formats = formats
            .iter()
            .map(|(name, descriptions)| crate::discogs::DiscogsFormat {
                name: name.to_string(),
                qty: "1".to_string(),
                descriptions: descriptions.iter().map(|d| d.to_string()).collect(),
                text: None,
            })
            .collect();
        discogs_search_result_to_metadata(result)
    };
    let media = |result: &MetadataResult| {
        let mut media = result.facts().media;
        media.sort_by_key(|counted| counted.medium);
        media
    };
    assert_eq!(
        media(&discogs_of(&[("Cassette", &["Album"])])),
        vec![MediaCount { medium: Medium::Cassette, count: 1 }]
    );
    assert_eq!(
        media(&discogs_of(&[("Vinyl", &["LP"]), ("CD", &["Album"])])),
        vec![
            MediaCount { medium: Medium::Cd, count: 1 },
            MediaCount { medium: Medium::Vinyl, count: 1 },
        ],
        "a Discogs record naming two formats is a pressing of both media"
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
            image: crate::remote_image::RemoteImageSet::with_copies(
                "https://discogs.example/full.jpg".to_string(),
                vec![crate::remote_image::DownscaledCopy {
                    url: "https://discogs.example/thumb.jpg".to_string(),
                    max_edge: 150,
                }],
            ),
            label: LookupCatalog::Discogs.cover_source_label().to_string(),
            source: Catalog::Discogs,
            standing: crate::remote_image::CoverStanding::Stated,
        })
    );
}

/// The medium a disc ID names is one disc of what the folder may hold, so
/// its tracks are not the release's tracklist read against the folder's
/// audio: the result states none until the release's document is read.
#[test]
fn discid_metadata_states_no_tracklist_of_the_medium_it_names() {
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

    assert_eq!(metadata.source_tracks, None);
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
            discs: vec![crate::musicbrainz::MbDisc { id: "disc-1".to_string() }],
            format: Some("CD".to_string()),
            tracks: vec![make_mb_track("1", "First CD Track")],
        },
        MbMedium {
            discs: vec![crate::musicbrainz::MbDisc { id: "disc-1".to_string() }],
            format: Some("CD".to_string()),
            tracks: vec![make_mb_track("1", "Second CD Track")],
        },
    ]);
    let mut valid = response_with_media(vec![MbMedium {
        discs: vec![crate::musicbrainz::MbDisc { id: "disc-1".to_string() }],
        format: Some("CD".to_string()),
        tracks: vec![make_mb_track("1", "CD Track")],
    }]);
    valid.id = "mb-release-2".to_string();

    let answered = |release: crate::musicbrainz::MbReleaseResponse| {
        let json = serde_json::to_string(&release).unwrap();
        (release, json)
    };
    let metadata = mb_discid_releases_to_metadata(
        "disc-1",
        vec![answered(no_match), answered(multiple_matches), answered(valid)],
    );

    assert_eq!(metadata.len(), 1);
    let (result, document) = &metadata[0];
    assert_eq!(result.release_id, "mb-release-2");
    assert_eq!(result.media, StatedMedia::PerMedium(vec![Some(Medium::Cd)]));
    assert_eq!(
        document.release(),
        &LookupRef::new(LookupCatalog::MusicBrainz, "mb-release-2"),
        "the release kept is paired with its own document"
    );
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

    let detail = mb_detail(&response);

    assert_eq!(
        detail.cover_art,
        vec![RemoteCover {
            standing: crate::remote_image::CoverStanding::Stated,
            ..RemoteCover::musicbrainz_release("mb-release-1")
        }]
    );
}

/// Vinyl side numbering runs continuously across media: medium 1 (A/B) is
/// sides 1-2, medium 2 (C/D) is sides 3-4.
#[test]
fn mb_values_number_vinyl_sides_across_media() {
    let response = response_with_media(vec![
        MbMedium {
            discs: vec![],
            format: Some("12\" Vinyl".to_string()),
            tracks: vec![make_mb_track("A1", "Track A1"), make_mb_track("B1", "Track B1")],
        },
        MbMedium {
            discs: vec![],
            format: Some("12\" Vinyl".to_string()),
            tracks: vec![make_mb_track("C1", "Track C1"), make_mb_track("D1", "Track D1")],
        },
    ]);

    let values = mb_release(&response).every_medium_values().unwrap();
    assert_eq!(crate::release_values::value_labels(&values), ["A1", "B1", "C1", "D1"]);
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

/// A duration whose milliseconds overflow a u64 states no length, rather
/// than stopping the read on the catalog's data.
#[test]
fn a_duration_past_u64_milliseconds_states_no_length() {
    for input in ["307445734561825861:00", "5124095576030432:00:00", "0:18446744073709552"] {
        assert_eq!(parse_duration_to_ms(input), None, "{input}");
    }
}

/// A track whose position names no side is on the side before it.
#[test]
fn mb_values_put_an_unlettered_track_on_the_side_before_it() {
    let response = response_with_media(vec![MbMedium {
        discs: vec![],
        format: Some("12\" Vinyl".to_string()),
        tracks: vec![make_mb_track("A1", "Track A1"), make_mb_track("1", "Numeric-only Track")],
    }]);

    let values = mb_release(&response).every_medium_values().unwrap();
    assert_eq!(crate::release_values::value_labels(&values), ["A1", "A1"]);
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
    response.status = Some("Promotion".to_string());
    response.packaging = Some("Digipak".to_string());
    response.barcode = Some("4900000000011".to_string());
    response.label_info = vec![crate::musicbrainz::MbLabelInfo {
        label: Some(crate::musicbrainz::MbLabel { id: None, name: Some("Label".to_string()) }),
        catalog_number: Some("CAT-8556".to_string()),
    }];

    let detail = mb_detail(&response);
    let parsed = mb_release(&response).every_medium_values().unwrap();
    let committed = parsed.pressing;

    assert_eq!(detail.year, committed.year);
    assert_eq!(detail.labels, committed.labels);
    assert_eq!(detail.facts, committed.facts);
    assert_eq!(detail.barcodes, committed.barcodes);
    assert_eq!(detail.facts.status, Some(bae_types::pressing::ReleaseStatus::Promotion));
}

/// A track's title is the recording's first, the track's own title only as
/// the fallback.
#[test]
fn a_track_title_prefers_the_recording_title() {
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

    let parsed = mb_release(&response).every_medium_values().unwrap();
    let titles: Vec<&str> = parsed.tracks.iter().map(|t| t.title.as_str()).collect();
    assert_eq!(titles, vec!["Recording Title", "Only A Track Title"]);
}

/// A track with no title anywhere fails the reading rather than becoming an
/// empty row the user can't tell from a real one.
#[test]
fn a_track_without_any_title_fails_the_reading() {
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

    let err = mb_release(&response)
        .every_medium_values()
        .expect_err("expected error for a title-less track");
    assert!(
        matches!(&err, CatalogError::SourceData { detail, .. } if detail.contains("has no track title")),
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
fn a_discogs_reading_collapses_an_index_for_one_matching_audio_file() {
    let titles = discogs_titles(&nested_discogs_release(), &[300_000, 240_000]);

    assert_eq!(titles, vec!["Suite Title", "Track Title"]);
}

#[test]
fn a_discogs_reading_selects_each_index_layout_from_ordered_durations() {
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

    let titles = discogs_titles(&release, &[60_000, 120_000, 540_000]);

    assert_eq!(titles, vec!["Suite One: Part One", "Suite One: Part Two", "Suite Two"]);
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

    let titles = discogs_titles(&release, &[600_000, 60_000, 120_000, 540_000]);

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

/// A MusicBrainz search response states each medium's format, its country,
/// status and packaging, and the result carries them typed: the row shows
/// the counted media, and pairing reads each medium.
#[test]
fn a_musicbrainz_search_result_states_its_media() {
    let release: crate::musicbrainz::SearchRelease = serde_json::from_value(serde_json::json!({
        "id": "mb-release-1",
        "title": "Album Title",
        "date": "1970",
        "country": "JM",
        "status": "Official",
        "packaging": "Cardboard/Paper Sleeve",
        "label-info": [],
        "media": [
            { "format": "Vinyl", "disc-count": 0, "track-count": 12 },
            { "format": "", "disc-count": 0, "track-count": 1 }
        ]
    }))
    .expect("search release parses");
    let result = search_release_to_metadata(release, None);
    assert_eq!(
        result.facts(),
        bae_types::pressing::PressingFacts {
            area: Some(bae_types::pressing::area("JM")),
            media: vec![MediaCount { medium: Medium::Vinyl, count: 1 }],
            status: Some(bae_types::pressing::ReleaseStatus::Official),
            packaging: Some(bae_types::pressing::Packaging::CardboardSleeve),
            discogs_details: Vec::new(),
        }
    );
    assert_eq!(result.media, StatedMedia::PerMedium(vec![Some(Medium::Vinyl), None]));
}

/// A Discogs search result carries its structured format entries: the
/// row reads the country as an area and each entry as a counted medium, a
/// "Promo" as the status, and the details it has no field for.
#[test]
fn a_discogs_search_result_reads_its_format_entries() {
    let mut result = result_with_title("Artist - Album");
    result.country = Some("Japan".to_string());
    result.formats = vec![crate::discogs::DiscogsFormat {
        name: "CD".to_string(),
        qty: "2".to_string(),
        descriptions: vec![
            "Album".to_string(),
            "Reissue".to_string(),
            "Promo".to_string(),
            "Stereo".to_string(),
        ],
        text: None,
    }];
    let result = discogs_search_result_to_metadata(result);
    assert_eq!(
        result.facts(),
        bae_types::pressing::PressingFacts {
            area: Some(bae_types::pressing::area("JP")),
            media: vec![MediaCount { medium: Medium::Cd, count: 2 }],
            status: Some(bae_types::pressing::ReleaseStatus::Promotion),
            packaging: None,
            discogs_details: vec![
                bae_types::pressing::DiscogsDetail::Reissue,
                bae_types::pressing::DiscogsDetail::Stereo,
            ],
        }
    );
}

/// A recording the search returned under no asked code answers nothing; a
/// release two recordings are on answers once; and a release that states no
/// credit of its own is the recording's.
#[test]
fn the_isrc_search_answers_each_release_of_an_asked_recording_once() {
    let recording = |isrc: &str, releases: serde_json::Value| {
        serde_json::from_value::<musicbrainz::SearchRecording>(serde_json::json!({
            "isrcs": [isrc],
            "artist-credit": [{"name": "Artist Name", "artist": null}],
            "releases": releases,
        }))
        .expect("the recording parses")
    };
    let album = serde_json::json!({"id": "rel-album", "title": "Album Title"});
    let compilation = serde_json::json!({
        "id": "rel-compilation",
        "title": "Compilation Title",
        "artist-credit": [{"name": "Various Artists", "artist": null}],
    });
    let results = isrc_releases_to_metadata(
        &["XX0000000001".to_string(), "XX0000000002".to_string()],
        vec![
            recording("xx-000-00-00001", serde_json::json!([album, compilation])),
            recording("XX0000000002", serde_json::json!([album])),
            recording(
                "XX0000000009",
                serde_json::json!([{"id": "rel-other", "title": "Other Title"}]),
            ),
        ],
    );
    assert_eq!(
        results
            .iter()
            .map(|result| (result.release_id.as_str(), result.artist.as_deref()))
            .collect::<Vec<_>>(),
        vec![("rel-album", Some("Artist Name")), ("rel-compilation", Some("Various Artists")),]
    );
}

/// A MusicBrainz search response lists every label with its number, and the
/// result keeps each pair in order.
#[test]
fn a_musicbrainz_search_result_keeps_every_label() {
    let release: crate::musicbrainz::SearchRelease = serde_json::from_value(serde_json::json!({
        "id": "mb-release-1",
        "title": "Album",
        "label-info": [
            { "catalog-number": "AB 100", "label": { "name": "Label A" } },
            { "catalog-number": "CL 719", "label": { "name": "Label B" } }
        ]
    }))
    .expect("search release parses");
    assert_eq!(
        search_release_to_metadata(release, None).labels,
        vec![
            ReleaseLabel::of(Some("Label A"), Some("AB 100")),
            ReleaseLabel::of(Some("Label B"), Some("CL 719")),
        ]
    );
}

/// A Discogs search result states the first label's number and no other, and
/// lists label and company names together: the result reads the first label
/// alone, its name with its number.
#[test]
fn a_discogs_search_result_reads_its_first_label() {
    let mut result = result_with_title("Artist - Album");
    result.label =
        Some(vec!["Label A".to_string(), "Label B".to_string(), "Pressing Plant".to_string()]);
    result.catno = Some("AB 100".to_string());
    assert_eq!(
        discogs_search_result_to_metadata(result).labels,
        vec![ReleaseLabel::of(Some("Label A"), Some("AB 100"))]
    );
}

/// A Discogs search result's `none` catalog number is no number: the first
/// label keeps its name alone.
#[test]
fn a_discogs_search_results_none_catalog_number_is_no_number() {
    let mut result = result_with_title("Artist - Album");
    result.label = Some(vec!["Label A".to_string()]);
    result.catno = Some("none".to_string());
    assert_eq!(
        discogs_search_result_to_metadata(result).labels,
        vec![ReleaseLabel::of(Some("Label A"), None)]
    );
}

/// A search result carries the free text its catalog writes about the
/// pressing before any document is read: a MusicBrainz release's
/// disambiguation, and what a Discogs format entry writes beside its name.
#[test]
fn a_search_result_carries_its_notes() {
    let release: crate::musicbrainz::SearchRelease = serde_json::from_value(serde_json::json!({
        "id": "mb-release-1",
        "title": "Album Title",
        "disambiguation": "made in Nordland",
    }))
    .expect("search release parses");
    assert_eq!(
        search_release_to_metadata(release, None).notes,
        vec!["made in Nordland".to_string()]
    );

    let result = crate::discogs::client::DiscogsSearchResult {
        id: 1,
        title: "Artist - Album".to_string(),
        year: None,
        formats: vec![crate::discogs::DiscogsFormat {
            name: "Vinyl".to_string(),
            qty: "1".to_string(),
            descriptions: vec!["LP".to_string()],
            text: Some("Small label".to_string()),
        }],
        country: None,
        label: None,
        catno: None,
        barcode: Vec::new(),
        cover_image: None,
        thumb: None,
        master_id: None,
        result_type: "release".to_string(),
    };
    assert_eq!(discogs_search_result_to_metadata(result).notes, vec!["Small label".to_string()]);
}
