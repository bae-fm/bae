use crate::db::{DbLibraryImage, LibraryImageType};
use crate::discogs::DiscogsClient;
use crate::import::MetadataSource;
use crate::library::LibraryManager;
use tracing::{debug, info, warn};

/// Fetch an artist image from Discogs.
///
/// Downloads the primary image and builds the row describing those bytes. The
/// caller already skips artists that have an image row, so this assumes none
/// exists. Finalize writes the row and blob atomically with the release import.
/// Best-effort: logs warnings on failure, never fails the import.
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
