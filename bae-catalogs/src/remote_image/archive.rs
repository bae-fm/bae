//! Complete Cover Art Archive galleries, fetched only when a picker opens.

use super::fetch::send_artwork_request;
use super::{
    push_unique_cover, CoverImageType, CoverLabel, CoverStanding, DownscaledCopy, RemoteCover,
    RemoteImageError, RemoteImageSet, ARCHIVE,
};
use crate::catalog::Catalog;
use bae_util::http::Http;
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
    types: Vec<CoverImageType>,
    comment: String,
    front: bool,
}

/// Release images first, followed by the release group's representative
/// gallery. Shared image URLs appear once even when both endpoints list them.
pub async fn musicbrainz_gallery(
    http: &Http,
    release_id: &str,
    group_id: Option<&str>,
) -> Result<Vec<RemoteCover>, RemoteImageError> {
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
) -> Result<Vec<RemoteCover>, RemoteImageError> {
    fetch_gallery(http, &format!("{ARCHIVE}/release-group/{group_id}/")).await
}

async fn fetch_gallery(http: &Http, url: &str) -> Result<Vec<RemoteCover>, RemoteImageError> {
    let Some(response) = send_artwork_request(http, url, "Cover Art Archive gallery").await? else {
        return Ok(Vec::new());
    };
    let bytes =
        bae_util::http::read_body_capped(response, 4 * 1024 * 1024).await.map_err(|error| {
            super::fetch::artwork_body_error(error, "Failed to read Cover Art Archive gallery")
        })?;
    parse_gallery(&bytes)
}

fn parse_gallery(bytes: &[u8]) -> Result<Vec<RemoteCover>, RemoteImageError> {
    let mut gallery: ArchiveGallery = serde_json::from_slice(bytes).map_err(|error| {
        RemoteImageError::Unusable { detail: format!("Invalid Cover Art Archive gallery: {error}") }
    })?;
    gallery.images.sort_by_key(|image| !image.front);
    let mut covers = Vec::new();
    for (index, image) in gallery.images.into_iter().enumerate() {
        let copies = downscaled_copies(&image.thumbnails);
        if copies.is_empty() {
            tracing::debug!(url = %image.image, "Archive image lists no downscaled copies; every slot reads its original");
        }
        push_unique_cover(
            &mut covers,
            RemoteCover {
                image: RemoteImageSet::with_copies(image.image, copies),
                label: CoverLabel::Gallery {
                    reference: None,
                    position: index as u32 + 1,
                    types: image.types,
                    comment: image.comment,
                },
                source: Catalog::MusicBrainz,
                // The archive's own listing of what it holds.
                standing: CoverStanding::Stated,
            },
        );
    }
    Ok(covers)
}

/// The copies a gallery image's `thumbnails` map lists, keyed by the box they
/// fit in. The archive names each by its edge (`250`, `500`, `1200`) and keeps
/// two older names for the same files: `small` is the 250 copy and `large` the
/// 500 one. An edge-named entry is the one read; an older name stands in only
/// for an edge the map does not name.
fn downscaled_copies(thumbnails: &HashMap<String, String>) -> Vec<DownscaledCopy> {
    let mut copies: Vec<DownscaledCopy> = thumbnails
        .iter()
        .filter_map(|(key, url)| {
            Some(DownscaledCopy { url: url.clone(), max_edge: key.parse().ok()? })
        })
        .collect();
    for (alias, max_edge) in [("small", 250), ("large", 500)] {
        if let Some(url) = thumbnails.get(alias) {
            if !copies.iter().any(|copy| copy.max_edge == max_edge) {
                copies.push(DownscaledCopy { url: url.clone(), max_edge });
            }
        }
    }
    copies
}

#[cfg(test)]
#[path = "archive_tests.rs"]
mod tests;
