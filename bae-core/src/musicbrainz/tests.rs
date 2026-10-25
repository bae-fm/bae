use super::*;
use std::collections::HashMap;
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
        r#"artist:(+"Test" +"Artist") AND release:(+"Test" +"Album") AND date:2000"#,
    );
    let params2 = ReleaseSearchParams {
        artist: Some("Another Artist".to_string()),
        catalog_number: Some("TL-1234".to_string()),
        ..Default::default()
    };
    assert_eq!(
        params2.build_query(),
        r#"artist:(+"Another" +"Artist") AND catno:"TL-1234""#
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
    assert_eq!(
        params.build_query(),
        r#"artist:(+"Artist" +"Name") AND date:2000"#
    );
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
fn release_search_params_build_query_with_escaped_words() {
    let params = ReleaseSearchParams {
        artist: Some("Artist Name".to_string()),
        album: Some(r#"Quoted "Middle" Phrase\"#.to_string()),
        ..Default::default()
    };

    assert_eq!(
        params.build_query(),
        r#"artist:(+"Artist" +"Name") AND release:(+"Quoted" +"\"Middle\"" +"Phrase\\")"#,
    );
}

/// A catalog may write an album's words in another order than the folder —
/// "Album 1999" where the folder has "1999 Album" — and an artist's too, as
/// its sort name does: every word is asked for, in any order.
#[test]
fn a_title_and_an_artist_are_asked_for_by_every_word_in_any_order() {
    let params = ReleaseSearchParams {
        artist: Some("Artist, The".to_string()),
        album: Some("1999 Words Album".to_string()),
        ..Default::default()
    };
    assert_eq!(
        params.build_query(),
        r#"artist:(+"Artist,") AND release:(+"1999" +"Words" +"Album")"#,
    );
}

/// The words that say nothing about which album it is are not asked for,
/// unless they are all the title has.
#[test]
fn stop_words_are_not_asked_for() {
    let params = ReleaseSearchParams {
        album: Some("The Words of the Album".to_string()),
        ..Default::default()
    };
    assert_eq!(params.build_query(), r#"release:(+"Words" +"Album")"#);
    let params = ReleaseSearchParams {
        artist: Some("The The".to_string()),
        ..Default::default()
    };
    assert_eq!(params.build_query(), r#"artist:(+"The" +"The")"#);
}

/// A hyphenated word is one word to MusicBrainz: "1990-2000" is asked for
/// whole, not as 1990 and 2000.
#[test]
fn a_hyphenated_word_is_asked_for_whole() {
    let params = ReleaseSearchParams {
        album: Some("1990-2000 the Words".to_string()),
        ..Default::default()
    };
    assert_eq!(params.build_query(), r#"release:(+"1990-2000" +"Words")"#);
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
    // this type is parsed from — the release lookup and the disc-ID lookup —
    // returns it.
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
// Seeded responses, so no test hits the network.

/// A bare MusicBrainz release document — no credits, no media, no cover art —
/// for the fetch paths, which only read its id and release group. Shared by
/// every seeded response below so they cannot drift apart.
fn mb_release(release_id: &str, release_group_id: Option<&str>) -> MbReleaseResponse {
    MbReleaseResponse {
        id: release_id.to_string(),
        title: "Album Title".to_string(),
        disambiguation: None,
        annotation: None,
        date: Some("1999".to_string()),
        country: None,
        status: None,
        packaging: None,
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
#[tokio::test]
async fn only_transient_musicbrainz_failures_are_retried() {
    let provider = |status: Option<u16>, told_wait: Option<Duration>| {
        repeat_mb(&MusicBrainzError::Provider { status, told_wait })
    };
    assert_eq!(repeat_mb(&MusicBrainzError::Timeout), Repeat::AfterBackoff);
    assert_eq!(
        repeat_mb(&MusicBrainzError::Network(
            crate::util::http::refused_request_error().await
        )),
        Repeat::AfterBackoff
    );
    assert_eq!(provider(Some(503), None), Repeat::AfterBackoff);
    assert_eq!(
        provider(Some(503), Some(Duration::from_secs(3))),
        Repeat::AfterToldWait(Duration::from_secs(3)),
        "a stated wait replaces the backoff"
    );
    assert_eq!(provider(Some(429), None), Repeat::AfterBackoff);

    assert_eq!(
        repeat_mb(&MusicBrainzError::NotFound("disc".into())),
        Repeat::Never
    );
    assert_eq!(provider(Some(404), None), Repeat::Never);
    assert_eq!(provider(Some(400), None), Repeat::Never);
    assert_eq!(
        repeat_mb(&MusicBrainzError::Other(
            "At least one search field must be provided".into()
        )),
        Repeat::Never
    );
    assert_eq!(
        repeat_mb(&MusicBrainzError::Request(
            crate::util::http::unbuildable_request_error()
        )),
        Repeat::Never
    );
}

/// A lookup that never reached MusicBrainz names what failed underneath — the
/// DNS lookup, the refused connection — in the detail a surface shows, where
/// reqwest's own text says only that sending failed. The request's path and
/// query stay out of it.
#[tokio::test]
async fn a_network_failure_names_its_cause_in_the_detail_a_surface_shows() {
    for (origin, cause) in [
        ("http://bae-test.invalid", "dns error"),
        ("http://127.0.0.1:1", "tcp connect error"),
    ] {
        let musicbrainz = MusicBrainz::for_test(Http::for_test().serve_every_host(origin));
        let error = musicbrainz
            .lookup_by_discid("disc-unreachable", CallPriority::Interactive)
            .await
            .expect_err("nothing answers there");
        assert!(matches!(error, MusicBrainzError::Network(_)), "{error}");
        let crate::ui::UiError::Diagnostic { category, detail } =
            crate::import::ImportError::from(error).ui_error();
        assert_eq!(category, crate::ui::UiErrorCategory::Import);
        assert!(detail.contains(cause), "{detail}");
        assert!(!detail.contains("disc-unreachable"), "{detail}");
    }
}

// ── release_labels ────────────────────────────────────────────────────────

fn label_info(name: Option<&str>, catalog_number: Option<&str>) -> MbLabelInfo {
    MbLabelInfo {
        label: Some(MbLabel {
            name: name.map(str::to_string),
        }),
        catalog_number: catalog_number.map(str::to_string),
    }
}

fn stated(name: Option<&str>, catalog_number: Option<&str>) -> crate::pressing::ReleaseLabel {
    crate::pressing::ReleaseLabel::new(name.map(str::to_string), catalog_number.map(str::to_string))
        .expect("a stated label")
}

#[test]
fn release_labels_keeps_every_label_info_in_order() {
    let labels = release_labels(&[
        label_info(Some("Label A"), Some("AB 100")),
        label_info(Some("Label B"), Some("CL 719")),
        label_info(None, Some("XY 3")),
        label_info(Some("Label C"), None),
    ]);
    assert_eq!(
        labels,
        vec![
            stated(Some("Label A"), Some("AB 100")),
            stated(Some("Label B"), Some("CL 719")),
            stated(None, Some("XY 3")),
            stated(Some("Label C"), None),
        ]
    );
}

/// MusicBrainz writes a label with no catalog number as `[none]`, and its
/// data holds other spellings too: each keeps the label's name and no number.
#[test]
fn release_labels_reads_a_none_placeholder_as_no_number() {
    for placeholder in ["[none]", "none", "None", "[None]", "(none)", "[none"] {
        assert_eq!(
            release_labels(&[label_info(Some("Label A"), Some(placeholder))]),
            vec![stated(Some("Label A"), None)],
            "{placeholder}"
        );
        assert!(release_labels(&[label_info(None, Some(placeholder))]).is_empty());
    }
}

#[test]
fn release_labels_leaves_out_empty_and_repeated_entries() {
    assert!(release_labels(&[]).is_empty());
    assert_eq!(
        release_labels(&[
            MbLabelInfo {
                label: None,
                catalog_number: None,
            },
            label_info(Some("Label A"), Some("AB 100")),
            label_info(Some("Label A"), Some("AB 100")),
        ]),
        vec![stated(Some("Label A"), Some("AB 100"))]
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
pub(super) async fn mb_response_server(
    responses: Vec<(u16, String)>,
) -> (String, Arc<AtomicUsize>) {
    let (url, count, _) = mb_recording_server(responses).await;
    (url, count)
}

async fn mb_recording_server(
    responses: Vec<(u16, String)>,
) -> (String, Arc<AtomicUsize>, Arc<Mutex<Vec<String>>>) {
    mb_raw_server(
        responses
            .into_iter()
            .map(|(status, body)| {
                format!(
                    "HTTP/1.1 {status} Response\r\nContent-Type: application/json\r\n\
                     Content-Length: {}\r\n\r\n{body}",
                    body.len()
                )
            })
            .collect(),
    )
    .await
}

/// [`mb_recording_server`] answering whole raw responses, headers and all.
pub(super) async fn mb_raw_server(
    responses: Vec<String>,
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
            let raw = remaining.next().unwrap_or_else(|| {
                "HTTP/1.1 599 Response\r\nContent-Length: 18\r\n\r\nunscripted request".to_string()
            });
            #[expect(
                clippy::let_underscore_must_use,
                reason = "a client that stopped reading needs no answer"
            )]
            let _ = stream.write_all(raw.as_bytes()).await;
        }
    });
    (url, request_count, paths)
}

/// A client whose MusicBrainz requests go to the local server at `origin`.
pub(super) fn served_by(origin: &str) -> MusicBrainz {
    MusicBrainz::for_test(Http::for_test().serve("musicbrainz.org", origin))
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
async fn release_lookup_fetches_only_the_requested_document() {
    let raw = mb_release_json(&mb_release("release-only", Some("parent-group")));
    let (url, requests) = mb_response_server(vec![(200, raw.clone())]).await;
    let musicbrainz = served_by(&url);

    let (release, archived) = musicbrainz
        .lookup_release_by_id("release-only", CallPriority::Interactive)
        .await
        .expect("the release fetch succeeds independently of its parent");

    assert_eq!(release.id, "release-only");
    assert_eq!(archived, raw);
    assert_eq!(requests.load(Ordering::SeqCst), 1);
}

fn release_page(id: &str) -> DiscogsPage {
    DiscogsPage::Release(id.into())
}

fn master_page(id: &str) -> DiscogsPage {
    DiscogsPage::Master(id.into())
}

#[test]
fn discogs_url_answers_keep_all_matching_targets_of_the_page_s_kind() {
    let relations = serde_json::json!([
        {"type": "discogs", "target-type": "release", "release": {"id": "release-b"}},
        {"type": "discogs", "target-type": "release_group", "release_group": {"id": "group-a"}},
        {"type": "discogs", "target-type": "release", "release": {"id": "release-a"}},
        {"type": "discogs", "target-type": "release", "release": {"id": "release-b"}},
        {"type": "discogs", "target-type": "artist", "artist": {"id": "artist-a"}},
        {"type": "other", "target-type": "release", "release": {"id": "unrelated"}}
    ]);
    let raw = serde_json::json!({"relations": relations}).to_string();

    assert_eq!(
        parse_discogs_lookup(&[release_page("1")], &raw).unwrap(),
        HashMap::from([(
            release_page("1"),
            vec!["release-a".into(), "release-b".into()]
        )])
    );
    assert_eq!(
        parse_discogs_lookup(&[master_page("2")], &raw).unwrap(),
        HashMap::from([(master_page("2"), vec!["group-a".into()])])
    );
    assert_eq!(
        parse_discogs_lookup(&[release_page("1")], r#"{"relations":[]}"#).unwrap(),
        HashMap::from([(release_page("1"), Vec::new())])
    );
    assert!(parse_discogs_lookup(&[release_page("1")], "{}").is_err());
    assert!(parse_discogs_lookup(
        &[master_page("2")],
        r#"{"relations":[{"type":"discogs","target-type":"release_group","release_group":{}}]}"#
    )
    .is_err());
}

/// Several pages are answered with a document for each one MusicBrainz knows,
/// naming its address; one it does not know is left out, and a document for
/// an address nobody asked about is not an answer to the question.
#[test]
fn several_pages_are_answered_by_address() {
    let asked = [release_page("1"), master_page("2"), release_page("3")];
    let raw = serde_json::json!({"url-count": 2, "url-offset": 0, "urls": [
        {"resource": "https://www.discogs.com/master/2", "relations": [
            {"type": "discogs", "target-type": "release_group", "release_group": {"id": "group-a"}}
        ]},
        {"resource": "https://www.discogs.com/release/1", "relations": [
            {"type": "discogs", "target-type": "release", "release": {"id": "release-a"}}
        ]}
    ]})
    .to_string();
    assert_eq!(
        parse_discogs_lookup(&asked, &raw).unwrap(),
        HashMap::from([
            (release_page("1"), vec!["release-a".into()]),
            (master_page("2"), vec!["group-a".into()]),
        ])
    );
    assert!(parse_discogs_lookup(&asked[..2], r#"{"urls":[]}"#)
        .unwrap()
        .is_empty());
    assert!(parse_discogs_lookup(
        &asked[..2],
        r#"{"urls":[{"resource":"https://www.discogs.com/release/9","relations":[]}]}"#
    )
    .is_err());
}

/// One page is asked about with the relationships its kind needs; several
/// together in one request, each address named, with every kind's
/// relationships.
#[tokio::test]
async fn a_lookup_names_every_page_s_address_in_one_request() {
    for (pages, resources, include) in [
        (
            vec![release_page("510001")],
            vec!["https://www.discogs.com/release/510001"],
            "release-rels",
        ),
        (
            vec![master_page("510001")],
            vec!["https://www.discogs.com/master/510001"],
            "release-group-rels",
        ),
        (
            vec![master_page("510002"), release_page("510001")],
            vec![
                "https://www.discogs.com/release/510001",
                "https://www.discogs.com/master/510002",
            ],
            "release-group-rels release-rels",
        ),
    ] {
        let raw = match pages.as_slice() {
            [_] => r#"{"relations":[]}"#.to_string(),
            _ => r#"{"urls":[]}"#.to_string(),
        };
        let (url, requests, paths) = mb_recording_server(vec![(200, raw)]).await;
        served_by(&url)
            .lookup_discogs_pages(&pages, CallPriority::Interactive)
            .await
            .unwrap();
        assert_eq!(requests.load(Ordering::SeqCst), 1);
        let request_line = paths.lock().unwrap()[0].clone();
        let path = request_line
            .split_whitespace()
            .nth(1)
            .expect("request names its URL");
        let requested = reqwest::Url::parse(&format!("{url}{path}")).unwrap();
        assert_eq!(requested.path(), "/ws/2/url");
        let query: Vec<(String, String)> = requested.query_pairs().into_owned().collect();
        let asked: Vec<&str> = query
            .iter()
            .filter(|(key, _)| key == "resource")
            .map(|(_, value)| value.as_str())
            .collect();
        assert_eq!(asked, resources);
        assert!(query.contains(&("inc".into(), include.into())), "{query:?}");
        assert!(query.contains(&("fmt".into(), "json".into())), "{query:?}");
    }
}

#[tokio::test]
async fn a_lookup_distinguishes_absence_empty_answers_and_failures() {
    for pages in [
        vec![release_page("510002")],
        vec![release_page("510002"), master_page("510003")],
    ] {
        let empty = match pages.as_slice() {
            [_] => r#"{"relations":[]}"#,
            _ => {
                r#"{"urls":[{"resource":"https://www.discogs.com/release/510002","relations":[]}]}"#
            }
        };
        for (status, body, expected) in [
            (404, "", "missing"),
            (200, empty, "empty"),
            (400, "bad request", "provider"),
            (200, "broken JSON", "parse"),
        ] {
            let (url, requests) = mb_response_server(vec![(status, body.into())]).await;
            let result = served_by(&url)
                .lookup_discogs_pages(&pages, CallPriority::Interactive)
                .await;
            match expected {
                "missing" => assert!(result.unwrap().is_empty()),
                "empty" => assert_eq!(
                    result.unwrap(),
                    HashMap::from([(release_page("510002"), Vec::new())])
                ),
                "provider" => assert!(matches!(
                    result,
                    Err(MusicBrainzError::Provider {
                        status: Some(400),
                        told_wait: None,
                    })
                )),
                "parse" => assert!(matches!(result, Err(MusicBrainzError::Other(_)))),
                _ => unreachable!(),
            }
            assert_eq!(requests.load(Ordering::SeqCst), 1);
        }
    }
}

/// A lookup is retried after a transient failure, and its answer is kept for
/// the same pages asked again in any order.
#[tokio::test]
async fn a_lookup_retries_transient_failures_and_caches_the_answer() {
    let (url, requests) = mb_response_server(vec![
        (503, "unavailable".into()),
        (200, r#"{"urls":[]}"#.into()),
    ])
    .await;
    let musicbrainz = served_by(&url);
    for pages in [
        [release_page("510004"), master_page("510005")],
        [master_page("510005"), release_page("510004")],
    ] {
        let answer = musicbrainz
            .lookup_discogs_pages(&pages, CallPriority::Interactive)
            .await
            .expect("the retry obtains the URL documents");
        assert!(answer.is_empty());
    }
    assert_eq!(requests.load(Ordering::SeqCst), 2);
}

/// What a test seeds for asking about several pages together is what a
/// lookup of those pages reads back, with no request.
#[tokio::test]
async fn a_seeded_lookup_is_read_back_without_a_request() {
    let (url, requests) = mb_response_server(Vec::new()).await;
    let musicbrainz = served_by(&url);
    musicbrainz.seed_discogs_page_lookup(&[
        (release_page("510006"), Some("release-a")),
        (master_page("510007"), None),
    ]);
    musicbrainz.seed_discogs_page_lookup(&[(master_page("510007"), Some("group-a"))]);
    assert_eq!(
        musicbrainz
            .lookup_discogs_pages(
                &[master_page("510007"), release_page("510006")],
                CallPriority::Interactive
            )
            .await
            .unwrap(),
        HashMap::from([(release_page("510006"), vec!["release-a".into()])])
    );
    assert_eq!(
        musicbrainz
            .lookup_discogs_pages(&[master_page("510007")], CallPriority::Interactive)
            .await
            .unwrap(),
        HashMap::from([(master_page("510007"), vec!["group-a".into()])])
    );
    assert_eq!(requests.load(Ordering::SeqCst), 0);
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
        assert!(release_labels(&release.label_info).is_empty());
    }
}

#[tokio::test]
async fn a_repeated_request_is_answered_without_a_second_round_trip() {
    let (url, requests) =
        mb_response_server(vec![(200, r#"{"id":"rg-repeat"}"#.to_string())]).await;
    let musicbrainz = served_by(&url);

    let first = musicbrainz
        .fetch_release_group_json("rg-repeat", CallPriority::Interactive)
        .await
        .expect("the release-group fetch succeeds");
    let second = musicbrainz
        .fetch_release_group_json("rg-repeat", CallPriority::Interactive)
        .await
        .expect("the repeated fetch is answered");

    assert_eq!(first, r#"{"id":"rg-repeat"}"#);
    assert_eq!(second, first);
    assert_eq!(requests.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_not_found_answer_is_kept() {
    let (url, requests) = mb_response_server(vec![(404, String::new())]).await;
    let musicbrainz = served_by(&url);

    for _ in 0..2 {
        let error = musicbrainz
            .lookup_by_discid("disc-not-found", CallPriority::Interactive)
            .await
            .expect_err("the disc is not in MusicBrainz");
        assert!(matches!(error, MusicBrainzError::NotFound(_)));
    }

    assert_eq!(requests.load(Ordering::SeqCst), 1);
}

/// A rate limit and a server error are the provider's momentary state, not its
/// answer: every retry goes to the wire, and so does the next call.
#[tokio::test]
async fn transient_failures_are_not_kept() {
    let attempts = RETRY.attempts() as usize;
    // Every try but the last is a server error; the last is a rate limit.
    let mut script: Vec<(u16, String)> = (1..attempts).map(|_| (503, String::new())).collect();
    script.push((429, String::new()));
    script.push((200, discid_body("mb-after-transient")));
    let (url, requests) = mb_response_server(script).await;
    let musicbrainz = served_by(&url);

    let error = musicbrainz
        .lookup_by_discid("disc-transient", CallPriority::Interactive)
        .await
        .expect_err("transient answers on every try exhaust the retries");
    assert!(matches!(
        error,
        MusicBrainzError::Provider {
            status: Some(429),
            ..
        }
    ));
    assert_eq!(
        requests.load(Ordering::SeqCst),
        attempts,
        "each retry asked the server again"
    );

    let releases = musicbrainz
        .lookup_by_discid("disc-transient", CallPriority::Interactive)
        .await
        .expect("the provider recovered");
    assert_eq!(releases[0].0.id, "mb-after-transient");
    assert_eq!(
        requests.load(Ordering::SeqCst),
        attempts + 1,
        "the failed answer was not kept"
    );
}

/// Each client keeps its own answers: two clients asking the same URL each
/// ask their own server.
#[tokio::test]
async fn two_clients_keep_their_own_answers() {
    let (first_url, first_requests) =
        mb_response_server(vec![(200, r#"{"id":"first"}"#.to_string())]).await;
    let (second_url, second_requests) =
        mb_response_server(vec![(200, r#"{"id":"second"}"#.to_string())]).await;

    let first = served_by(&first_url)
        .fetch_release_group_json("rg-two-bases", CallPriority::Interactive)
        .await
        .expect("the first server answers");
    let second = served_by(&second_url)
        .fetch_release_group_json("rg-two-bases", CallPriority::Interactive)
        .await
        .expect("the second server answers");

    assert_eq!(first, r#"{"id":"first"}"#);
    assert_eq!(second, r#"{"id":"second"}"#);
    assert_eq!(first_requests.load(Ordering::SeqCst), 1);
    assert_eq!(second_requests.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn invalid_request_is_diagnostic_not_a_network_outage() {
    let musicbrainz = MusicBrainz::for_test(Http::for_test());
    let error = musicbrainz
        .get("not a URL", CallPriority::Interactive)
        .await
        .unwrap_err();
    assert!(matches!(&error, MusicBrainzError::Request(_)), "{error}");
    assert!(
        error.to_string().contains("relative URL without a base"),
        "{error}"
    );
}

/// A Discogs master's URL document as MusicBrainz serves it: the target type
/// and its object are spelled `release_group`, not `release-group`.
#[test]
fn a_master_lookup_reads_the_group_musicbrainz_names() {
    let served = r#"{"resource":"https://www.discogs.com/master/269277","relations":[{"target-credit":"","end":null,"type-id":"99e550f3-5ab4-3110-b5b9-fe01d970b126","begin":null,"release_group":{"title":"Album","first-release-date":"1965","disambiguation":"","id":"30c2eb28-a463-38ab-9f34-018cef8a5cfc","secondary-types":[],"primary-type":null,"secondary-type-ids":[],"primary-type-id":null},"attribute-values":{},"attribute-ids":{},"ended":false,"target-type":"release_group","direction":"backward","type":"discogs","attributes":[],"source-credit":""}],"id":"c7bc7064-875b-447f-a01b-039e316e0f26"}"#;

    assert_eq!(
        parse_discogs_lookup(&[master_page("269277")], served).unwrap(),
        HashMap::from([(
            master_page("269277"),
            vec!["30c2eb28-a463-38ab-9f34-018cef8a5cfc".to_string()]
        )])
    );
}

/// A recording search page answering `count` matches in all, with recordings
/// registered under `isrcs`, one each.
fn recording_page(count: usize, isrcs: &[&str]) -> String {
    serde_json::json!({
        "count": count,
        "recordings": isrcs
            .iter()
            .map(|isrc| serde_json::json!({
                "isrcs": [isrc],
                "releases": [{"id": format!("rel-{isrc}"), "title": "Album Title"}],
            }))
            .collect::<Vec<_>>(),
    })
    .to_string()
}

/// Every code is asked in one search, and the pages are read until every
/// match is in.
#[tokio::test]
async fn an_isrc_search_asks_every_code_at_once_and_reads_every_page() {
    let (url, requests, paths) = mb_recording_server(vec![
        (200, recording_page(3, &["XX0000000001", "XX0000000002"])),
        (200, recording_page(3, &["XX0000000003"])),
    ])
    .await;
    let musicbrainz = served_by(&url);

    let recordings = musicbrainz
        .search_recordings_by_isrcs(
            &[
                "XX0000000001".to_string(),
                "XX0000000002".to_string(),
                "XX0000000003".to_string(),
            ],
            CallPriority::Interactive,
        )
        .await
        .expect("the search answers");

    assert_eq!(
        recordings
            .iter()
            .flat_map(|recording| &recording.isrcs)
            .collect::<Vec<_>>(),
        vec!["XX0000000001", "XX0000000002", "XX0000000003"]
    );
    assert_eq!(requests.load(Ordering::SeqCst), 2);
    let paths = paths.lock().unwrap();
    assert!(
        paths[0].contains("query=isrc%3A%28XX0000000001+OR+XX0000000002+OR+XX0000000003%29"),
        "{}",
        paths[0]
    );
    assert!(paths[0].contains("offset=0"), "{}", paths[0]);
    assert!(paths[1].contains("offset=2"), "{}", paths[1]);
}
