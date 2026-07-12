use crate::db::{DbLibraryImage, LibraryImageType};
use crate::discogs::DiscogsClient;
use crate::import::MetadataSource;
use crate::library::LibraryManager;
use tracing::{debug, info, warn};

/// Download an artist's primary image from Discogs and build the row describing
/// those bytes; finalize writes the row and the blob atomically with the release
/// import. The caller (`fetch_artist_images`) already skips artists that have an
/// image row, so this assumes none exists.
///
/// Best-effort: a failure logs a warning and returns `None`, never failing the
/// import.
pub async fn fetch_artist_image(
    artist_id: &str,
    discogs_artist_id: &str,
    discogs_client: &DiscogsClient,
    library_manager: &LibraryManager,
) -> Option<(DbLibraryImage, Vec<u8>)> {
    let image_url = match discogs_client.get_artist_image(discogs_artist_id).await {
        Ok(Some(url)) => url,
        Ok(None) => {
            debug!("No image found for Discogs artist {}", discogs_artist_id);
            return None;
        }
        Err(e) => {
            warn!(
                "Failed to fetch artist image URL from Discogs for artist {artist_id} (Discogs {discogs_artist_id}): {e}"
            );
            return None;
        }
    };

    let (bytes, content_type) = match crate::import::cover_art::download_image_bytes(
        &image_url,
        "Artist image download",
    )
    .await
    {
        Ok(download) => download,
        Err(e) => {
            warn!(
                "Failed to download artist image for artist {artist_id} (Discogs {discogs_artist_id}) from {image_url}: {e}"
            );
            return None;
        }
    };

    let now = library_manager.clock().now();
    let db_image = DbLibraryImage {
        id: artist_id.to_string(),
        image_type: LibraryImageType::Artist,
        content_type,
        file_size: bytes.len() as i64,
        width: None,
        height: None,
        source: MetadataSource::Discogs.as_str().to_string(),
        source_url: Some(image_url),
        cloud_path: None,
        created_at: now,
    };

    info!(
        "Fetched artist image ({} bytes) for artist {artist_id}",
        bytes.len()
    );

    Some((db_image, bytes))
}
