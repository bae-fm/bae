use super::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const GROUP: &str = "0f5d2a51-8c1e-4b7a-9e3d-6a2b4c8d1e7f";

fn release(source: Catalog, release_id: &str, group_id: Option<&str>) -> MetadataResult {
    MetadataResult::for_test(source, release_id, group_id)
}

fn discogs(key: &str) -> MetadataRef {
    MetadataRef::new(Catalog::Discogs, key)
}

/// A local server standing in for MusicBrainz, Wikidata and Discogs: it
/// answers by what each request asks for and counts the requests.
struct Catalogs {
    requests: Arc<Mutex<HashMap<String, usize>>>,
    task: tokio::task::JoinHandle<()>,
    providers: crate::providers::Providers,
}

impl Catalogs {
    /// Answers are keyed `browse:<group>` for a release group's browsed
    /// releases, `wikidata:<item>` for an entity document, and
    /// `discogs:<release>` for a Discogs release. Anything else answers 599.
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
                let key = if url.path() == "/ws/2/release" {
                    let group = url
                        .query_pairs()
                        .find(|(key, _)| key == "release-group")
                        .unwrap()
                        .1
                        .into_owned();
                    format!("browse:{group}")
                } else if let Some(item) = url.path().strip_prefix("/wiki/Special:EntityData/") {
                    format!("wikidata:{}", item.trim_end_matches(".json"))
                } else if let Some(id) = url.path().strip_prefix("/releases/") {
                    format!("discogs:{id}")
                } else {
                    url.path().to_owned()
                };
                *recorded.lock().unwrap().entry(key.clone()).or_insert(0) += 1;
                let (status, body) = answers
                    .get(&key)
                    .cloned()
                    .unwrap_or((599, format!("Unexpected request: {key}")));
                let response = format!(
                    "HTTP/1.1 {status} Response\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        });
        let http = crate::util::http::Http::for_test()
            .serve("musicbrainz.org", &origin)
            .serve("www.wikidata.org", &origin)
            .serve("api.discogs.com", &origin);
        Self {
            requests,
            task,
            providers: crate::providers::Providers::for_test(http),
        }
    }

    async fn read(&self, to_read: &ToRead) -> Vec<GroupReading<()>> {
        let client = DiscogsClient::new(self.providers.discogs().clone(), "fixture-token".into());
        self.providers
            .read_album_links(Some(&client), to_read, CallPriority::Interactive)
            .await
    }

    fn requests(&self) -> HashMap<String, usize> {
        self.requests.lock().unwrap().clone()
    }
}

