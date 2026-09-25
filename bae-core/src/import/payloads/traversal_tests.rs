use super::*;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// A local server answering MusicBrainz and Discogs requests by path, and the
/// providers whose requests reach it.
struct ProviderServer {
    requests: Arc<Mutex<HashMap<String, usize>>>,
    task: tokio::task::JoinHandle<()>,
    providers: crate::providers::Providers,
}

impl ProviderServer {
    /// Answers are keyed by request path, and a MusicBrainz URL lookup by
    /// `url:` and the resource it asks about. Anything else is answered 599.
    async fn start(answers: HashMap<String, (u16, String)>) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(HashMap::new()));
        let recorded = requests.clone();
        let task = tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut buffer = [0; 4096];
                let length = stream.read(&mut buffer).await.unwrap();
                let request = std::str::from_utf8(&buffer[..length]).unwrap();
                let path = request.split_whitespace().nth(1).unwrap();
                let url = reqwest::Url::parse(&format!("http://fixture.example{path}")).unwrap();
                let key = if url.path() == "/ws/2/url" {
                    let resource = url
                        .query_pairs()
                        .find(|(key, _)| key == "resource")
                        .unwrap()
                        .1
                        .into_owned();
                    format!("url:{resource}")
                } else {
                    url.path().to_owned()
                };
                *recorded.lock().unwrap().entry(key.clone()).or_insert(0) += 1;
                let (status, body) = match answers.get(&key) {
                    Some(answer) => answer.clone(),
                    None => (599, format!("Unexpected provider request: {key}")),
                };
                let response = format!("HTTP/1.1 {status} Response\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        });
        let http = crate::util::http::Http::for_test()
            .serve("musicbrainz.org", &origin)
            .serve("api.discogs.com", &origin);
        Self {
            requests,
            task,
            providers: crate::providers::Providers::for_test(http),
        }
    }

    fn requests(&self) -> HashMap<String, usize> {
        self.requests.lock().unwrap().clone()
    }
}

impl Drop for ProviderServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn release_json(id: &str, group: Option<&str>, urls: &[&str]) -> String {
    serde_json::json!({
        "id": id, "title": "Album Title",
        "release-group": group.map(|id| serde_json::json!({"id":id})),
        "relations": urls.iter().map(|url| serde_json::json!({"url":{"resource":url}})).collect::<Vec<_>>(),
        "cover-art-archive":{"front":false,"darkened":false}
    }).to_string()
}

#[tokio::test]
async fn failed_canonical_group_is_not_requested_again_through_another_alias() {
    // A 400 is neither cached nor retried by the provider, so its HTTP count
    // measures traversal's own duplicate-request protection.
    let server = ProviderServer::start(HashMap::from([(
        "/ws/2/release-group/shared-group".into(),
        (400, "Rejected group request".into()),
    )]))
    .await;
    server.providers.musicbrainz().seed_release_cache(
        "selected",
        release_json(
            "selected",
            None,
            &[
                "https://www.discogs.com/master/910001",
                "https://www.discogs.com/master/910002",
            ],
        ),
    );
    for id in [910001, 910002] {
        server.providers.discogs().seed_master_cache(
            &id.to_string(),
            serde_json::json!({"id":id}).to_string(),
        );
        server.providers.musicbrainz().seed_discogs_master_url_lookup(
            &id.to_string(),
            Some("shared-group".into()),
        );
    }
    let discogs = DiscogsClient::new(server.providers.discogs().clone(), "fixture-token".into());
    let payloads = server.providers.fetch_payloads(Some(&discogs), &MetadataRef::new(Catalog::MusicBrainz, "selected"), CallPriority::Interactive)
    .await
    .unwrap();

    assert_eq!(
        server.requests(),
        HashMap::from([("/ws/2/release-group/shared-group".into(), 1)])
    );
    assert_eq!(payloads.supporting.len(), 2);
    assert!(payloads
        .supporting
        .iter()
        .all(|document| document.source == PayloadSource::DiscogsMaster));
    assert_eq!(
        payloads
            .extract().unwrap()
            .records()
            .iter()
            .filter(|record| record.catalog() == Catalog::MusicBrainz)
            .count(),
        1
    );
}

