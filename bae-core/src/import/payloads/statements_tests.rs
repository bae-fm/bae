//! The statements a MusicBrainz release's documents make about its album on
//! Discogs, strongest first.

use super::*;
use crate::import::album_links::{AlbumLink, AlbumLinks, AlbumStatement};
use crate::import::source_release::{UnfetchedDocument, UnfetchedReason};
use serde_json::json;

fn url(resource: &str) -> serde_json::Value {
    json!({"url":{"resource":resource}})
}

/// A release in group `g` linking `release_urls`, the group's relations
/// embedded as `embedded` where given.
fn release(release_urls: &[&str], embedded: Option<&[&str]>, supporting: Vec<SourcePayload>) -> ReleasePayloads {
    let mut group = json!({"id":"g"});
    if let Some(urls) = embedded {
        group["relations"] = urls.iter().map(|resource| url(resource)).collect();
    }
    ReleasePayloads::for_test(
        MetadataRef::new(Catalog::MusicBrainz, "mb-release"),
        json!({
            "id":"mb-release", "title":"Album Title",
            "artist-credit":[{"name":"Artist Name","artist":{"id":"artist-id","name":"Artist Name"}}],
            "release-group": group,
            "relations": release_urls.iter().map(|resource| url(resource)).collect::<Vec<_>>(),
            "cover-art-archive": {"front": false, "darkened": false},
        })
        .to_string(),
        supporting,
    )
}

fn group_document(urls: &[&str]) -> SourcePayload {
    SourcePayload::new(
        PayloadSource::MusicBrainzReleaseGroup,
        "g",
        json!({"id":"g","relations":urls.iter().map(|resource| url(resource)).collect::<Vec<_>>()})
            .to_string(),
    )
}

fn wikidata(item: &str, master: &str) -> SourcePayload {
    SourcePayload::new(
        PayloadSource::Wikidata,
        item,
        json!({"entities":{item:{"claims":{"P1954":[{"mainsnak":{"datavalue":{"value":master}}}]}}}})
            .to_string(),
    )
}

fn discogs_release(id: u64, master: Option<u64>) -> SourcePayload {
    SourcePayload::new(
        PayloadSource::Discogs,
        id.to_string(),
        json!({"id":id,"title":"Album Title","master_id":master}).to_string(),
    )
}

fn link(master: &str, stated: AlbumStatement) -> AlbumLink {
    AlbumLink {
        album: MetadataRef::new(Catalog::Discogs, master),
        stated,
    }
}

fn through(twin: &str) -> AlbumStatement {
    AlbumStatement::Release {
        musicbrainz_release: "mb-release".to_string(),
        twin: MetadataRef::new(Catalog::Discogs, twin),
    }
}

#[test]
fn the_group_page_is_taken_over_every_weaker_statement() {
    let payloads = release(
        &["https://www.discogs.com/release/11"],
        None,
        vec![
            group_document(&[
                "https://www.discogs.com/master/201",
                "https://www.wikidata.org/wiki/Q1",
            ]),
            wikidata("Q1", "202"),
            discogs_release(11, Some(101)),
        ],
    );
    assert_eq!(
        payloads.album_statements().unwrap(),
        AlbumLinks::Read(vec![link("201", AlbumStatement::Page)])
    );
}

#[test]
fn the_group_relations_the_release_embeds_are_its_page() {
    let payloads = release(&[], Some(&["https://www.discogs.com/master/201"]), vec![]);
    assert_eq!(
        payloads.album_statements().unwrap(),
        AlbumLinks::Read(vec![link("201", AlbumStatement::Page)])
    );
}

#[test]
fn a_wikidata_item_the_page_links_is_taken_over_a_release_link() {
    let payloads = release(
        &["https://www.discogs.com/release/11"],
        None,
        vec![
            group_document(&["https://www.wikidata.org/wiki/Q1"]),
            wikidata("Q1", "202"),
            discogs_release(11, Some(101)),
        ],
    );
    assert_eq!(
        payloads.album_statements().unwrap(),
        AlbumLinks::Read(vec![link(
            "202",
            AlbumStatement::Wikidata {
                item: "Q1".to_string()
            }
        )])
    );
}

#[test]
fn a_linked_release_names_its_master_when_nothing_stronger_does() {
    let payloads = release(
        &["https://www.discogs.com/release/11"],
        None,
        vec![group_document(&[]), discogs_release(11, Some(101))],
    );
    assert_eq!(
        payloads.album_statements().unwrap(),
        AlbumLinks::Read(vec![link("101", through("11"))])
    );
}

#[test]
fn linked_releases_name_one_master_only_when_every_one_was_read() {
    let mut payloads = release(
        &[
            "https://www.discogs.com/release/11",
            "https://www.discogs.com/release/12",
        ],
        None,
        vec![group_document(&[]), discogs_release(11, Some(101))],
    );
    payloads.unfetched.push(UnfetchedDocument {
        document: PayloadSource::Discogs,
        key: "12".to_string(),
        reason: UnfetchedReason::Failed,
    });
    assert_eq!(payloads.album_statements().unwrap(), AlbumLinks::Unread);

    let both = release(
        &[
            "https://www.discogs.com/release/11",
            "https://www.discogs.com/release/12",
        ],
        None,
        vec![
            group_document(&[]),
            discogs_release(11, Some(101)),
            discogs_release(12, Some(101)),
        ],
    );
    assert_eq!(
        both.album_statements().unwrap(),
        AlbumLinks::Read(vec![link("101", through("11"))])
    );
}

#[test]
fn a_linked_release_that_was_not_fetched_leaves_the_album_unread() {
    let mut payloads = release(&["https://www.discogs.com/release/11"], None, vec![group_document(&[])]);
    payloads.unfetched.push(UnfetchedDocument {
        document: PayloadSource::Discogs,
        key: "11".to_string(),
        reason: UnfetchedReason::DiscogsNotConfigured,
    });
    assert_eq!(payloads.album_statements().unwrap(), AlbumLinks::Unread);
}

#[test]
fn documents_that_link_nothing_name_no_album() {
    let payloads = release(&[], None, vec![group_document(&[])]);
    assert_eq!(payloads.album_statements().unwrap(), AlbumLinks::Read(Vec::new()));
}
