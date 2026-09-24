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

struct MemoryArchive(HashMap<DocumentKey, String>);

impl ArchivedDocuments for MemoryArchive {
    fn document(&self, source: PayloadSource, id: &str) -> Result<Option<String>, ImportError> {
        Ok(self.0.get(&(source, id.to_owned())).cloned())
    }
}

#[tokio::test]
async fn cyclic_release_and_album_links_fetch_each_document_once_and_replay() {
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
    let mut archived = HashMap::from([(
        (PayloadSource::MusicBrainz, selected.key.clone()),
        payloads.anchor.clone(),
    )]);
    for document in &payloads.supporting {
        assert!(
            archived
                .insert(
                    (document.source, document.source_release_id.clone()),
                    document.json.clone()
                )
                .is_none(),
            "each archived key occurs once"
        );
    }
    assert_eq!(archived.len(), 6);
    let replay = load_documents(&MemoryArchive(archived), &selected)
        .unwrap()
        .unwrap();
    assert_eq!(replay, payloads);
    assert_eq!(
        server.requests(),
        expected,
        "archive replay makes no provider requests"
    );
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
async fn malformed_optional_group_is_skipped_online_and_from_the_archive() {
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
    let fetched = server.providers.fetch_payloads(None, &selected, CallPriority::Interactive)
        .await
        .unwrap();
    assert!(fetched.supporting.is_empty());
    let archived = MemoryArchive(HashMap::from([
        (
            (PayloadSource::MusicBrainz, selected.key.clone()),
            anchor.clone(),
        ),
        (
            (
                PayloadSource::MusicBrainzReleaseGroup,
                "optional-shape-group".into(),
            ),
            "{}".into(),
        ),
    ]));
    let replayed = load_documents(&archived, &selected).unwrap().unwrap();
    assert_eq!(replayed, fetched);
    let stored = ReleasePayloads {
        release: selected,
        anchor,
        supporting: vec![SourcePayload::new(
            PayloadSource::MusicBrainzReleaseGroup,
            "optional-shape-group",
            "{}".into(),
        )],
    };
    let stored: ReleasePayloads =
        serde_json::from_str(&serde_json::to_string(&stored).unwrap()).unwrap();
    let enriched = server.providers.enrich_payloads(None, &stored, CallPriority::Interactive)
    .await
    .unwrap();
    assert_eq!(enriched, fetched);
    assert_eq!(
        server.requests(),
        HashMap::from([
            ("/ws/2/release/optional-shape-release".into(), 1),
            ("/ws/2/release-group/optional-shape-group".into(), 1),
        ])
    );
}

#[tokio::test]
async fn malformed_archived_reverse_alias_does_not_block_enrichment() {
    let stored = ReleasePayloads {
        release: MetadataRef::new(Catalog::Discogs, "940001"),
        anchor: serde_json::json!({"id":940001,"title":"Selected Album"}).to_string(),
        supporting: vec![SourcePayload::new(
            PayloadSource::MusicBrainzDiscogsXref,
            "940001",
            "{}".into(),
        )],
    };
    let server = ProviderServer::start(HashMap::new()).await;
    server.providers.musicbrainz().seed_discogs_url_lookup("940001", None);
    let archive = MemoryArchive(HashMap::from([
        (
            (PayloadSource::Discogs, stored.release.key.clone()),
            stored.anchor.clone(),
        ),
        (
            (PayloadSource::MusicBrainzDiscogsXref, "940001".into()),
            "{}".into(),
        ),
    ]));
    let replayed = load_documents(&archive, &stored.release).unwrap().unwrap();
    assert!(replayed.supporting.is_empty());
    let stored: ReleasePayloads =
        serde_json::from_str(&serde_json::to_string(&stored).unwrap()).unwrap();
    let enriched = server.providers.enrich_payloads(None, &stored, CallPriority::Interactive)
    .await
    .unwrap();
    assert!(enriched.supporting.is_empty());
    assert_eq!(enriched.extract().unwrap().records().len(), 1);
    assert!(server.requests().is_empty());
}

#[tokio::test]
async fn malformed_required_anchor_is_rejected_online_and_from_the_archive() {
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
    let archive = MemoryArchive(HashMap::from([(
        (PayloadSource::MusicBrainz, selected.key.clone()),
        "{}".into(),
    )]));
    assert!(load_documents(&archive, &selected).is_err());
    assert_eq!(
        server.requests(),
        HashMap::from([("/ws/2/release/required-shape-release".into(), 1)])
    );
}