#[tokio::test]
async fn cyclic_release_and_album_links_fetch_each_document_once() {
    let release = release_json(
        "cycle-release",
        Some("cycle-group"),
        &[
            "https://www.discogs.com/release/920001",
            "https://www.discogs.com/release/920001",
        ],
    );
    let group = serde_json::json!({"id":"cycle-group", "relations":[
        {"url":{"resource":"https://www.discogs.com/master/920002"}},
        {"url":{"resource":"https://www.discogs.com/master/920002"}}
    ]})
    .to_string();
    let answers = HashMap::from([
        ("/ws/2/release/cycle-release".into(), (200, release)),
        ("/ws/2/release-group/cycle-group".into(), (200, group)),
        ("/releases/920001".into(), (200, serde_json::json!({"id":920001,"title":"Album Title","master_id":920002}).to_string())),
        ("/masters/920002".into(), (200, serde_json::json!({"id":920002,"title":"Album Title"}).to_string())),
        ("url:https://www.discogs.com/release/920001".into(), (200, serde_json::json!({"relations":[
            {"type":"discogs","target-type":"release","release":{"id":"cycle-release"}}
        ]}).to_string())),
        ("url:https://www.discogs.com/master/920002".into(), (200, serde_json::json!({"relations":[
            {"type":"discogs","target-type":"release_group","release_group":{"id":"cycle-group"}}
        ]}).to_string())),
    ]);
    let expected: HashMap<_, _> = answers.keys().cloned().map(|key| (key, 1)).collect();
    let server = ProviderServer::start(answers).await;
    let discogs = DiscogsClient::new(server.providers.discogs().clone(), "fixture-token".into());
    let selected = MetadataRef::new(Catalog::MusicBrainz, "cycle-release");
    let payloads = tokio::time::timeout(
        std::time::Duration::from_secs(15),
        server.providers.fetch_payloads(Some(&discogs), &selected, CallPriority::Interactive),
    )
    .await
    .unwrap()
    .unwrap();

    assert_eq!(server.requests(), expected);
    let mut fetched = HashMap::from([(
        (PayloadSource::MusicBrainz, selected.key.clone()),
        payloads.anchor.clone(),
    )]);
    for document in &payloads.supporting {
        assert!(
            fetched
                .insert(
                    (document.source, document.source_release_id.clone()),
                    document.json.clone()
                )
                .is_none(),
            "each fetched key occurs once"
        );
    }
    assert_eq!(fetched.len(), 6);
}

#[tokio::test]
async fn ambiguous_master_backlinks_do_not_fetch_or_claim_either_album() {
    let server = ProviderServer::start(HashMap::from([(
        "url:https://www.discogs.com/master/930002".into(),
        (
            200,
            serde_json::json!({"relations":[
                {"type":"discogs","target-type":"release_group","release_group":{"id":"group-a"}},
                {"type":"discogs","target-type":"release_group","release_group":{"id":"group-b"}}
            ]})
            .to_string(),
        ),
    )]))
    .await;
    server.providers.discogs().seed_release_cache(
        "930001",
        serde_json::json!({"id":930001,"title":"Album Title","master_id":930002}).to_string(),
    );
    server.providers.discogs().seed_master_cache(
        "930002",
        serde_json::json!({"id":930002}).to_string(),
    );
    server.providers.musicbrainz().seed_discogs_url_lookup("930001", None);
    let discogs = DiscogsClient::new(server.providers.discogs().clone(), "fixture-token".into());
    let selected = MetadataRef::new(Catalog::Discogs, "930001");
    let payloads = server.providers.fetch_payloads(Some(&discogs), &selected, CallPriority::Interactive)
        .await
        .unwrap();

    assert_eq!(
        server.requests(),
        HashMap::from([("url:https://www.discogs.com/master/930002".into(), 1)])
    );
    assert_eq!(payloads.supporting.len(), 1);
    assert_eq!(payloads.supporting[0].source, PayloadSource::DiscogsMaster);
    assert!(payloads
        .extract().unwrap()
        .records()
        .iter()
        .all(|record| record.catalog() == Catalog::Discogs));
}

