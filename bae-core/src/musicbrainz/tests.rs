use super::*;

#[test]
fn test_release_search_params_build_query() {
    let params = ReleaseSearchParams {
        artist: Some("Test Artist".to_string()),
        album: Some("Test Album".to_string()),
        year: Some("2000".to_string()),
        ..Default::default()
    };
    assert_eq!(
        params.build_query(),
        "artist:\"Test Artist\" AND release:\"Test Album\" AND date:2000",
    );
    let params2 = ReleaseSearchParams {
        artist: Some("Another Artist".to_string()),
        catalog_number: Some("TL-1234".to_string()),
        ..Default::default()
    };
    assert_eq!(
        params2.build_query(),
        "artist:\"Another Artist\" AND catno:\"TL-1234\""
    );
}

#[test]
fn release_search_params_ignore_blank_fields() {
    let blank_params = ReleaseSearchParams {
        artist: Some("   ".to_string()),
        album: Some("\n\t".to_string()),
        ..Default::default()
    };
    assert!(!blank_params.has_any_field());
    assert_eq!(blank_params.build_query(), "");

    let params = ReleaseSearchParams {
        artist: Some("  Artist Name  ".to_string()),
        year: Some(" 2000 ".to_string()),
        ..Default::default()
    };
    assert!(params.has_any_field());
    assert_eq!(params.build_query(), "artist:\"Artist Name\" AND date:2000");
}

