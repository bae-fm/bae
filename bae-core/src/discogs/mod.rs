pub mod client;
pub mod models;
use crate::import::cover_art::RemoteCover;
use crate::import::MetadataSource;
pub use client::DiscogsClient;
pub use models::*;
use std::fmt::Display;
use tracing::debug;

/// Split a Discogs "Artist - Album" title into its trimmed `(artist, album)`
/// halves. Discogs packs both into one title field separated by " - ". Returns
/// `None` when there's no separator; callers pick their own fallback.
pub(crate) fn split_title(title: &str) -> Option<(&str, &str)> {
    title.split_once(" - ").map(|(a, b)| (a.trim(), b.trim()))
}

/// Map `Some("")` → `None`. Discogs returns empty strings (`""`)
/// instead of `null` for missing URL fields (`thumb`, `cover_image`,
/// etc.); serde deserializes those into `Some(String::new())` which
/// then leaks downstream as a "valid" URL until something tries to
/// parse it. Apply with
/// `#[serde(default, deserialize_with = "empty_string_as_none")]` on
/// any `Option<String>` field where empty has only one meaning
/// (absence).
pub(crate) fn empty_string_as_none<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;
    Ok(Option::<String>::deserialize(deserializer)?.filter(|s| !s.is_empty()))
}

pub(crate) fn remote_cover_from_urls<I>(
    cover_image: Option<&str>,
    thumb: Option<&str>,
    entity: &str,
    id: I,
) -> Option<RemoteCover>
where
    I: Display + Copy,
{
    let url = match (cover_image, thumb) {
        (Some(url), _) => url.to_string(),
        (None, Some(thumb)) => {
            debug!(
                discogs_entity = entity,
                discogs_id = %id,
                "Discogs cover has no cover image URL; using thumbnail URL"
            );
            thumb.to_string()
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
    let thumbnail_url = match thumb {
        Some(thumb) => thumb.to_string(),
        None => {
            debug!(
                discogs_entity = entity,
                discogs_id = %id,
                "Discogs cover has no thumbnail URL; using image URL"
            );
            url.clone()
        }
    };

    Some(RemoteCover {
        url,
        thumbnail_url,
        label: MetadataSource::Discogs.cover_source_label().to_string(),
        source: MetadataSource::Discogs,
    })
}
