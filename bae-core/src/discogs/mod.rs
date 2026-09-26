pub mod client;
pub mod models;
use crate::import::cover_art::{DownscaledCopy, RemoteCover, RemoteImageSet};
use crate::import::Catalog;
pub use client::DiscogsClient;
pub use models::*;
use std::fmt::Display;
use tracing::debug;

/// Discogs packs the artist and album into one title field, separated by " - ".
/// `None` when there's no separator; an empty artist half also comes back `None`.
pub(crate) fn split_title(title: &str) -> Option<(Option<&str>, &str)> {
    title
        .split_once(" - ")
        .map(|(a, b)| (a.trim(), b.trim()))
        .map(|(artist, album)| {
            let artist = if artist.is_empty() {
                None
            } else {
                Some(artist)
            };
            (artist, album)
        })
}

/// The box Discogs fits its thumbnails in: an image's `uri150` and a search
/// result's `thumb` are both served at most 150 pixels on either side.
const DISCOGS_THUMBNAIL_EDGE: u32 = 150;

/// A Discogs cover from the two addresses Discogs gives an image: the image
/// (`uri`, or a search result's `cover_image`) and its thumbnail (`uri150`, or
/// `thumb`). A cover with only a thumbnail is that thumbnail at its one size.
pub(crate) fn remote_cover_from_urls<I>(
    cover_image: Option<&str>,
    thumb: Option<&str>,
    entity: &str,
    id: I,
) -> Option<RemoteCover>
where
    I: Display + Copy,
{
    let image = match (cover_image, thumb) {
        (Some(url), Some(thumb)) => RemoteImageSet::with_copies(
            url.to_string(),
            vec![DownscaledCopy {
                url: thumb.to_string(),
                max_edge: DISCOGS_THUMBNAIL_EDGE,
            }],
        ),
        (Some(url), None) => {
            debug!(
                discogs_entity = entity,
                discogs_id = %id,
                "Discogs cover has no thumbnail URL; every slot reads the image"
            );
            RemoteImageSet::original(url.to_string())
        }
        (None, Some(thumb)) => {
            debug!(
                discogs_entity = entity,
                discogs_id = %id,
                "Discogs cover has no cover image URL; using thumbnail URL"
            );
            RemoteImageSet::original(thumb.to_string())
        }
        (None, None) => {
            debug!(
                discogs_entity = entity,
                discogs_id = %id,
                "Discogs cover has no cover image or thumbnail URL; skipping remote cover"
            );
            return None;
        }
    };

    Some(RemoteCover {
        image,
        label: Catalog::Discogs.cover_source_label().to_string(),
        source: Catalog::Discogs,
    })
}