#[tokio::test]
async fn a_malformed_optional_group_is_skipped() {
    let selected = MetadataRef::new(Catalog::MusicBrainz, "optional-shape-release");
    let anchor = release_json(&selected.key, Some("optional-shape-group"), &[]);
    let server = ProviderServer::start(HashMap::from([
        (
            "/ws/2/release/optional-shape-release".into(),
            (200, anchor.clone()),
        ),
        (
            "/ws/2/release-group/optional-shape-group".into(),
            (200, "{}".into()),
        ),
    ]))
    .await;
    let fetched = server
        .providers
        .fetch_payloads(None, &selected, CallPriority::Interactive)
        .await
        .unwrap();
    assert!(fetched.supporting.is_empty());
    assert_eq!(
        fetched.unfetched,
        vec![crate::import::source_release::UnfetchedDocument {
            document: PayloadSource::MusicBrainzReleaseGroup,
            key: "optional-shape-group".into(),
            reason: UnfetchedReason::Failed,
        }],
        "a group that does not read is a failed answer, asked again next time"
    );
    assert_eq!(
        server.requests(),
        HashMap::from([
            ("/ws/2/release/optional-shape-release".into(), 1),
            ("/ws/2/release-group/optional-shape-group".into(), 1),
        ])
    );
}

/// A reverse cross-reference whose document does not read is skipped, and
/// the release keeps its own record.
#[tokio::test]
async fn a_malformed_reverse_alias_is_skipped() {
    let server = ProviderServer::start(HashMap::new()).await;
    server.providers.discogs().seed_release_cache(
        "940001",
        serde_json::json!({"id":940001,"title":"Selected Album"}).to_string(),
    );
    server
        .providers
        .musicbrainz()
        .seed_discogs_url_lookup("940001", Some("mb-alias".into()));
    server
        .providers
        .musicbrainz()
        .seed_release_cache("mb-alias", "{}".into());
    let discogs = DiscogsClient::new(server.providers.discogs().clone(), "fixture-token".into());
    let fetched = server
        .providers
        .fetch_payloads(
            Some(&discogs),
            &MetadataRef::new(Catalog::Discogs, "940001"),
            CallPriority::Interactive,
        )
        .await
        .unwrap();
    assert!(fetched.supporting.is_empty());
    assert_eq!(fetched.extract().unwrap().records().len(), 1);
    assert!(server.requests().is_empty());
}

/// What a fetch followed a link to and did not get is named with the
/// release: a source that failed, and a Discogs document with no key to ask
/// with. A catalog answering that there is none is not missing anything.
#[tokio::test]
async fn documents_a_fetch_could_not_get_are_named() {
    let selected = MetadataRef::new(Catalog::MusicBrainz, "missing-parts-release");
    let server = ProviderServer::start(HashMap::from([
        (
            "/ws/2/release/missing-parts-release".into(),
            (
                200,
                release_json(
                    &selected.key,
                    Some("missing-parts-group"),
                    &["https://www.discogs.com/release/950001"],
                ),
            ),
        ),
        (
            "/ws/2/release-group/missing-parts-group".into(),
            (400, "Rejected group request".into()),
        ),
    ]))
    .await;
    let fetched = server
        .providers
        .fetch_payloads(None, &selected, CallPriority::Interactive)
        .await
        .unwrap();
    let mut unfetched = fetched.unfetched.clone();
    unfetched.sort_by_key(|document| document.key.clone());
    assert_eq!(
        unfetched,
        vec![
            crate::import::source_release::UnfetchedDocument {
                document: PayloadSource::Discogs,
                key: "950001".into(),
                reason: UnfetchedReason::DiscogsNotConfigured,
            },
            crate::import::source_release::UnfetchedDocument {
                document: PayloadSource::MusicBrainzReleaseGroup,
                key: "missing-parts-group".into(),
                reason: UnfetchedReason::Failed,
            },
        ]
    );
    let stored = fetched.extract().unwrap();
    assert!(stored.fetch_could_add(false), "the failed group can be asked again");
    let mut only_the_key_missing = stored.clone();
    only_the_key_missing
        .unfetched
        .retain(|document| document.reason == UnfetchedReason::DiscogsNotConfigured);
    assert!(
        !only_the_key_missing.fetch_could_add(false),
        "without a key, asking again gets no Discogs document"
    );
    assert!(
        only_the_key_missing.fetch_could_add(true),
        "with a key, the Discogs document can be asked for"
    );
}

#[tokio::test]
async fn a_malformed_required_anchor_is_rejected() {
    let selected = MetadataRef::new(Catalog::MusicBrainz, "required-shape-release");
    let server = ProviderServer::start(HashMap::from([(
        "/ws/2/release/required-shape-release".into(),
        (200, "{}".into()),
    )]))
    .await;
    assert!(
        server.providers.fetch_payloads(None, &selected, CallPriority::Interactive)
            .await
            .is_err()
    );
    assert_eq!(
        server.requests(),
        HashMap::from([("/ws/2/release/required-shape-release".into(), 1)])
    );
}
