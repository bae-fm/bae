use super::*;
use serial_test::serial;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
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
fn first_discogs_release_url_skips_master_urls_and_missing_urls() {
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

    assert_eq!(
        first_discogs_release_url(&relations),
        Some("https://www.discogs.com/release/67890".to_string())
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
    assert!(crate::import::cover_art::musicbrainz_covers(&response).is_empty());
}

// ── fetch_mb_xref ────────────────────────────────────────────────
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

#[test]
fn release_group_fallback_error_is_logged_with_group_id() {
    let mut result = None;

    let logs = crate::test_logs::capture_warn_logs(|| {
        result = Some(release_group_discogs_url(
            "rg-error",
            Err(MusicBrainzError::Other("transient fetch".to_string())),
        ));
    });

    assert!(logs.contains("rg-error"));
    assert!(logs.contains("transient fetch"));
    assert!(result.expect("closure ran").is_none());
}

#[tokio::test]
#[serial(musicbrainz)]
async fn test_fetch_mb_xref_with_backlink_returns_response_and_metadata() {
    let discogs_id = "fetch-mb-xref-hit-1";
    let mb_release_id = "mb-release-hit-1";
    let mb_group_id = "mb-group-hit-1";

    seed_discogs_url_lookup(discogs_id, Some(mb_release_id.to_string()));
    seed_release_cache(
        mb_release_id,
        mb_release_json(&mb_release(mb_release_id, Some(mb_group_id))),
    );
    seed_release_group_json_cache(mb_group_id, r#"{"id":"mb-group-hit-1"}"#.to_string());

    let result = fetch_mb_xref(discogs_id, CallPriority::Interactive).await;

    let (response, pairs) = result.expect("expected cross-link to be found");
    assert_eq!(response.id, mb_release_id);
    assert_eq!(
        response.release_group.as_ref().map(|rg| rg.id.as_str()),
        Some(mb_group_id)
    );
    // Two documents: the MB release, re-keyed under the Discogs release the
    // lookup started from, and its release group under its own id.
    assert_eq!(pairs.len(), 2);
    assert_eq!(
        pairs[0].source,
        crate::import::PayloadSource::MusicBrainzDiscogsXref
    );
    assert_eq!(pairs[0].source_release_id, discogs_id);
    assert_eq!(
        pairs[1].source,
        crate::import::PayloadSource::MusicBrainzReleaseGroup
    );
    assert_eq!(pairs[1].source_release_id, mb_group_id);
}

#[tokio::test]
#[serial(musicbrainz)]
async fn test_fetch_mb_xref_no_backlink_returns_none() {
    let discogs_id = "fetch-mb-xref-miss-1";
    seed_discogs_url_lookup(discogs_id, None);

    let result = fetch_mb_xref(discogs_id, CallPriority::Interactive).await;

    assert!(
        result.is_none(),
        "expected None when MB has no back-link, got Some"
    );
}

#[tokio::test]
#[serial(musicbrainz)]
async fn test_fetch_mb_xref_release_without_group_still_returns_response() {
    // A missing release group is not a fetch-time failure: `fetch_mb_xref`
    // returns whatever MB gave it. The mapper is what gates on `release_group`,
    // and only emits an MB identity row when one is present.
    let discogs_id = "fetch-mb-xref-no-rg";
    let mb_release_id = "mb-release-no-rg";

    seed_discogs_url_lookup(discogs_id, Some(mb_release_id.to_string()));
    seed_release_cache(
        mb_release_id,
        mb_release_json(&mb_release(mb_release_id, None)),
    );

    let result = fetch_mb_xref(discogs_id, CallPriority::Interactive).await;

    let (response, pairs) = result.expect("expected response even without release_group");
    assert_eq!(response.id, mb_release_id);
    assert!(response.release_group.is_none());
    // Only one document (no release-group JSON to fetch).
    assert_eq!(pairs.len(), 1);
    assert_eq!(
        pairs[0].source,
        crate::import::PayloadSource::MusicBrainzDiscogsXref
    );
    assert_eq!(pairs[0].source_release_id, discogs_id);
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

// ── fetch_release_with_metadata ─────────────────────────────────────────────

/// The archival pairs every MB import path writes: the release under
/// `musicbrainz`, its release-group under `musicbrainz_release_group`. Both the
/// direct import and the Discogs cross-reference fetch through here, so a change
/// to this shape reaches both.
#[tokio::test]
#[serial(musicbrainz)]
async fn fetch_release_with_metadata_archives_release_and_group() {
    let release_id = "fetch-with-metadata-rel";
    let group_id = "fetch-with-metadata-group";
    let mut release = mb_release(release_id, Some(group_id));
    release.relations = vec![MbRelation {
        url: Some(MbUrlResource {
            resource: Some("https://www.discogs.com/release/1".to_string()),
        }),
        ..Default::default()
    }];
    let raw_json = mb_release_json(&release);
    seed_release_cache(release_id, raw_json.clone());
    seed_release_group_json_cache(group_id, r#"{"id":"group"}"#.to_string());

    let fetched = fetch_release_with_metadata(release_id, CallPriority::Interactive)
        .await
        .unwrap();

    assert_eq!(fetched.response.id, release_id);
    assert_eq!(
        fetched.discogs_url.as_deref(),
        Some("https://www.discogs.com/release/1")
    );
    assert_eq!(fetched.raw_json, raw_json);
    assert_eq!(
        fetched.release_group,
        Some(crate::import::SourcePayload::new(
            crate::import::PayloadSource::MusicBrainzReleaseGroup,
            group_id,
            r#"{"id":"group"}"#.to_string()
        ))
    );
}

/// A release with no release group archives just its own JSON. The group is
/// supplementary — its absence is not an import failure.
#[tokio::test]
#[serial(musicbrainz)]
async fn fetch_release_with_metadata_without_group_archives_only_the_release() {
    let release_id = "fetch-with-metadata-no-group";
    let raw_json = mb_release_json(&mb_release(release_id, None));
    seed_release_cache(release_id, raw_json.clone());

    let fetched = fetch_release_with_metadata(release_id, CallPriority::Interactive)
        .await
        .unwrap();

    assert_eq!(fetched.discogs_url, None);
    assert_eq!(fetched.raw_json, raw_json);
    assert_eq!(fetched.release_group, None);
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
    tokio::spawn(async move {
        let mut remaining = responses.into_iter();
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            counted_requests.fetch_add(1, Ordering::SeqCst);
            let mut buffer = [0; 4096];
            let _ = stream.read(&mut buffer).await;
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
    (url, request_count)
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
