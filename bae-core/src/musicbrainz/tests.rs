use super::*;
use serial_test::serial;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

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
        }],
        "cover-art-archive": {
            "count": 2, "artwork": true, "front": true, "back": true,
            "darkened": false
        }
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
    assert_eq!(response.label_info.len(), 1);
    assert_eq!(
        response.label_info[0].catalog_number.as_deref(),
        Some("3822202")
    );
    assert_eq!(response.relations.len(), 1);
    assert!(response.has_front_cover());
}

#[test]
fn test_deserialize_mb_release_response_minimal() {
    // Minimal response: every field the type requires, every optional array
    // absent. The `cover-art-archive` block is required because every endpoint
    // this type is parsed from — the release lookup, the disc-ID lookup, the
    // release browse — returns it.
    let json = r#"{
        "id": "abc-123",
        "title": "Minimal Release",
        "cover-art-archive": {
            "count": 0, "artwork": false, "front": false, "back": false,
            "darkened": false
        }
    }"#;

    let response: MbReleaseResponse = serde_json::from_str(json).unwrap();
    assert_eq!(response.id, "abc-123");
    assert_eq!(response.title, "Minimal Release");
    assert!(response.date.is_none());
    assert!(response.artist_credit.is_empty());
    assert!(response.media.is_empty());
    assert!(response.relations.is_empty());
    assert!(!response.has_front_cover());
}

/// A takedown darkens the whole release's art: the archive serves nothing for
/// it, whatever `front` says.
#[test]
fn a_darkened_release_serves_no_front_cover() {
    let json = r#"{
        "id": "darkened-1",
        "title": "Album Title",
        "cover-art-archive": {
            "count": 3, "artwork": true, "front": true, "back": false,
            "darkened": true
        }
    }"#;

    let response: MbReleaseResponse = serde_json::from_str(json).unwrap();
    assert!(!response.has_front_cover());
    assert!(crate::import::cover_art::musicbrainz_release_cover(&response).is_none());
}

// ── Provider response fixtures ─────────────────────────────────────────────
//
// Seeded responses, so no test hits the network. A seeded answer is keyed by
// the URL its request goes to, base address included, and that address is
// process-wide — so these are `#[serial(musicbrainz)]` against every test that
// points it somewhere else. Each test also uses ids of its own, to keep
// another test's seed from answering for it.

/// A bare MusicBrainz release document — no credits, no media, no cover art —
/// for the fetch paths, which only read its id and release group. Shared by
/// every seeded response below so they cannot drift apart.
fn mb_release(release_id: &str, release_group_id: Option<&str>) -> MbReleaseResponse {
    MbReleaseResponse {
        id: release_id.to_string(),
        title: "Album Title".to_string(),
        date: Some("1999".to_string()),
        country: None,
        barcode: None,
        artist_credit: vec![],
        release_group: release_group_id.map(|id| MbReleaseGroupRef {
            id: id.to_string(),
            first_release_date: None,
            relations: None,
        }),
        label_info: vec![],
        media: vec![],
        relations: vec![],
        cover_art_archive: crate::musicbrainz::MbCoverArtArchive {
            front: false,
            darkened: false,
        },
    }
}

/// A release document as the endpoint returns it — the bytes the client parses
/// and the import archives are the same bytes.
fn mb_release_json(release: &MbReleaseResponse) -> String {
    serde_json::to_string(release).expect("the test release serializes")
}

/// Only a failure a retry could fix is retried. A `NotFound` is the ordinary
/// answer for a disc MusicBrainz doesn't have, and each extra attempt costs
/// another round trip plus a 1s rate-limit wait; the "at least one search field"
/// error is raised before a request is even built.
#[test]
fn only_transient_musicbrainz_failures_are_retried() {
    assert!(should_retry_mb(&MusicBrainzError::Timeout));
    assert!(should_retry_mb(&MusicBrainzError::Network(
        "refused".into()
    )));
    assert!(should_retry_mb(&MusicBrainzError::Provider {
        status: Some(503)
    }));
    assert!(should_retry_mb(&MusicBrainzError::Provider {
        status: Some(429)
    }));

    assert!(!should_retry_mb(&MusicBrainzError::NotFound("disc".into())));
    assert!(!should_retry_mb(&MusicBrainzError::Provider {
        status: Some(404)
    }));
    assert!(!should_retry_mb(&MusicBrainzError::Provider {
        status: Some(400)
    }));
    assert!(!should_retry_mb(&MusicBrainzError::Other(
        "At least one search field must be provided".into()
    )));
}

