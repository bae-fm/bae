use super::*;
use crate::payloads::ReleasePayloads;
use crate::records::{PayloadSource, SourcePayload};
use crate::test_documents::musicbrainz_release_with_relations;
use bae_util::http::Http;

/// A Cover Art Archive that holds `front` and `back` for the release
/// `mb-release` and `front` and `booklet` for its group `mb-group`, and the
/// transport that reaches it.
async fn archive() -> (Http, tokio::task::JoinHandle<()>) {
    use axum::{routing::get, Json, Router};
    let item = |name: &str| {
        serde_json::json!({
            "image": format!("https://images.example/{name}.jpg"),
            "thumbnails": {}, "types": [], "comment": "", "front": false
        })
    };
    let release = serde_json::json!({"images": [item("front"), item("back")]});
    let group = serde_json::json!({"images": [item("front"), item("booklet")]});
    let app = Router::new()
        .route("/release/mb-release/", get(move || async { Json(release) }))
        .route("/release-group/mb-group/", get(move || async { Json(group) }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("listener binds");
    let base = format!("http://{}", listener.local_addr().expect("listener address"));
    let server =
        tokio::spawn(async move { axum::serve(listener, app).await.expect("server runs") });
    (Http::for_test().serve("coverartarchive.org", &base), server)
}

fn discogs_release_with_one_image() -> String {
    serde_json::json!({
        "id": 123, "title": "Album Title",
        "images": [{ "type": "primary", "uri": "https://images.example/discogs-front.jpg" }]
    })
    .to_string()
}

fn urls(covers: &[RemoteCover]) -> Vec<&str> {
    covers.iter().map(|cover| cover.image.url.as_str()).collect()
}

/// The picker's gallery lists the primary's Cover Art Archive images first,
/// its release's before its group's, each once, then the partner's images.
#[tokio::test]
async fn a_picks_gallery_lists_the_primarys_images_first() {
    let (http, server) = archive().await;
    let primary = ReleasePayloads::for_test(
        LookupRef::new(LookupCatalog::MusicBrainz, "mb-release"),
        musicbrainz_release_with_relations("mb-release", "mb-group", &[]).to_string(),
        Vec::new(),
    )
    .extract()
    .expect("the primary extracts");
    let partner = ReleasePayloads::for_test(
        LookupRef::new(LookupCatalog::Discogs, "123"),
        discogs_release_with_one_image(),
        Vec::new(),
    )
    .extract()
    .expect("the partner extracts");

    let covers = pick_gallery_covers(&http, &primary, &[partner]).await.expect("galleries load");

    assert_eq!(
        urls(&covers),
        [
            "https://images.example/front.jpg",
            "https://images.example/back.jpg",
            "https://images.example/booklet.jpg",
            "https://images.example/discogs-front.jpg",
        ]
    );
    server.abort();
}

/// A Discogs primary offers the images its release stored before the archive
/// gallery of the MusicBrainz release it is cross-linked to.
#[tokio::test]
async fn a_discogs_primarys_own_images_come_before_its_archive_gallery() {
    let (http, server) = archive().await;
    let cross_linked = musicbrainz_release_with_relations(
        "mb-release",
        "mb-group",
        &["https://www.discogs.com/release/123"],
    )
    .to_string();
    let primary = ReleasePayloads::for_test(
        LookupRef::new(LookupCatalog::Discogs, "123"),
        discogs_release_with_one_image(),
        vec![SourcePayload::new(PayloadSource::MusicBrainzDiscogsXref, "123", cross_linked)],
    )
    .extract()
    .expect("the primary extracts");

    let covers = pick_gallery_covers(&http, &primary, &[]).await.expect("galleries load");

    assert_eq!(
        urls(&covers),
        [
            "https://images.example/discogs-front.jpg",
            "https://images.example/front.jpg",
            "https://images.example/back.jpg",
            "https://images.example/booklet.jpg",
        ]
    );
    server.abort();
}
