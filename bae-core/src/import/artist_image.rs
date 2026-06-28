use crate::db::{DbLibraryImage, LibraryImageType};
use crate::discogs::DiscogsClient;
use crate::import::MetadataSource;
use crate::library::LibraryManager;
use tracing::{debug, info, warn};

/// Fetch and save an artist image from Discogs.
///
/// Downloads the primary image and hands its bytes to coven's local store (a
/// host-provided Local blob), then writes the `artist_images` row. The caller
/// already skips artists that have an image row, so this assumes none exists.
/// Best-effort: logs warnings on failure, never fails the import.
///
/// Returns true if an image was saved successfully.
pub async fn fetch_and_save_artist_image(
    artist_id: &str,
    discogs_artist_id: &str,
    discogs_client: &DiscogsClient,
    library_manager: &LibraryManager,
) -> bool {
    let image_url = match discogs_client.get_artist_image(discogs_artist_id).await {
        Ok(Some(url)) => url,
        Ok(None) => {
            debug!("No image found for Discogs artist {}", discogs_artist_id);
            return false;
        }
        Err(e) => {
            warn!("Failed to fetch artist image URL from Discogs: {}", e);
            return false;
        }
    };

    // Download the image
    let client = match crate::util::http::client_builder().build() {
        Ok(c) => c,
        Err(e) => {
            warn!("Failed to create HTTP client for artist image: {}", e);
            return false;
        }
    };

    let response = match client.get(&image_url).send().await {
        Ok(r) => r,
        Err(e) => {
            warn!("Failed to download artist image: {}", e);
            return false;
        }
    };

    if !response.status().is_success() {
        warn!(
            "Artist image download returned status {}",
            response.status()
        );
        return false;
    }

    let content_type =
        super::image_response::image_content_type_from_response(response.headers(), &image_url);

    let bytes = match response.bytes().await {
        Ok(b) => b,
        Err(e) => {
            warn!("Failed to read artist image bytes: {}", e);
            return false;
        }
    };

    if bytes.len() < 100 {
        warn!("Downloaded artist image too small ({} bytes)", bytes.len());
        return false;
    }

    let now = library_manager.clock().now();
    // Under a browsable home the artist image blob lands at an `{artist_id}/
    // artist.{ext}` key, stored here; an opaque home leaves `cloud_path` NULL
    // (hashed-by-id).
    let cloud_path = library_manager.artist_image_cloud_path(artist_id, &content_type);
    let db_image = DbLibraryImage {
        id: artist_id.to_string(),
        image_type: LibraryImageType::Artist,
        content_type,
        file_size: bytes.len() as i64,
        width: None,
        height: None,
        source: MetadataSource::Discogs.as_str().to_string(),
        source_url: Some(image_url),
        cloud_path,
        created_at: now,
    };

    if let Err(e) = library_manager
        .store_library_image_blob(&db_image, &bytes)
        .await
    {
        warn!("Failed to store artist library image: {}", e);
        return false;
    }

    info!(
        "Saved artist image ({} bytes) for artist {artist_id}",
        bytes.len()
    );

    true
}