// ── label_and_catno ────────────────────────────────────────────────────────

#[test]
fn label_and_catno_reads_the_first_label_info() {
    let label_info = vec![
        MbLabelInfo {
            label: Some(MbLabel {
                name: Some("First Label".to_string()),
            }),
            catalog_number: Some("CAT-1".to_string()),
        },
        MbLabelInfo {
            label: Some(MbLabel {
                name: Some("Second Label".to_string()),
            }),
            catalog_number: Some("CAT-2".to_string()),
        },
    ];
    assert_eq!(
        label_and_catno(&label_info),
        (Some("First Label".to_string()), Some("CAT-1".to_string()))
    );

    // No label info at all, and an entry with neither field, both read as unknown.
    assert_eq!(label_and_catno(&[]), (None, None));
    assert_eq!(
        label_and_catno(&[MbLabelInfo {
            label: None,
            catalog_number: None,
        }]),
        (None, None)
    );
}

// ── The response cache ──────────────────────────────────────────────────────
//
// Driven against a local server that answers the URLs the live service does and
// counts what was asked for, so "did this go to the wire?" is answered by a
// request count rather than by a stub.

/// A local HTTP server answering `responses` in order, then answering anything
/// further with a status no test expects — an over-count fails on the
/// assertion instead of hanging until the request timeout.
///
/// One connection per response: the stream is dropped once the body is written,
/// so the client opens a fresh connection for its next request and the accept
/// count is the request count.
async fn mb_response_server(responses: Vec<(u16, String)>) -> (String, Arc<AtomicUsize>) {
    let (url, count, _) = mb_recording_server(responses).await;
    (url, count)
}

async fn mb_recording_server(
    responses: Vec<(u16, String)>,
) -> (String, Arc<AtomicUsize>, Arc<Mutex<Vec<String>>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("test listener should bind");
    let url = format!(
        "http://{}",
        listener
            .local_addr()
            .expect("test listener should have an address")
    );
    let request_count = Arc::new(AtomicUsize::new(0));
    let counted_requests = request_count.clone();
    let paths = Arc::new(Mutex::new(Vec::new()));
    let captured = paths.clone();
    tokio::spawn(async move {
        let mut remaining = responses.into_iter();
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            counted_requests.fetch_add(1, Ordering::SeqCst);
            let mut buffer = [0; 4096];
            let count = stream.read(&mut buffer).await.expect("request is readable");
            let request = std::str::from_utf8(&buffer[..count]).expect("request header is UTF-8");
            captured.lock().unwrap().push(
                request
                    .lines()
                    .next()
                    .expect("request line exists")
                    .to_string(),
            );
            let (status, body) = remaining
                .next()
                .unwrap_or_else(|| (599, "unscripted request".to_string()));
            let raw = format!(
                "HTTP/1.1 {status} Response\r\nContent-Type: application/json\r\n\
                 Content-Length: {}\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(raw.as_bytes()).await;
        }
    });
    (url, request_count, paths)
}

/// Points MusicBrainz at a local server and restores the live address when the
/// test ends, panic included. The base URL and the rate limiter are both
/// process-wide, so every test holding one of these is `#[serial(musicbrainz)]`.
struct TestBase;

impl TestBase {
    fn point_at(url: &str) -> Self {
        BASE_URL.set_for_test(Some(url.to_string()));
        reset_rate_limiter_for_test();
        TestBase
    }
}

impl Drop for TestBase {
    fn drop(&mut self) {
        BASE_URL.set_for_test(None);
    }
}

