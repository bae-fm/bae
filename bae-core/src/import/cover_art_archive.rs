//! Complete Cover Art Archive galleries, fetched only when a picker opens.

use super::{
    archive_base, push_unique_cover, send_artwork_request, ImportError, MetadataSource,
    RemoteCover, RETRY_BASE_DELAY,
};
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
    release_id: &str,
    group_id: Option<&str>,
) -> Result<Vec<RemoteCover>, ImportError> {
    let base = archive_base();
    fetch_gallery_set(&base, release_id, group_id).await
}

async fn fetch_gallery_set(
    base: &str,
    release_id: &str,
    group_id: Option<&str>,
) -> Result<Vec<RemoteCover>, ImportError> {
    let mut covers = fetch_gallery(&format!("{base}/release/{release_id}/")).await?;
    if let Some(group_id) = group_id {
        for cover in fetch_gallery(&format!("{base}/release-group/{group_id}/")).await? {
            push_unique_cover(&mut covers, cover);
        }
    }
    Ok(covers)
}

async fn fetch_gallery(url: &str) -> Result<Vec<RemoteCover>, ImportError> {
    let Some(response) =
        send_artwork_request(url, "Cover Art Archive gallery", RETRY_BASE_DELAY).await?
    else {
        return Ok(Vec::new());
    };
    let bytes = crate::util::http::read_body_capped(response, 4 * 1024 * 1024)
        .await
        .map_err(|error| ImportError::CoverArt {
            detail: format!("Failed to read Cover Art Archive gallery: {error}"),
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
        let thumbnail_url = match image
            .thumbnails
            .get("250")
            .or_else(|| image.thumbnails.get("small"))
        {
            Some(url) => url.clone(),
            None => {
                tracing::debug!(url = %image.image, "Archive image has no thumbnail; previewing its original");
                image.image.clone()
            }
        };
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
                url: image.image,
                thumbnail_url,
                label,
                source: MetadataSource::MusicBrainz,
            },
        );
    }
    Ok(covers)
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
        let covers = fetch_gallery_set(&base, "release-1", Some("group-1"))
            .await
            .expect("both galleries load");
        assert_eq!(covers.len(), 3);
        assert_eq!(covers[2].url, "https://images.example/booklet.jpg");
        assert!(fetch_gallery_set(&base, "missing", None)
            .await
            .expect("404 means no artwork")
            .is_empty());
        assert!(fetch_gallery_set(&base, "failed", None).await.is_err());
        server.abort();
    }

    #[test]
    fn archive_gallery_keeps_all_image_types_and_front_first() {
        let covers = parse_gallery(&serde_json::to_vec(&serde_json::json!({
            "images": [
                {"image":"https://images.example/back.jpg", "thumbnails":{}, "types":["Back"], "comment":"liner notes", "front":false},
                {"image":"https://images.example/front.jpg", "thumbnails":{"250":"https://images.example/front-small.jpg"}, "types":["Front"], "comment":"", "front":true},
                {"image":"https://images.example/booklet.jpg", "thumbnails":{}, "types":["Booklet"], "comment":"pages 1–2", "front":false}
            ]
        })).expect("fixture serializes")).expect("gallery parses");
        assert_eq!(covers.len(), 3);
        assert_eq!(covers[0].url, "https://images.example/front.jpg");
        assert_eq!(
            covers[0].thumbnail_url,
            "https://images.example/front-small.jpg"
        );
        assert!(covers[1].label.contains("Back · liner notes"));
        assert_eq!(covers[2].thumbnail_url, covers[2].url);
    }

    #[test]
    fn malformed_gallery_is_an_error_not_an_empty_gallery() {
        assert!(parse_gallery(br#"{"message":"unavailable"}"#).is_err());
    }
}
