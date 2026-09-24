use super::*;
use crate::util::http::Http;

fn release(source: Catalog, release_id: &str, group_id: Option<&str>) -> MetadataResult {
    MetadataResult::for_test(source, release_id, group_id)
}

/// A group's links join nothing unless the other catalog's releases are on
/// the list, so a list of one catalog asks for none.
#[test]
fn a_list_of_one_catalog_reads_no_links() {
    let musicbrainz = [
        release(Catalog::MusicBrainz, "mb-1", Some("group-a")),
        release(Catalog::MusicBrainz, "mb-2", Some("group-b")),
    ];
    assert!(groups_to_read(&musicbrainz, |_| false).is_empty());

    let discogs = [release(Catalog::Discogs, "dg-1", Some("7"))];
    assert!(groups_to_read(&discogs, |_| false).is_empty());
}

/// With both catalogs on the list, every MusicBrainz group not yet asked is
/// read once, in the order the list names it.
#[test]
fn a_list_of_both_catalogs_reads_each_group_not_yet_asked_once() {
    let mut read = release(Catalog::MusicBrainz, "mb-4", Some("group-read"));
    read.album_links = AlbumLinks::Read(Vec::new());
    let results = [
        release(Catalog::Discogs, "dg-1", Some("7")),
        release(Catalog::MusicBrainz, "mb-1", Some("group-b")),
        release(Catalog::MusicBrainz, "mb-2", Some("group-a")),
        release(Catalog::MusicBrainz, "mb-3", Some("group-b")),
        release(Catalog::MusicBrainz, "mb-ungrouped", None),
        release(Catalog::MusicBrainz, "mb-5", Some("group-asked")),
        read,
    ];
    assert_eq!(
        groups_to_read(&results, |group| group == "group-asked"),
        vec!["group-b".to_string(), "group-a".to_string()]
    );
}

/// What was read lands on every MusicBrainz record of the group, and on
/// nothing else.
#[test]
fn what_was_read_lands_on_the_group_s_musicbrainz_records() {
    let read = vec![(
        "group-a".to_string(),
        AlbumLinks::Read(vec![MetadataRef::new(Catalog::Discogs, "7")]),
    )];
    let mut linked = release(Catalog::MusicBrainz, "mb-1", Some("group-a"));
    let mut other = release(Catalog::MusicBrainz, "mb-2", Some("group-b"));
    let mut discogs = release(Catalog::Discogs, "dg-1", Some("group-a"));
    apply(&mut linked, &read);
    apply(&mut other, &read);
    apply(&mut discogs, &read);
    assert_eq!(linked.album_links, read[0].1);
    assert_eq!(other.album_links, AlbumLinks::NotAsked);
    assert_eq!(discogs.album_links, AlbumLinks::NotAsked);
}

/// The group's page names its Discogs master among its other links; only
/// the other lookup catalog's album is one to join.
#[tokio::test]
async fn a_group_page_names_its_discogs_master() {
    let musicbrainz = MusicBrainz::for_test(Http::for_test());
    musicbrainz.seed_release_group_json_cache(
        "0f5d2a51-8c1e-4b7a-9e3d-6a2b4c8d1e7f",
        serde_json::json!({
            "id": "0f5d2a51-8c1e-4b7a-9e3d-6a2b4c8d1e7f",
            "title": "Album",
            "relations": [
                {"type": "discogs", "url": {"resource": "https://www.discogs.com/master/510001"}},
                {"type": "discogs", "url": {"resource": "https://www.discogs.com/master/510001-Album"}},
                {"type": "allmusic", "url": {"resource": "https://www.allmusic.com/album/mw0000000001"}},
                {"type": "discogs", "url": {"resource": "https://www.discogs.com/release/42"}}
            ]
        })
        .to_string(),
    );
    let read = read(
        &musicbrainz,
        &["0f5d2a51-8c1e-4b7a-9e3d-6a2b4c8d1e7f".to_string()],
        CallPriority::Interactive,
    )
    .await;
    assert_eq!(
        read,
        vec![(
            "0f5d2a51-8c1e-4b7a-9e3d-6a2b4c8d1e7f".to_string(),
            AlbumLinks::Read(vec![MetadataRef::new(Catalog::Discogs, "510001")]),
        )]
    );
}

/// A page that cannot be had is unread, not a page that links nothing.
#[tokio::test]
async fn a_group_page_that_cannot_be_had_is_unread() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        while let Ok((mut stream, _)) = listener.accept().await {
            let mut buffer = [0u8; 4096];
            let _ = stream.read(&mut buffer).await;
            let _ = stream
                .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 2\r\n\r\n{}")
                .await;
        }
    });
    let musicbrainz = MusicBrainz::for_test(Http::for_test().serve("musicbrainz.org", &origin));
    let read = read(
        &musicbrainz,
        &["group-gone".to_string()],
        CallPriority::Interactive,
    )
    .await;
    assert_eq!(read, vec![("group-gone".to_string(), AlbumLinks::Unread)]);
}
