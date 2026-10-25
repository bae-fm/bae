use super::*;
use crate::import::source_release::UnfetchedDocument;
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
    /// Answers are keyed by request path, and a MusicBrainz URL lookup's by
    /// `url:` and the resource it asks about. A lookup asking about several
    /// resources is answered as MusicBrainz answers it, from each one's own
    /// answer: the documents of those answered 200, and every one answered
    /// 404 left out. Anything else is answered 599.
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
                let resources: Vec<String> = url
                    .query_pairs()
                    .filter(|(key, _)| key == "resource")
                    .map(|(_, resource)| format!("url:{resource}"))
                    .collect();
                let key = if url.path() == "/ws/2/url" {
                    let mut named = resources.clone();
                    named.sort();
                    named.join(" ")
                } else {
                    url.path().to_owned()
                };
                *recorded.lock().unwrap().entry(key.clone()).or_insert(0) += 1;
                let unexpected = || (599, format!("Unexpected provider request: {key}"));
                let (status, body) = match resources.as_slice() {
                    [_, _, ..] => several_resources(&answers, &resources).unwrap_or_else(unexpected),
                    _ => answers.get(&key).cloned().unwrap_or_else(unexpected),
                };
                let response = format!("HTTP/1.1 {status} Response\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        });
        let http = crate::util::http::Http::for_test()
            .serve("musicbrainz.org", &origin)
            .serve("api.discogs.com", &origin)
            .serve("www.wikidata.org", &origin);
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

/// MusicBrainz's answer about several `resources`, from each one's own
/// answer, or `None` when one has none. One answered with neither a document
/// nor a 404 answers the whole request so.
fn several_resources(
    answers: &HashMap<String, (u16, String)>,
    resources: &[String],
) -> Option<(u16, String)> {
    let mut urls = Vec::new();
    for resource in resources {
        match answers.get(resource)? {
            (200, body) => {
                let mut document: serde_json::Value = serde_json::from_str(body).unwrap();
                document["resource"] = resource.trim_start_matches("url:").into();
                urls.push(document);
            }
            (404, _) => {}
            failed => return Some(failed.clone()),
        }
    }
    Some((
        200,
        serde_json::json!({"url-count": urls.len(), "url-offset": 0, "urls": urls}).to_string(),
    ))
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
    }
    server.providers.musicbrainz().seed_discogs_page_lookup(&[
        (DiscogsPage::Master("910001".into()), Some("shared-group")),
        (DiscogsPage::Master("910002".into()), Some("shared-group")),
    ]);
    let discogs = DiscogsClient::new(server.providers.discogs().clone(), "fixture-token".into());
    let payloads = server.providers.fetch_payloads(Some(&discogs), &MetadataRef::new(Catalog::MusicBrainz, "selected"), ReadDepth::Complete, CallPriority::Interactive)
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

/// A MusicBrainz release linking a Discogs release as itself, its group
/// linking that release's master, and each Discogs page's MusicBrainz URL
/// lookup leading back — every answer by the path it is asked on.
fn linked_pair_answers() -> HashMap<String, (u16, String)> {
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
    HashMap::from([
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
    ])
}

/// The one MusicBrainz request asking about both Discogs addresses of
/// [`linked_pair_answers`], as [`ProviderServer`] records it.
const BOTH_ADDRESSES: &str =
    "url:https://www.discogs.com/master/920002 url:https://www.discogs.com/release/920001";

#[tokio::test]
async fn cyclic_release_and_album_links_fetch_each_document_once() {
    let server = ProviderServer::start(linked_pair_answers()).await;
    let discogs = DiscogsClient::new(server.providers.discogs().clone(), "fixture-token".into());
    let selected = MetadataRef::new(Catalog::MusicBrainz, "cycle-release");
    let payloads = tokio::time::timeout(
        std::time::Duration::from_secs(15),
        server.providers.fetch_payloads(Some(&discogs), &selected, ReadDepth::Complete, CallPriority::Interactive),
    )
    .await
    .unwrap()
    .unwrap();

    assert_eq!(
        server.requests(),
        HashMap::from([
            ("/ws/2/release/cycle-release".into(), 1),
            ("/ws/2/release-group/cycle-group".into(), 1),
            ("/releases/920001".into(), 1),
            ("/masters/920002".into(), 1),
            (BOTH_ADDRESSES.into(), 1),
        ]),
        "the lookups by the Discogs release's address and its master's go out together"
    );
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

/// Reading a MusicBrainz release completely fetches the documents of the
/// Discogs release it names as itself, so reading that release completely
/// afterwards is answered by the providers' response caches: no request goes
/// out.
#[tokio::test]
async fn a_linked_release_is_read_from_what_fetching_its_namer_fetched() {
    let server = ProviderServer::start(linked_pair_answers()).await;
    let discogs = DiscogsClient::new(server.providers.discogs().clone(), "fixture-token".into());
    let namer = MetadataRef::new(Catalog::MusicBrainz, "cycle-release");
    server
        .providers
        .fetch_payloads(Some(&discogs), &namer, ReadDepth::Complete, CallPriority::Interactive)
        .await
        .unwrap();
    let asked = server.requests();

    let named = MetadataRef::new(Catalog::Discogs, "920001");
    let twin = server
        .providers
        .fetch_payloads(Some(&discogs), &named, ReadDepth::Complete, CallPriority::Interactive)
        .await
        .unwrap();
    assert_eq!(server.requests(), asked, "reading the twin asks nothing");
    assert!(twin.unfetched.is_empty(), "and it reads every document it names");
}

/// The supporting documents a read stored, by source and key.
fn stored_keys(payloads: &ReleasePayloads) -> Vec<DocumentKey> {
    let mut keys: Vec<DocumentKey> = payloads
        .supporting
        .iter()
        .map(|document| (document.source, document.source_release_id.clone()))
        .collect();
    keys.sort_by(|left, right| (left.0.as_str(), &left.1).cmp(&(right.0.as_str(), &right.1)));
    keys
}

/// A complete read of a Discogs release asks MusicBrainz about its address and
/// its master's in one request, and follows each to the MusicBrainz release
/// and release group linking it, as a lookup by each address alone did.
#[tokio::test]
async fn a_complete_read_of_a_discogs_release_asks_about_both_addresses_at_once() {
    let server = ProviderServer::start(linked_pair_answers()).await;
    let discogs = DiscogsClient::new(server.providers.discogs().clone(), "fixture-token".into());
    let payloads = server
        .providers
        .fetch_payloads(
            Some(&discogs),
            &MetadataRef::new(Catalog::Discogs, "920001"),
            ReadDepth::Complete,
            CallPriority::Interactive,
        )
        .await
        .unwrap();

    assert_eq!(
        server.requests(),
        HashMap::from([
            ("/releases/920001".into(), 1),
            ("/masters/920002".into(), 1),
            (BOTH_ADDRESSES.into(), 1),
            ("/ws/2/release/cycle-release".into(), 1),
            ("/ws/2/release-group/cycle-group".into(), 1),
        ])
    );
    assert_eq!(
        stored_keys(&payloads),
        vec![
            (PayloadSource::DiscogsMaster, "920002".into()),
            (PayloadSource::MusicBrainz, "cycle-release".into()),
            (PayloadSource::MusicBrainzDiscogsMasterXref, "920002".into()),
            (PayloadSource::MusicBrainzDiscogsXref, "920001".into()),
            (PayloadSource::MusicBrainzReleaseGroup, "cycle-group".into()),
        ]
    );
    let answers = linked_pair_answers();
    let document = |source, key: &str| {
        payloads
            .supporting
            .iter()
            .find(|document| document.source == source && document.source_release_id == key)
            .map(|document| document.json.clone())
            .unwrap()
    };
    assert_eq!(
        document(PayloadSource::MusicBrainzDiscogsXref, "920001"),
        answers["/ws/2/release/cycle-release"].1,
        "the release's address stands for the MusicBrainz release linking it"
    );
    assert_eq!(
        document(PayloadSource::MusicBrainzDiscogsMasterXref, "920002"),
        answers["/ws/2/release-group/cycle-group"].1,
        "the master's address stands for the release group linking it"
    );
    assert!(payloads.unfetched.is_empty(), "{:?}", payloads.unfetched);
}

/// One request answering for a known address and an unknown one: the known
/// one is followed to the MusicBrainz release linking it, and the unknown one
/// has no document and is not missing anything.
#[tokio::test]
async fn an_address_musicbrainz_does_not_know_is_absent_beside_one_it_does() {
    let mut answers = linked_pair_answers();
    answers.insert("url:https://www.discogs.com/master/920002".into(), (404, String::new()));
    answers.remove("/ws/2/release-group/cycle-group");
    let server = ProviderServer::start(answers).await;
    let release = release_json("cycle-release", None, &["https://www.discogs.com/release/920001"]);
    server.providers.musicbrainz().seed_release_cache("cycle-release", release);
    let discogs = DiscogsClient::new(server.providers.discogs().clone(), "fixture-token".into());
    let payloads = server
        .providers
        .fetch_payloads(
            Some(&discogs),
            &MetadataRef::new(Catalog::Discogs, "920001"),
            ReadDepth::Complete,
            CallPriority::Interactive,
        )
        .await
        .unwrap();

    assert_eq!(
        server.requests(),
        HashMap::from([
            ("/releases/920001".into(), 1),
            ("/masters/920002".into(), 1),
            (BOTH_ADDRESSES.into(), 1),
        ])
    );
    assert_eq!(
        stored_keys(&payloads),
        vec![
            (PayloadSource::DiscogsMaster, "920002".into()),
            (PayloadSource::MusicBrainz, "cycle-release".into()),
            (PayloadSource::MusicBrainzDiscogsXref, "920001".into()),
        ]
    );
    assert!(payloads.unfetched.is_empty(), "{:?}", payloads.unfetched);
}

/// The one request asking about both addresses failing fails both lookups it
/// carried: each is named as a document the read did not get.
#[tokio::test]
async fn a_failed_lookup_by_both_addresses_fails_both() {
    let mut answers = linked_pair_answers();
    answers.insert(
        "url:https://www.discogs.com/master/920002".into(),
        (400, "Rejected URL request".into()),
    );
    let server = ProviderServer::start(answers).await;
    let discogs = DiscogsClient::new(server.providers.discogs().clone(), "fixture-token".into());
    let payloads = server
        .providers
        .fetch_payloads(
            Some(&discogs),
            &MetadataRef::new(Catalog::Discogs, "920001"),
            ReadDepth::Complete,
            CallPriority::Interactive,
        )
        .await
        .unwrap();

    assert_eq!(
        server.requests(),
        HashMap::from([
            ("/releases/920001".into(), 1),
            ("/masters/920002".into(), 1),
            (BOTH_ADDRESSES.into(), 1),
        ])
    );
    assert_eq!(stored_keys(&payloads), vec![(PayloadSource::DiscogsMaster, "920002".into())]);
    let mut unfetched = payloads.unfetched.clone();
    unfetched.sort_by_key(|document| document.document.as_str());
    assert_eq!(
        unfetched,
        vec![
            UnfetchedDocument {
                document: PayloadSource::MusicBrainzDiscogsMasterXref,
                key: "920002".into(),
                reason: UnfetchedReason::Failed,
            },
            UnfetchedDocument {
                document: PayloadSource::MusicBrainzDiscogsXref,
                key: "920001".into(),
                reason: UnfetchedReason::Failed,
            },
        ]
    );
}

/// An identification read of a Discogs release reads its own page and its
/// master, and asks MusicBrainz nothing: no lookup by its Discogs address or
/// its master's, and no MusicBrainz page. What its pages name and it did not
/// follow is recorded as stopped short of, which only a complete read asks
/// for again.
#[tokio::test]
async fn an_identification_read_of_a_discogs_release_asks_musicbrainz_nothing() {
    let server = ProviderServer::start(linked_pair_answers()).await;
    let discogs = DiscogsClient::new(server.providers.discogs().clone(), "fixture-token".into());
    let selected = MetadataRef::new(Catalog::Discogs, "920001");
    let payloads = server
        .providers
        .fetch_payloads(
            Some(&discogs),
            &selected,
            ReadDepth::Identification,
            CallPriority::Interactive,
        )
        .await
        .unwrap();

    assert_eq!(
        server.requests(),
        HashMap::from([
            ("/releases/920001".into(), 1),
            ("/masters/920002".into(), 1),
        ])
    );
    let mut unfetched = payloads.unfetched.clone();
    unfetched.sort_by_key(|document| document.key.clone());
    assert_eq!(
        unfetched,
        vec![
            UnfetchedDocument {
                document: PayloadSource::MusicBrainzDiscogsXref,
                key: "920001".into(),
                reason: UnfetchedReason::StoppedShort,
            },
            UnfetchedDocument {
                document: PayloadSource::MusicBrainzDiscogsMasterXref,
                key: "920002".into(),
                reason: UnfetchedReason::StoppedShort,
            },
        ]
    );
    let stored = payloads.extract().unwrap();
    assert_eq!(
        stored.metadata.album.first_year, None,
        "the master states no year, and nothing else is asked for one"
    );
    assert!(!stored.fetch_could_add(ReadDepth::Identification, true));
    assert!(stored.fetch_could_add(ReadDepth::Complete, true));
}

/// A MusicBrainz release linking a Discogs release as itself, whose release
/// group links a Wikidata item that states the album's Discogs master, with
/// every document either read could ask for answered by its path.
fn statement_answers() -> HashMap<String, (u16, String)> {
    let mut answers = linked_pair_answers();
    answers.insert(
        "/ws/2/release-group/cycle-group".into(),
        (
            200,
            serde_json::json!({"id":"cycle-group", "relations":[
                {"url":{"resource":"https://www.wikidata.org/wiki/Q920003"}}
            ]})
            .to_string(),
        ),
    );
    answers.insert(
        "/wiki/Special:EntityData/Q920003.json".into(),
        (
            200,
            serde_json::json!({"entities":{"Q920003":{"claims":{
                "P1954":[{"mainsnak":{"datavalue":{"value":"920002"}}}],
                "P436":[{"mainsnak":{"datavalue":{"value":"cycle-group"}}}]
            }}}})
            .to_string(),
        ),
    );
    answers
}

/// An identification read of a MusicBrainz release reads what its album
/// statements read — its release group, the Wikidata item the group links and
/// the Discogs release it links as itself — and not the Discogs master or any
/// lookup by a Discogs address. Its statements are the ones a complete read
/// finds.
#[tokio::test]
async fn an_identification_read_of_a_musicbrainz_release_reads_what_its_statements_read() {
    let selected = MetadataRef::new(Catalog::MusicBrainz, "cycle-release");
    let server = ProviderServer::start(statement_answers()).await;
    let discogs = DiscogsClient::new(server.providers.discogs().clone(), "fixture-token".into());
    let identification = server
        .providers
        .fetch_payloads(
            Some(&discogs),
            &selected,
            ReadDepth::Identification,
            CallPriority::Interactive,
        )
        .await
        .unwrap();

    assert_eq!(
        server.requests(),
        HashMap::from([
            ("/ws/2/release/cycle-release".into(), 1),
            ("/ws/2/release-group/cycle-group".into(), 1),
            ("/wiki/Special:EntityData/Q920003.json".into(), 1),
            ("/releases/920001".into(), 1),
        ])
    );
    let mut unfetched = identification.unfetched.clone();
    unfetched.sort_by_key(|document| (document.key.clone(), document.document.as_str()));
    assert_eq!(
        unfetched,
        vec![
            UnfetchedDocument {
                document: PayloadSource::MusicBrainzDiscogsXref,
                key: "920001".into(),
                reason: UnfetchedReason::StoppedShort,
            },
            UnfetchedDocument {
                document: PayloadSource::DiscogsMaster,
                key: "920002".into(),
                reason: UnfetchedReason::StoppedShort,
            },
            UnfetchedDocument {
                document: PayloadSource::MusicBrainzDiscogsMasterXref,
                key: "920002".into(),
                reason: UnfetchedReason::StoppedShort,
            },
        ]
    );

    let complete_server = ProviderServer::start(statement_answers()).await;
    let complete_discogs =
        DiscogsClient::new(complete_server.providers.discogs().clone(), "fixture-token".into());
    let complete = complete_server
        .providers
        .fetch_payloads(
            Some(&complete_discogs),
            &selected,
            ReadDepth::Complete,
            CallPriority::Interactive,
        )
        .await
        .unwrap();
    assert!(complete.unfetched.is_empty(), "{:?}", complete.unfetched);
    let identified = identification.extract().unwrap();
    let completed = complete.extract().unwrap();
    assert_eq!(identified.album_links(), completed.album_links());
    assert_eq!(
        identified.album_links().read().iter().map(|link| link.album.clone()).collect::<Vec<_>>(),
        vec![MetadataRef::new(Catalog::Discogs, "920002")]
    );
    assert_eq!(identified.links(), completed.links());
    assert_eq!(identified.metadata, completed.metadata);
}

#[tokio::test]
async fn ambiguous_master_backlinks_do_not_fetch_or_claim_either_album() {
    let server = ProviderServer::start(HashMap::from([
        (
            "url:https://www.discogs.com/master/930002".into(),
            (
                200,
                serde_json::json!({"relations":[
                    {"type":"discogs","target-type":"release_group","release_group":{"id":"group-a"}},
                    {"type":"discogs","target-type":"release_group","release_group":{"id":"group-b"}}
                ]})
                .to_string(),
            ),
        ),
        ("url:https://www.discogs.com/release/930001".into(), (404, String::new())),
    ]))
    .await;
    server.providers.discogs().seed_release_cache(
        "930001",
        serde_json::json!({"id":930001,"title":"Album Title","master_id":930002}).to_string(),
    );
    server.providers.discogs().seed_master_cache(
        "930002",
        serde_json::json!({"id":930002}).to_string(),
    );
    let discogs = DiscogsClient::new(server.providers.discogs().clone(), "fixture-token".into());
    let selected = MetadataRef::new(Catalog::Discogs, "930001");
    let payloads = server.providers.fetch_payloads(Some(&discogs), &selected, ReadDepth::Complete, CallPriority::Interactive)
        .await
        .unwrap();

    assert_eq!(
        server.requests(),
        HashMap::from([(
            "url:https://www.discogs.com/master/930002 url:https://www.discogs.com/release/930001"
                .into(),
            1
        )])
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
        .fetch_payloads(None, &selected, ReadDepth::Complete, CallPriority::Interactive)
        .await
        .unwrap();
    assert!(fetched.supporting.is_empty());
    assert_eq!(
        fetched.unfetched,
        vec![UnfetchedDocument {
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
        .seed_discogs_page_lookup(&[(DiscogsPage::Release("940001".into()), Some("mb-alias"))]);
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
            ReadDepth::Complete,
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
        .fetch_payloads(None, &selected, ReadDepth::Complete, CallPriority::Interactive)
        .await
        .unwrap();
    let mut unfetched = fetched.unfetched.clone();
    unfetched.sort_by_key(|document| document.key.clone());
    assert_eq!(
        unfetched,
        vec![
            UnfetchedDocument {
                document: PayloadSource::Discogs,
                key: "950001".into(),
                reason: UnfetchedReason::DiscogsNotConfigured,
            },
            UnfetchedDocument {
                document: PayloadSource::MusicBrainzReleaseGroup,
                key: "missing-parts-group".into(),
                reason: UnfetchedReason::Failed,
            },
        ]
    );
    let stored = fetched.extract().unwrap();
    assert!(stored.fetch_could_add(ReadDepth::Complete, false), "the failed group can be asked again");
    let mut only_the_key_missing = stored.clone();
    only_the_key_missing
        .unfetched
        .retain(|document| document.reason == UnfetchedReason::DiscogsNotConfigured);
    assert!(
        !only_the_key_missing.fetch_could_add(ReadDepth::Complete, false),
        "without a key, asking again gets no Discogs document"
    );
    assert!(
        only_the_key_missing.fetch_could_add(ReadDepth::Complete, true),
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
        server.providers.fetch_payloads(None, &selected, ReadDepth::Complete, CallPriority::Interactive)
            .await
            .is_err()
    );
    assert_eq!(
        server.requests(),
        HashMap::from([("/ws/2/release/required-shape-release".into(), 1)])
    );
}
