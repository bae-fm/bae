//! Complete Cover Art Archive galleries, fetched only when a picker opens.

use super::{
    push_unique_cover, send_artwork_request, Catalog, DownscaledCopy, ImportError, RemoteCover,
    RemoteImageSet, ARCHIVE, RETRY_BASE_DELAY,
};
use crate::util::http::Http;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize)]
struct ArchiveGallery {
    images: Vec<ArchiveImage>,
}

#[derive(Deserialize)]
struct ArchiveImage {
    image: String,
    thumbnails: HashMap<String, String>,
    types: Vec<String>,
    comment: String,
    front: bool,
}

/// Release images first, followed by the release group's representative
/// gallery. Shared image URLs appear once even when both endpoints list them.
pub async fn musicbrainz_gallery(
    http: &Http,
    release_id: &str,
    group_id: Option<&str>,
) -> Result<Vec<RemoteCover>, ImportError> {
    let mut covers = fetch_gallery(http, &format!("{ARCHIVE}/release/{release_id}/")).await?;
    if let Some(group_id) = group_id {
        for cover in musicbrainz_group_gallery(http, group_id).await? {
            push_unique_cover(&mut covers, cover);
        }
    }
    Ok(covers)
}

/// A known album identity requests only its release-group gallery.
pub async fn musicbrainz_group_gallery(
    http: &Http,
    group_id: &str,
) -> Result<Vec<RemoteCover>, ImportError> {
    fetch_gallery(http, &format!("{ARCHIVE}/release-group/{group_id}/")).await
}

async fn fetch_gallery(http: &Http, url: &str) -> Result<Vec<RemoteCover>, ImportError> {
    let Some(response) =
        send_artwork_request(http, url, "Cover Art Archive gallery", RETRY_BASE_DELAY).await?
    else {
        return Ok(Vec::new());
    };
    let bytes = crate::util::http::read_body_capped(response, 4 * 1024 * 1024)
        .await
        .map_err(|error| {
            super::artwork_body_error(error, "Failed to read Cover Art Archive gallery")
        })?;
    parse_gallery(&bytes)
}

fn parse_gallery(bytes: &[u8]) -> Result<Vec<RemoteCover>, ImportError> {
    let mut gallery: ArchiveGallery =
        serde_json::from_slice(bytes).map_err(|error| ImportError::CoverArt {
            detail: format!("Invalid Cover Art Archive gallery: {error}"),
        })?;
    gallery.images.sort_by_key(|image| !image.front);
    let mut covers = Vec::new();
    for (index, image) in gallery.images.into_iter().enumerate() {
        let copies = downscaled_copies(&image.thumbnails);
        if copies.is_empty() {
            tracing::debug!(url = %image.image, "Archive image lists no downscaled copies; every slot reads its original");
        }
        let mut label = format!("Cover Art Archive · {}", index + 1);
        if !image.types.is_empty() {
            label.push_str(" · ");
            label.push_str(&image.types.join(", "));
        }
        if !image.comment.is_empty() {
            label.push_str(" · ");
            label.push_str(&image.comment);
        }
        push_unique_cover(
            &mut covers,
            RemoteCover {
                image: RemoteImageSet::with_copies(image.image, copies),
                label,
                source: Catalog::MusicBrainz,
            },
        );
    }
    Ok(covers)
}

/// The copies a gallery image's `thumbnails` map lists, keyed by the box they
/// fit in. The archive names each by its edge (`250`, `500`, `1200`) and keeps
/// two older names for the same files: `small` is the 250 copy and `large` the
/// 500 one.
fn downscaled_copies(thumbnails: &HashMap<String, String>) -> Vec<DownscaledCopy> {
    thumbnails
        .iter()
        .filter_map(|(key, url)| {
            let max_edge = match key.as_str() {
                "small" => 250,
                "large" => 500,
                edge => edge.parse().ok()?,
            };
            Some(DownscaledCopy {
                url: url.clone(),
                max_edge,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
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
            .route(
                "/release-group/group-1/",
                get(move || async { Json(group) }),
            )
            .route(
                "/release/failed/",
                get(|| async { axum::http::StatusCode::BAD_REQUEST }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener binds");
        let base = format!(
            "http://{}",
            listener.local_addr().expect("listener address")
        );
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
        assert_eq!(
            image.url_covering(Some(200)),
            "https://images.example/front-small.jpg"
        );
        assert_eq!(
            image.url_covering(Some(400)),
            "https://images.example/front-large.jpg"
        );
        assert_eq!(
            image.url_covering(Some(520)),
            "https://images.example/front.jpg"
        );
    }

    #[tokio::test]
    async fn gallery_interrupted_body_is_a_network_failure() {
        let url = super::super::tests::truncated_body_url().await;
        let http = Http::new().expect("the test HTTP client builds");
        let error = fetch_gallery(&http, &url).await.unwrap_err();
        assert_eq!(
            crate::import::search::import_error_to_lookup_failure(&error),
            crate::signals::LookupFailure::Network
        );
    }

    #[test]
    fn malformed_gallery_is_an_error_not_an_empty_gallery() {
        assert!(parse_gallery(br#"{"message":"unavailable"}"#).is_err());
    }
}