/// A disc-ID response body carrying one release.
fn discid_body(release_id: &str) -> String {
    serde_json::json!({
        "releases": [serde_json::to_value(mb_release(release_id, None))
            .expect("the test release serializes")],
    })
    .to_string()
}

#[tokio::test]
#[serial(musicbrainz)]
async fn release_lookup_fetches_only_the_requested_document() {
    let raw = mb_release_json(&mb_release("release-only", Some("parent-group")));
    let (url, requests) = mb_response_server(vec![(200, raw.clone())]).await;
    let _base = TestBase::point_at(&url);

    let (release, archived) = lookup_release_by_id("release-only", CallPriority::Interactive)
        .await
        .expect("the release fetch succeeds independently of its parent");

    assert_eq!(release.id, "release-only");
    assert_eq!(archived, raw);
    assert_eq!(requests.load(Ordering::SeqCst), 1);
}

#[test]
fn discogs_url_answers_keep_all_matching_targets_and_their_entity_kind() {
    let raw = serde_json::json!({"relations": [
        {"type": "discogs", "target-type": "release", "release": {"id": "release-b"}},
        {"type": "discogs", "target-type": "release-group", "release-group": {"id": "group-a"}},
        {"type": "discogs", "target-type": "release", "release": {"id": "release-a"}},
        {"type": "discogs", "target-type": "release", "release": {"id": "release-b"}},
        {"type": "discogs", "target-type": "artist", "artist": {"id": "artist-a"}},
        {"type": "other", "target-type": "release", "release": {"id": "unrelated"}}
    ]})
    .to_string();

    assert_eq!(
        parse_discogs_release_lookup(&raw).unwrap(),
        vec![
            crate::import::CatalogPage::Release {
                catalog: crate::import::Catalog::MusicBrainz,
                key: "release-a".into()
            },
            crate::import::CatalogPage::Release {
                catalog: crate::import::Catalog::MusicBrainz,
                key: "release-b".into()
            },
        ]
    );
    assert_eq!(
        parse_discogs_master_lookup(&raw).unwrap(),
        vec![crate::import::CatalogPage::Group {
            catalog: crate::import::Catalog::MusicBrainz,
            key: "group-a".into()
        },]
    );
    assert!(parse_discogs_release_lookup(r#"{"relations":[]}"#)
        .unwrap()
        .is_empty());
    assert!(parse_discogs_master_lookup(r#"{"relations":[]}"#)
        .unwrap()
        .is_empty());
    assert!(parse_discogs_release_lookup("{}").is_err());
    assert!(parse_discogs_master_lookup(
        r#"{"relations":[{"type":"discogs","target-type":"release-group","release-group":{}}]}"#
    )
    .is_err());
}

#[tokio::test]
#[serial(musicbrainz)]
async fn reverse_release_and_master_lookups_preserve_raw_answers() {
    for (master, target) in [(false, "release"), (true, "release-group")] {
        let raw = serde_json::json!({"relations": [{
            "type": "discogs", "target-type": target, target: {"id": "linked-id"}
        }]})
        .to_string();
        let (url, requests, paths) = mb_recording_server(vec![(200, raw.clone())]).await;
        let _base = TestBase::point_at(&url);
        let result = if master {
            lookup_groups_by_discogs_master("510001", CallPriority::Interactive).await
        } else {
            lookup_releases_by_discogs_release("510001", CallPriority::Interactive).await
        }
        .unwrap()
        .expect("a URL document exists");
        assert_eq!(result.1, raw);
        assert_eq!(result.0.len(), 1);
        assert_eq!(requests.load(Ordering::SeqCst), 1);
        let request_line = paths.lock().unwrap()[0].clone();
        let path = request_line
            .split_whitespace()
            .nth(1)
            .expect("request names its URL");
        let requested = reqwest::Url::parse(&format!("{url}{path}")).unwrap();
        let query: std::collections::BTreeMap<_, _> =
            requested.query_pairs().into_owned().collect();
        assert_eq!(requested.path(), "/url");
        assert_eq!(
            query["resource"],
            if master {
                "https://www.discogs.com/master/510001"
            } else {
                "https://www.discogs.com/release/510001"
            }
        );
        assert_eq!(
            query["inc"],
            if master {
                "release-group-rels"
            } else {
                "release-rels"
            }
        );
        assert_eq!(query["fmt"], "json");
    }
}

#[tokio::test]
#[serial(musicbrainz)]
async fn reverse_lookup_distinguishes_absence_empty_answers_and_failures() {
    for master in [false, true] {
        for (status, body, expected) in [
            (404, "", "missing"),
            (200, r#"{"relations":[]}"#, "empty"),
            (400, "bad request", "provider"),
            (200, "broken JSON", "parse"),
        ] {
            let (url, requests) = mb_response_server(vec![(status, body.into())]).await;
            let _base = TestBase::point_at(&url);
            let result = if master {
                lookup_groups_by_discogs_master("510002", CallPriority::Interactive).await
            } else {
                lookup_releases_by_discogs_release("510002", CallPriority::Interactive).await
            };
            match expected {
                "missing" => assert!(result.unwrap().is_none()),
                "empty" => assert_eq!(result.unwrap(), Some((Vec::new(), body.into()))),
                "provider" => assert!(matches!(
                    result,
                    Err(MusicBrainzError::Provider { status: Some(400) })
                )),
                "parse" => assert!(matches!(result, Err(MusicBrainzError::Other(_)))),
                _ => unreachable!(),
            }
            assert_eq!(requests.load(Ordering::SeqCst), 1);
        }
    }
}

#[tokio::test]
#[serial(musicbrainz)]
async fn reverse_lookup_retries_transient_failures_and_caches_the_answer() {
    for master in [false, true] {
        let (url, requests) = mb_response_server(vec![
            (503, "unavailable".into()),
            (200, r#"{"relations":[]}"#.into()),
        ])
        .await;
        let _base = TestBase::point_at(&url);
        for _ in 0..2 {
            let answer = if master {
                lookup_groups_by_discogs_master("510003", CallPriority::Interactive).await
            } else {
                lookup_releases_by_discogs_release("510003", CallPriority::Interactive).await
            }
            .unwrap()
            .expect("the retry obtains the URL document");
            assert!(answer.0.is_empty());
        }
        assert_eq!(requests.load(Ordering::SeqCst), 2);
    }
}

#[test]
fn release_group_keeps_album_metadata_and_optional_absence() {
    let group = parse_release_group(
        r#"{
        "id":"group-one", "title":"Album Title", "first-release-date":"1982-04",
        "artist-credit":[{"name":"Artist Name","artist":{"id":"artist-one"}}],
        "relations":[{"url":{"resource":"https://www.discogs.com/master/42"}}]
    }"#,
    )
    .unwrap();
    assert_eq!(group.id, "group-one");
    assert_eq!(group.title.as_deref(), Some("Album Title"));
    assert_eq!(group.first_release_date.as_deref(), Some("1982-04"));
    assert_eq!(group.artist_credit[0].name, "Artist Name");
    assert_eq!(
        relation_urls(&group.relations).collect::<Vec<_>>(),
        ["https://www.discogs.com/master/42"]
    );

    let sparse =
        parse_release_group(r#"{"id":"group-empty","title":"","first-release-date":""}"#).unwrap();
    assert!(sparse.title.is_none());
    assert!(sparse.first_release_date.is_none());
    assert!(sparse.artist_credit.is_empty());
}

#[test]
fn blank_pressing_fields_are_absent_in_musicbrainz_documents() {
    for value in ["", " \t"] {
        let raw = serde_json::json!({
            "id":"release-empty", "title":"Album Title", "country":value, "barcode":value,
            "media":[{"format":value}],
            "label-info":[{"catalog-number":value,"label":{"name":value}}],
            "cover-art-archive":{"front":false,"darkened":false}
        })
        .to_string();
        let release: MbReleaseResponse = serde_json::from_str(&raw).unwrap();
        assert!(release.country.is_none());
        assert!(release.barcode.is_none());
        assert!(release.media[0].format.is_none());
        assert_eq!(label_and_catno(&release.label_info), (None, None));
    }
}

#[tokio::test]
#[serial(musicbrainz)]
async fn a_repeated_request_is_answered_without_a_second_round_trip() {
    let (url, requests) =
        mb_response_server(vec![(200, r#"{"id":"rg-repeat"}"#.to_string())]).await;
    let _base = TestBase::point_at(&url);

    let first = fetch_release_group_json("rg-repeat", CallPriority::Interactive)
        .await
        .expect("the release-group fetch succeeds");
    let second = fetch_release_group_json("rg-repeat", CallPriority::Interactive)
        .await
        .expect("the repeated fetch is answered");

    assert_eq!(first, r#"{"id":"rg-repeat"}"#);
    assert_eq!(second, first);
    assert_eq!(requests.load(Ordering::SeqCst), 1);
}

#[tokio::test]
#[serial(musicbrainz)]
async fn a_not_found_answer_is_kept() {
    let (url, requests) = mb_response_server(vec![(404, String::new())]).await;
    let _base = TestBase::point_at(&url);

    for _ in 0..2 {
        let error = lookup_by_discid("disc-not-found", CallPriority::Interactive)
            .await
            .expect_err("the disc is not in MusicBrainz");
        assert!(matches!(error, MusicBrainzError::NotFound(_)));
    }

    assert_eq!(requests.load(Ordering::SeqCst), 1);
}

/// A rate limit and a server error are the provider's momentary state, not its
/// answer: every retry goes to the wire, and so does the next call.
#[tokio::test]
#[serial(musicbrainz)]
async fn transient_failures_are_not_kept() {
    let (url, requests) = mb_response_server(vec![
        (429, String::new()),
        (503, String::new()),
        (429, String::new()),
        (200, discid_body("mb-after-transient")),
    ])
    .await;
    let _base = TestBase::point_at(&url);

    let error = lookup_by_discid("disc-transient", CallPriority::Interactive)
        .await
        .expect_err("three transient answers exhaust the retries");
    assert!(matches!(
        error,
        MusicBrainzError::Provider { status: Some(429) }
    ));
    assert_eq!(
        requests.load(Ordering::SeqCst),
        3,
        "each retry asked the server again"
    );

    let releases = lookup_by_discid("disc-transient", CallPriority::Interactive)
        .await
        .expect("the provider recovered");
    assert_eq!(releases[0].id, "mb-after-transient");
    assert_eq!(
        requests.load(Ordering::SeqCst),
        4,
        "the failed answer was not kept"
    );
}

/// The key is the whole URL, so the same path under two base addresses is two
/// answers — which is what keeps one test's fake provider out of another's.
#[tokio::test]
#[serial(musicbrainz)]
async fn the_same_path_under_two_base_urls_is_two_answers() {
    let (first_url, first_requests) =
        mb_response_server(vec![(200, r#"{"id":"first"}"#.to_string())]).await;
    let (second_url, second_requests) =
        mb_response_server(vec![(200, r#"{"id":"second"}"#.to_string())]).await;

    let first = {
        let _base = TestBase::point_at(&first_url);
        fetch_release_group_json("rg-two-bases", CallPriority::Interactive)
            .await
            .expect("the first server answers")
    };
    let second = {
        let _base = TestBase::point_at(&second_url);
        fetch_release_group_json("rg-two-bases", CallPriority::Interactive)
            .await
            .expect("the second server answers")
    };

    assert_eq!(first, r#"{"id":"first"}"#);
    assert_eq!(second, r#"{"id":"second"}"#);
    assert_eq!(first_requests.load(Ordering::SeqCst), 1);
    assert_eq!(second_requests.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn invalid_request_is_diagnostic_not_a_network_outage() {
    let error = mb_get(http_client().get("not a URL"), CallPriority::Interactive)
        .await
        .unwrap_err();
    assert!(matches!(&error, MusicBrainzError::Other(_)), "{error}");
    assert!(
        error.to_string().contains("RelativeUrlWithoutBase"),
        "{error}"
    );
}