#[test]
fn release_search_params_escape_quoted_lucene_values() {
    assert_eq!(
        QueryValueFormat::Quoted.render("release", r#"Quoted "Middle" Phrase"#),
        r#"release:"Quoted \"Middle\" Phrase""#,
    );
    assert_eq!(
        QueryValueFormat::Quoted.render("release", r#"Backslash at end\"#),
        r#"release:"Backslash at end\\""#,
    );
    assert_eq!(
        QueryValueFormat::Quoted.render("artist", "Artist Name"),
        r#"artist:"Artist Name""#,
    );
}

#[test]
fn release_search_params_build_query_with_escaped_phrase() {
    let params = ReleaseSearchParams {
        artist: Some("Artist Name".to_string()),
        album: Some(r#"Quoted "Middle" Phrase"#.to_string()),
        ..Default::default()
    };

    assert_eq!(
        params.build_query(),
        r#"artist:"Artist Name" AND release:"Quoted \"Middle\" Phrase""#,
    );
}

#[tokio::test(start_paused = true)]
async fn test_rate_limiter_enforces_spacing() {
    // First call should return immediately
    let start = Instant::now();
    wait_for_rate_limit().await;
    let first_elapsed = start.elapsed();
    assert!(
        first_elapsed < Duration::from_millis(100),
        "First call should be near-instant, took {:?}",
        first_elapsed
    );

    // Second call should wait ~1 second
    let start = Instant::now();
    wait_for_rate_limit().await;
    let second_elapsed = start.elapsed();
    assert!(
        second_elapsed >= Duration::from_millis(900),
        "Second call should wait ~1s, only waited {:?}",
        second_elapsed
    );
}

#[test]
fn test_mb_release_response_track_count() {
    let response = MbReleaseResponse {
        id: "r1".to_string(),
        title: "T".to_string(),
        date: None,
        country: None,
        barcode: None,
        artist_credit: vec![],
        release_group: None,
        label_info: vec![],
        media: vec![
            MbMedium {
                format: None,
                tracks: vec![
                    MbTrack {
                        position: Some(1),
                        number: None,
                        title: Some("Track 1".to_string()),
                        length: None,
                        recording: None,
                        artist_credit: vec![],
                    },
                    MbTrack {
                        position: Some(2),
                        number: None,
                        title: Some("Track 2".to_string()),
                        length: None,
                        recording: None,
                        artist_credit: vec![],
                    },
                ],
            },
            MbMedium {
                format: None,
                tracks: vec![MbTrack {
                    position: Some(1),
                    number: None,
                    title: Some("Track 1 Disc 2".to_string()),
                    length: None,
                    recording: None,
                    artist_credit: vec![],
                }],
            },
        ],
        relations: vec![],
    };

    assert_eq!(response.track_count(), 3);
}

#[test]
fn test_extract_urls_from_relations() {
    let relations = vec![
        MbRelation {
            url: Some(MbUrlResource {
                resource: Some("https://www.discogs.com/master/12345".to_string()),
            }),
            ..Default::default()
        },
        MbRelation {
            url: Some(MbUrlResource {
                resource: Some("https://www.discogs.com/release/67890".to_string()),
            }),
            ..Default::default()
        },
        MbRelation {
            url: None,
            ..Default::default()
        },
    ];

    let mut urls = ExternalUrls {
        discogs_release_url: None,
    };

    extract_urls_from_relations(&relations, &mut urls);

    assert_eq!(
        urls.discogs_release_url.as_deref(),
        Some("https://www.discogs.com/release/67890")
    );
}

#[test]
fn test_deserialize_mb_release_response() {
    let json = r#"{
        "id": "f9469bd8-a413-43f1-bee3-e3baabfb91cc",
        "title": "Super Hits of the 70s",
        "date": "2002",
        "country": null,
        "barcode": "8711638222024",
        "artist-credit": [{
            "name": "All Star Cover Band",
            "artist": {
                "id": "53ebb100-5cfb-42e7-9ae3-453464420840",
                "name": "All Star Cover Band",
                "sort-name": "All Star Cover Band"
            }
        }],
        "release-group": {
            "id": "ded0036e-243a-4ae4-8c65-7ec37aae4bd9",
            "first-release-date": "2002",
            "secondary-types": [],
            "secondary-type-ids": []
        },
        "label-info": [{
            "catalog-number": "3822202",
            "label": { "name": "Galaxy Music" }
        }],
        "media": [{
            "format": "CD",
            "tracks": [
                { "position": 1, "title": "Track One Title", "length": 216000 },
                { "position": 2, "title": "Track Two Title", "length": 241000 }
            ]
        }],
        "relations": [{
            "url": { "resource": "https://www.discogs.com/release/67890" }
        }]
    }"#;

    let response: MbReleaseResponse = serde_json::from_str(json).unwrap();
    assert_eq!(response.id, "f9469bd8-a413-43f1-bee3-e3baabfb91cc");
    assert_eq!(response.title, "Super Hits of the 70s");
    assert_eq!(response.date.as_deref(), Some("2002"));
    assert!(response.country.is_none());
    assert_eq!(response.barcode.as_deref(), Some("8711638222024"));
    assert_eq!(response.artist_credit.len(), 1);
    assert_eq!(response.artist_credit[0].name, "All Star Cover Band");
    assert_eq!(response.media.len(), 1);
    assert_eq!(response.media[0].tracks.len(), 2);
    assert_eq!(
        response.media[0].tracks[0].title.as_deref(),
        Some("Track One Title")
    );
    assert_eq!(response.track_count(), 2);
    assert_eq!(response.label_info.len(), 1);
    assert_eq!(
        response.label_info[0].catalog_number.as_deref(),
        Some("3822202")
    );
    assert_eq!(response.relations.len(), 1);
}

#[test]
fn test_deserialize_mb_release_response_minimal() {
    // Minimal response with only required fields — all optional arrays absent
    let json = r#"{
        "id": "abc-123",
        "title": "Minimal Release"
    }"#;

    let response: MbReleaseResponse = serde_json::from_str(json).unwrap();
    assert_eq!(response.id, "abc-123");
    assert_eq!(response.title, "Minimal Release");
    assert!(response.date.is_none());
    assert!(response.artist_credit.is_empty());
    assert!(response.media.is_empty());
    assert!(response.relations.is_empty());
    assert_eq!(response.track_count(), 0);
}

// ── fetch_mb_xref ────────────────────────────────────────────────
//
// Drives the cross-reference path through cache seeding so the
// tests don't hit the network. Each test uses a unique Discogs
// release ID so other tests' cache seeds don't bleed in (the
// caches are process-global LRUs).

fn make_mb_response(id: &str, release_group_id: Option<&str>) -> MbReleaseResponse {
    MbReleaseResponse {
        id: id.to_string(),
        title: "Test Album".to_string(),
        date: None,
        country: None,
        barcode: None,
        artist_credit: vec![],
        release_group: release_group_id.map(|rg| MbReleaseGroupRef {
            id: rg.to_string(),
            first_release_date: None,
            relations: None,
        }),
        label_info: vec![],
        media: vec![],
        relations: vec![],
    }
}

fn empty_external_urls() -> ExternalUrls {
    ExternalUrls {
        discogs_release_url: None,
    }
}

#[test]
fn release_group_fallback_error_is_logged_with_group_id() {
    let mut urls = empty_external_urls();

    let logs = crate::test_logs::capture_warn_logs(|| {
        merge_release_group_external_urls(
            "rg-error",
            Err(MusicBrainzError::Other("transient fetch".to_string())),
            &mut urls,
        );
    });

    assert!(logs.contains("rg-error"));
    assert!(logs.contains("transient fetch"));
    assert!(urls.discogs_release_url.is_none());
}

#[tokio::test]
async fn test_fetch_mb_xref_with_backlink_returns_response_and_metadata() {
    let discogs_id = "fetch-mb-xref-hit-1";
    let mb_release_id = "mb-release-hit-1";
    let mb_group_id = "mb-group-hit-1";

    seed_discogs_url_lookup(discogs_id, Some(mb_release_id.to_string()));
    seed_release_cache(
        mb_release_id,
        (
            make_mb_response(mb_release_id, Some(mb_group_id)),
            empty_external_urls(),
            r#"{"id":"mb-release-hit-1"}"#.to_string(),
        ),
    );
    seed_release_group_json_cache(mb_group_id, r#"{"id":"mb-group-hit-1"}"#.to_string());

    let result = fetch_mb_xref(discogs_id).await;

    let (response, pairs) = result.expect("expected cross-link to be found");
    assert_eq!(response.id, mb_release_id);
    assert_eq!(
        response.release_group.as_ref().map(|rg| rg.id.as_str()),
        Some(mb_group_id)
    );
    // Two pairs: MB release JSON + MB release-group JSON.
    assert_eq!(pairs.len(), 2);
    assert_eq!(pairs[0].0, "musicbrainz");
    assert_eq!(pairs[1].0, "musicbrainz_release_group");
}

#[tokio::test]
async fn test_fetch_mb_xref_no_backlink_returns_none() {
    let discogs_id = "fetch-mb-xref-miss-1";
    seed_discogs_url_lookup(discogs_id, None);

    let result = fetch_mb_xref(discogs_id).await;

    assert!(
        result.is_none(),
        "expected None when MB has no back-link, got Some"
    );
}

#[tokio::test]
async fn test_fetch_mb_xref_release_without_group_still_returns_response() {
    // The mapper's identity-emission gate is checked at the mapper
    // layer (it only emits an MB identity row when `release_group`
    // is present). `fetch_mb_xref` itself returns whatever the MB
    // API gave us — the absence of a release group is not a
    // fetch-time failure.
    let discogs_id = "fetch-mb-xref-no-rg";
    let mb_release_id = "mb-release-no-rg";

    seed_discogs_url_lookup(discogs_id, Some(mb_release_id.to_string()));
    seed_release_cache(
        mb_release_id,
        (
            make_mb_response(mb_release_id, None),
            empty_external_urls(),
            r#"{"id":"mb-release-no-rg"}"#.to_string(),
        ),
    );

    let result = fetch_mb_xref(discogs_id).await;

    let (response, pairs) = result.expect("expected response even without release_group");
    assert_eq!(response.id, mb_release_id);
    assert!(response.release_group.is_none());
    // Only one pair (no release-group JSON to fetch).
    assert_eq!(pairs.len(), 1);
    assert_eq!(pairs[0].0, "musicbrainz");
}
