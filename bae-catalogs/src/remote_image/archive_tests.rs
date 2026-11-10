#![expect(
    clippy::disallowed_methods,
    reason = "test code: what it spawns ends with the test, which has no crash reporter to report to"
)]

use super::*;

#[tokio::test]
async fn archive_gallery_requests_both_entities_and_deduplicates_images() {
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
        .route("/release/release-1/", get(move || async { Json(release) }))
        .route("/release-group/group-1/", get(move || async { Json(group) }))
        .route("/release/failed/", get(|| async { axum::http::StatusCode::BAD_REQUEST }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("listener binds");
    let base = format!("http://{}", listener.local_addr().expect("listener address"));
    let server =
        tokio::spawn(async move { axum::serve(listener, app).await.expect("server runs") });
    let http = Http::for_test().serve("coverartarchive.org", &base);
    let covers = musicbrainz_gallery(&http, "release-1", Some("group-1"))
        .await
        .expect("both galleries load");
    assert_eq!(covers.len(), 3);
    assert_eq!(covers[2].image.url, "https://images.example/booklet.jpg");
    assert!(musicbrainz_gallery(&http, "missing", None)
        .await
        .expect("404 means no artwork")
        .is_empty());
    assert!(musicbrainz_gallery(&http, "failed", None).await.is_err());
    server.abort();
}

#[test]
fn archive_gallery_keeps_all_image_types_and_front_first() {
    let covers = parse_gallery(&serde_json::to_vec(&serde_json::json!({
        "images": [
            {"image":"https://images.example/back.jpg", "thumbnails":{}, "types":["Back"], "comment":"liner notes", "front":false},
            {"image":"https://images.example/front.jpg", "thumbnails":{
                "1200":"https://images.example/front-1200.jpg",
                "250":"https://images.example/front-250.jpg",
                "500":"https://images.example/front-500.jpg",
                "large":"https://images.example/front-500.jpg",
                "small":"https://images.example/front-250.jpg"
            }, "types":["Front"], "comment":"", "front":true},
            {"image":"https://images.example/booklet.jpg", "thumbnails":{}, "types":["Booklet"], "comment":"pages 1–2", "front":false}
        ]
    })).expect("fixture serializes")).expect("gallery parses");
    assert_eq!(covers.len(), 3);
    assert_eq!(
        covers[0].image,
        RemoteImageSet {
            url: "https://images.example/front.jpg".to_string(),
            downscaled: [250, 500, 1200]
                .map(|edge| DownscaledCopy {
                    url: format!("https://images.example/front-{edge}.jpg"),
                    max_edge: edge,
                })
                .to_vec(),
        }
    );
    assert!(covers[1].label.contains("Back · liner notes"));
    assert!(covers[2].image.downscaled.is_empty());
}

#[test]
fn an_edge_named_copy_wins_over_its_older_name() {
    let covers = parse_gallery(
        &serde_json::to_vec(&serde_json::json!({
            "images": [
                {"image":"https://images.example/front.jpg", "thumbnails":{
                    "250":"https://images.example/front-250.jpg",
                    "small":"https://images.example/front-small.jpg"
                }, "types":["Front"], "comment":"", "front":true}
            ]
        }))
        .expect("fixture serializes"),
    )
    .expect("gallery parses");
    assert_eq!(
        covers[0].image.downscaled,
        vec![DownscaledCopy {
            url: "https://images.example/front-250.jpg".to_string(),
            max_edge: 250,
        }]
    );
}

#[test]
fn archive_gallery_reads_the_older_copy_names_by_their_edges() {
    let covers = parse_gallery(
        &serde_json::to_vec(&serde_json::json!({
            "images": [
                {"image":"https://images.example/front.jpg", "thumbnails":{
                    "large":"https://images.example/front-large.jpg",
                    "small":"https://images.example/front-small.jpg"
                }, "types":["Front"], "comment":"", "front":true}
            ]
        }))
        .expect("fixture serializes"),
    )
    .expect("gallery parses");
    let image = &covers[0].image;
    assert_eq!(image.url_covering(Some(200)), "https://images.example/front-small.jpg");
    assert_eq!(image.url_covering(Some(400)), "https://images.example/front-large.jpg");
    assert_eq!(image.url_covering(Some(520)), "https://images.example/front.jpg");
}

#[tokio::test]
async fn gallery_interrupted_body_is_a_transport_failure() {
    let url = crate::remote_image::tests::truncated_body_url().await;
    let http = crate::remote_image::tests::direct_http();
    let error = fetch_gallery(&http, &url).await.unwrap_err();
    assert!(
        matches!(&error, RemoteImageError::Transport { error, .. } if error.is_decode()),
        "{error}"
    );
}

#[test]
fn malformed_gallery_is_an_error_not_an_empty_gallery() {
    assert!(parse_gallery(br#"{"message":"unavailable"}"#).is_err());
}