impl Drop for Catalogs {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// A group's browsed releases: each release with the addresses it links,
/// and the group's own addresses on every one of them.
fn browsed(group_urls: &[&str], releases: &[(&str, &[&str])]) -> (u16, String) {
    let relations = |urls: &[&str]| {
        urls.iter()
            .map(|url| serde_json::json!({"type": "discogs", "url": {"resource": url}}))
            .collect::<Vec<_>>()
    };
    let body = serde_json::json!({
        "release-count": releases.len(),
        "releases": releases.iter().map(|(id, urls)| serde_json::json!({
            "id": id,
            "relations": relations(urls),
            "release-group": {"id": GROUP, "relations": relations(group_urls)},
        })).collect::<Vec<_>>(),
    });
    (200, body.to_string())
}

fn discogs_release(id: u64, master: Option<u64>) -> (u16, String) {
    let body = serde_json::json!({
        "id": id,
        "title": "Album",
        "artists": [{"id": 1, "name": "Artist"}],
        "country": "Europe",
        "master_id": master,
    });
    (200, body.to_string())
}

fn wikidata_item(item: &str, discogs_master: &str) -> (u16, String) {
    let body = serde_json::json!({"entities": {item: {"claims": {
        "P1954": [{"mainsnak": {"datavalue": {"value": discogs_master}}}],
    }}}});
    (200, body.to_string())
}

/// One group to read, its releases on the list each with the links its
/// record states, beside a Discogs release of master 510009 already on the
/// list.
fn group(releases: &[(&str, &[MetadataRef])]) -> ToRead {
    ToRead {
        groups: vec![GroupToRead {
            group: GROUP.to_string(),
            releases: releases
                .iter()
                .map(|(id, links)| (id.to_string(), links.to_vec()))
                .collect(),
        }],
        on_list: vec![(discogs("800"), Some("510009".to_string()))],
    }
}

/// A group's links join nothing unless the other catalog's releases are on
/// the list, so a list of one catalog asks for none.
#[test]
fn a_list_of_one_catalog_reads_no_links() {
    let musicbrainz = [
        release(Catalog::MusicBrainz, "mb-1", Some("group-a")),
        release(Catalog::MusicBrainz, "mb-2", Some("group-b")),
    ];
    assert!(to_read(&musicbrainz, |_| false).is_empty());

    let discogs = [release(Catalog::Discogs, "dg-1", Some("7"))];
    assert!(to_read(&discogs, |_| false).is_empty());
}

/// With both catalogs on the list, every MusicBrainz group not yet asked is
/// read once, in the order the list names it, with its releases on the list
/// and the links their records state; the other catalog's releases go with
/// it, each with its album.
#[test]
fn a_list_of_both_catalogs_reads_each_group_not_yet_asked_once() {
    let mut read = release(Catalog::MusicBrainz, "mb-4", Some("group-read"));
    read.album_links = AlbumLinks::Read(Vec::new());
    let mut linking = release(Catalog::MusicBrainz, "mb-3", Some("group-b"));
    linking.links = vec![discogs("dg-2")];
    let results = [
        release(Catalog::Discogs, "dg-1", Some("7")),
        release(Catalog::MusicBrainz, "mb-1", Some("group-b")),
        release(Catalog::MusicBrainz, "mb-2", Some("group-a")),
        linking,
        release(Catalog::MusicBrainz, "mb-ungrouped", None),
        release(Catalog::MusicBrainz, "mb-5", Some("group-asked")),
        read,
    ];
    let to_read = to_read(&results, |group| group == "group-asked");
    assert_eq!(
        to_read.groups,
        vec![
            GroupToRead {
                group: "group-b".to_string(),
                releases: vec![
                    ("mb-1".to_string(), Vec::new()),
                    ("mb-3".to_string(), vec![discogs("dg-2")]),
                ],
            },
            GroupToRead {
                group: "group-a".to_string(),
                releases: vec![("mb-2".to_string(), Vec::new())],
            },
        ]
    );
    assert_eq!(to_read.on_list, vec![(discogs("dg-1"), Some("7".to_string()))]);
}

/// What was read lands on every MusicBrainz record of the group, and on
/// nothing else; a release whose own links the browse read carries them.
#[test]
fn what_was_read_lands_on_the_group_s_musicbrainz_records() {
    let links = AlbumLinks::Read(vec![AlbumLink {
        album: discogs("7"),
        stated: AlbumStatement::Page,
    }]);
    let read = vec![GroupReading {
        group: "group-a".to_string(),
        links: links.clone(),
        release_links: vec![("mb-1".to_string(), vec![discogs("dg-9")])],
        twin: None::<Twin<()>>,
    }];
    let mut linked = release(Catalog::MusicBrainz, "mb-1", Some("group-a"));
    let mut sibling = release(Catalog::MusicBrainz, "mb-3", Some("group-a"));
    let mut other = release(Catalog::MusicBrainz, "mb-2", Some("group-b"));
    let mut discogs_record = release(Catalog::Discogs, "dg-1", Some("group-a"));
    apply(&mut linked, &read);
    apply(&mut sibling, &read);
    apply(&mut other, &read);
    apply(&mut discogs_record, &read);
    assert_eq!(linked.album_links, links);
    assert_eq!(linked.links, vec![discogs("dg-9")]);
    assert_eq!(sibling.album_links, links);
    assert!(sibling.links.is_empty());
    assert_eq!(other.album_links, AlbumLinks::NotAsked);
    assert_eq!(discogs_record.album_links, AlbumLinks::NotAsked);
}

/// A twin goes on a list only beside the release that names it, and never
/// where the list already holds it.
#[test]
fn a_twin_goes_only_beside_the_release_that_names_it() {
    let twin = |named_by: &str, id: &str| Twin {
        result: release(Catalog::Discogs, id, Some("7")),
        named_by: MetadataRef::new(Catalog::MusicBrainz, named_by),
        status: (),
    };
    let reading = |twin: Twin<()>| GroupReading {
        group: "group-a".to_string(),
        links: AlbumLinks::Read(Vec::new()),
        release_links: Vec::new(),
        twin: Some(twin),
    };
    let read = vec![
        reading(twin("mb-1", "dg-twin")),
        reading(twin("mb-gone", "dg-orphan")),
        reading(twin("mb-1", "dg-listed")),
    ];
    let named = release(Catalog::MusicBrainz, "mb-1", Some("group-a"));
    let listed = release(Catalog::Discogs, "dg-listed", Some("7"));
    let on_list = twins(&read, &[&named, &listed]);
    assert_eq!(
        on_list
            .iter()
            .map(|twin| twin.result.release_id.as_str())
            .collect::<Vec<_>>(),
        vec!["dg-twin"]
    );
}

/// The group's page names its Discogs master among its other links: the
/// album-level statement, taken without asking Wikidata or Discogs anything.
#[tokio::test]
async fn a_group_page_naming_its_master_asks_nothing_more() {
    let catalogs = Catalogs::start(HashMap::from([(
        format!("browse:{GROUP}"),
        browsed(
            &[
                "https://www.discogs.com/master/510001",
                "https://www.discogs.com/master/510001-Album",
                "https://www.wikidata.org/wiki/Q1",
                "https://www.allmusic.com/album/mw0000000001",
            ],
            &[("mb-1", &["https://www.discogs.com/release/700"])],
        ),
    )]))
    .await;
    let read = catalogs.read(&group(&[("mb-1", &[discogs("700")])])).await;
    assert_eq!(
        read[0].links,
        AlbumLinks::Read(vec![AlbumLink {
            album: discogs("510001"),
            stated: AlbumStatement::Page,
        }])
    );
    assert_eq!(read[0].twin, None);
    assert_eq!(
        catalogs.requests(),
        HashMap::from([(format!("browse:{GROUP}"), 1)])
    );
}

/// A page that links no master but a Wikidata item that states one: the
/// item's statement is taken, and Discogs is asked nothing.
#[tokio::test]
async fn a_wikidata_item_the_page_links_states_the_master() {
    let catalogs = Catalogs::start(HashMap::from([
        (
            format!("browse:{GROUP}"),
            browsed(
                &[
                    "https://www.allmusic.com/album/mw0000000001",
                    "https://www.wikidata.org/wiki/Q1",
                ],
                &[("mb-1", &["https://www.discogs.com/release/700"])],
            ),
        ),
        ("wikidata:Q1".to_string(), wikidata_item("Q1", "510001")),
    ]))
    .await;
    let read = catalogs.read(&group(&[("mb-1", &[discogs("700")])])).await;
    assert_eq!(
        read[0].links,
        AlbumLinks::Read(vec![AlbumLink {
            album: discogs("510001"),
            stated: AlbumStatement::Wikidata {
                item: "Q1".to_string()
            },
        }])
    );
    assert_eq!(read[0].twin, None);
    assert!(!catalogs.requests().contains_key("discogs:700"));
}

/// With no album-level statement, a release of the group on the list that
/// links a Discogs release is followed: that release's master is the album,
/// and the release goes beside the one that names it. A release a search
/// returned states no links of its own; the browse is what reads them.
#[tokio::test]
async fn a_release_link_is_followed_to_its_master_and_the_twin_goes_beside_it() {
    let catalogs = Catalogs::start(HashMap::from([
        (
            format!("browse:{GROUP}"),
            browsed(
                &["https://www.allmusic.com/album/mw0000000001"],
                &[("mb-1", &["https://www.discogs.com/release/700-Album"])],
            ),
        ),
        ("discogs:700".to_string(), discogs_release(700, Some(510009))),
    ]))
    .await;
    let read = catalogs.read(&group(&[("mb-1", &[])])).await;
    assert_eq!(
        read[0].links,
        AlbumLinks::Read(vec![AlbumLink {
            album: discogs("510009"),
            stated: AlbumStatement::Release {
                musicbrainz_release: "mb-1".to_string(),
                twin: discogs("700"),
            },
        }])
    );
    assert_eq!(
        read[0].release_links,
        vec![("mb-1".to_string(), vec![discogs("700")])]
    );
    let twin = read[0].twin.as_ref().expect("the twin goes beside mb-1");
    assert_eq!(twin.named_by, MetadataRef::new(Catalog::MusicBrainz, "mb-1"));
    assert_eq!(twin.result.source, Catalog::Discogs);
    assert_eq!(twin.result.release_id, "700");
    assert_eq!(twin.result.source_group_id.as_deref(), Some("510009"));
    assert_eq!(twin.result.artist.as_deref(), Some("Artist"));
    assert_eq!(catalogs.requests()["discogs:700"], 1);
}

/// Two releases of one group each linking a Discogs release cost one Discogs
/// request between them: the first on the list is followed.
#[tokio::test]
async fn two_releases_of_one_group_follow_one_link() {
    let catalogs = Catalogs::start(HashMap::from([
        (
            format!("browse:{GROUP}"),
            browsed(
                &[],
                &[
                    ("mb-1", &["https://www.discogs.com/release/700"]),
                    ("mb-2", &["https://www.discogs.com/release/701"]),
                ],
            ),
        ),
        ("discogs:700".to_string(), discogs_release(700, Some(510009))),
        ("discogs:701".to_string(), discogs_release(701, Some(510009))),
    ]))
    .await;
    let read = catalogs
        .read(&group(&[
            ("mb-1", &[discogs("700")]),
            ("mb-2", &[discogs("701")]),
        ]))
        .await;
    assert!(read[0].links.names(&discogs("510009")));
    let requests = catalogs.requests();
    assert_eq!(requests.get("discogs:700"), Some(&1));
    assert_eq!(requests.get("discogs:701"), None);
}

/// A release link naming a Discogs release the list already holds is read off
/// the list: its album is the listed record's, and nothing is asked.
#[tokio::test]
async fn a_link_to_a_release_on_the_list_is_read_off_the_list() {
    let catalogs = Catalogs::start(HashMap::from([(
        format!("browse:{GROUP}"),
        browsed(
            &[],
            &[
                ("mb-1", &["https://www.discogs.com/release/700"]),
                ("mb-2", &["https://www.discogs.com/release/800"]),
            ],
        ),
    )]))
    .await;
    let read = catalogs
        .read(&group(&[
            ("mb-1", &[discogs("700")]),
            ("mb-2", &[discogs("800")]),
        ]))
        .await;
    assert_eq!(
        read[0].links,
        AlbumLinks::Read(vec![AlbumLink {
            album: discogs("510009"),
            stated: AlbumStatement::Release {
                musicbrainz_release: "mb-2".to_string(),
                twin: discogs("800"),
            },
        }])
    );
    assert_eq!(read[0].twin, None, "the list holds it already");
    assert!(!catalogs
        .requests()
        .keys()
        .any(|key| key.starts_with("discogs:")));
}

/// A release link followed to a Discogs release outside the list's albums
/// names that release's own master, whatever the list holds: the album the
/// chain states is the one its documents state.
#[tokio::test]
async fn a_followed_release_names_its_own_master_not_a_listed_one() {
    let catalogs = Catalogs::start(HashMap::from([
        (
            format!("browse:{GROUP}"),
            browsed(&[], &[("mb-1", &["https://www.discogs.com/release/700"])]),
        ),
        ("discogs:700".to_string(), discogs_release(700, Some(620000))),
    ]))
    .await;
    let read = catalogs.read(&group(&[("mb-1", &[discogs("700")])])).await;
    assert!(read[0].links.names(&discogs("620000")));
    assert!(!read[0].links.names(&discogs("510009")));
}

/// The Discogs release a link names cannot be had and nothing else names an
/// album: whether one is stated is not known, and no twin goes on the list.
#[tokio::test]
async fn a_release_link_that_cannot_be_followed_leaves_the_album_unread() {
    let catalogs = Catalogs::start(HashMap::from([(
        format!("browse:{GROUP}"),
        browsed(&[], &[("mb-1", &["https://www.discogs.com/release/700"])]),
    )]))
    .await;
    let read = catalogs.read(&group(&[("mb-1", &[discogs("700")])])).await;
    assert_eq!(read[0].links, AlbumLinks::Unread);
    assert_eq!(read[0].twin, None);
}

/// Nothing on the page, no Wikidata item, no release linking Discogs: read,
/// and nothing is stated.
#[tokio::test]
async fn a_group_nothing_links_reads_as_naming_nothing() {
    let catalogs = Catalogs::start(HashMap::from([(
        format!("browse:{GROUP}"),
        browsed(&[], &[("mb-1", &[])]),
    )]))
    .await;
    let read = catalogs.read(&group(&[("mb-1", &[])])).await;
    assert_eq!(read[0].links, AlbumLinks::Read(Vec::new()));
}

/// A group whose releases cannot be browsed is unread, not a group that
/// links nothing.
#[tokio::test]
async fn a_group_that_cannot_be_browsed_is_unread() {
    let catalogs = Catalogs::start(HashMap::from([(
        format!("browse:{GROUP}"),
        (404, "{}".to_string()),
    )]))
    .await;
    let read = catalogs.read(&group(&[("mb-1", &[])])).await;
    assert_eq!(read[0].links, AlbumLinks::Unread);
}
