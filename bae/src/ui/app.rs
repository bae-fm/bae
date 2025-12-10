use dioxus::desktop::{wry, Config as DioxusConfig, WindowBuilder};
use dioxus::prelude::*;
use std::borrow::Cow;
use tracing::{debug, warn};
use wry::http::Response as HttpResponse;

use crate::cache::CacheManager;
use crate::cloud_storage::CloudStorageManager;
use crate::encryption::EncryptionService;
use crate::library::SharedLibraryManager;
use crate::ui::components::import::ImportWorkflowManager;
use crate::ui::components::*;
#[cfg(target_os = "macos")]
use crate::ui::window_activation::setup_macos_window_activation;
use crate::ui::AppContext;

pub const FAVICON: Asset = asset!("/assets/favicon.ico");
pub const MAIN_CSS: Asset = asset!("/assets/main.css");
pub const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");

#[derive(Debug, Clone, Routable, PartialEq)]
#[rustfmt::skip]
pub enum Route {
    #[layout(Navbar)]
    #[route("/")]
    Library {},
    #[route("/album/:album_id?:release_id")]
    AlbumDetail { 
        album_id: String,
        release_id: String,
    },
    #[route("/import")]
    ImportWorkflowManager {},
    #[route("/settings")]
    Settings {},
}

/// Get MIME type from file extension
fn mime_type_for_extension(ext: &str) -> &'static str {
    match ext.to_lowercase().as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "svg" => "image/svg+xml",
        "tiff" | "tif" => "image/tiff",
        _ => "application/octet-stream",
    }
}

/// Services needed for image reconstruction from chunks
#[derive(Clone)]
struct ImageServices {
    library_manager: SharedLibraryManager,
    cloud_storage: CloudStorageManager,
    cache: CacheManager,
    encryption_service: EncryptionService,
    chunk_size_bytes: usize,
}

pub fn make_config(context: &AppContext, chunk_size_bytes: usize) -> DioxusConfig {
    let services = ImageServices {
        library_manager: context.library_manager.clone(),
        cloud_storage: context.cloud_storage.clone(),
        cache: context.cache.clone(),
        encryption_service: context.encryption_service.clone(),
        chunk_size_bytes,
    };

    DioxusConfig::default()
        .with_window(make_window())
        // Enable native file drop handler (false = don't disable) to get full file paths
        // On macOS/Linux: Native handler captures paths and merges them with HTML drag events
        // On Windows: Native handler captures paths and uses WindowsDragDrop events to bridge to HTML drag events
        .with_disable_drag_drop_handler(false)
        // Custom protocol for serving local files and images from chunk storage
        // Usage: bae://local/path/to/file.jpg or bae://image/{image_id}
        .with_custom_protocol("bae", move |_webview_id, request| {
            let uri = request.uri().to_string();

            if uri.starts_with("bae://local") {
                // Serve local file
                let encoded_path = uri.strip_prefix("bae://local").unwrap_or("");
                let path = urlencoding::decode(encoded_path)
                    .map(|s| s.into_owned())
                    .unwrap_or_else(|_| encoded_path.to_string());

                match std::fs::read(&path) {
                    Ok(data) => {
                        let mime_type = std::path::Path::new(&path)
                            .extension()
                            .and_then(|e| e.to_str())
                            .map(mime_type_for_extension)
                            .unwrap_or("application/octet-stream");

                        HttpResponse::builder()
                            .status(200)
                            .header("Content-Type", mime_type)
                            .body(Cow::Owned(data))
                            .unwrap()
                    }
                    Err(e) => {
                        warn!("Failed to read file {}: {}", path, e);
                        HttpResponse::builder()
                            .status(404)
                            .body(Cow::Borrowed(b"File not found" as &[u8]))
                            .unwrap()
                    }
                }
            } else if uri.starts_with("bae://image/") {
                // Serve image from chunk storage
                let image_id = uri.strip_prefix("bae://image/").unwrap_or("");
                if image_id.is_empty() {
                    return HttpResponse::builder()
                        .status(400)
                        .body(Cow::Borrowed(b"Missing image ID" as &[u8]))
                        .unwrap();
                }

                // Use block_on to run async code in sync context
                let result = tokio::runtime::Handle::current()
                    .block_on(serve_image_from_chunks(image_id, &services));

                match result {
                    Ok((data, mime_type)) => HttpResponse::builder()
                        .status(200)
                        .header("Content-Type", mime_type)
                        .body(Cow::Owned(data))
                        .unwrap(),
                    Err(e) => {
                        warn!("Failed to serve image {}: {}", image_id, e);
                        HttpResponse::builder()
                            .status(404)
                            .body(Cow::Owned(format!("Image not found: {}", e).into_bytes()))
                            .unwrap()
                    }
                }
            } else {
                warn!("Invalid bae:// URL: {}", uri);
                HttpResponse::builder()
                    .status(400)
                    .body(Cow::Borrowed(b"Invalid URL" as &[u8]))
                    .unwrap()
            }
        })
}

/// Reconstruct an image from chunk storage
async fn serve_image_from_chunks(
    image_id: &str,
    services: &ImageServices,
) -> Result<(Vec<u8>, &'static str), String> {
    debug!("Serving image from chunks: {}", image_id);

    // 1. Look up the image
    let image = services
        .library_manager
        .get()
        .get_image_by_id(image_id)
        .await
        .map_err(|e| format!("Database error: {}", e))?
        .ok_or_else(|| format!("Image not found: {}", image_id))?;

    // 2. Find the file record for this image
    let file = services
        .library_manager
        .get()
        .get_file_by_release_and_filename(&image.release_id, &image.filename)
        .await
        .map_err(|e| format!("Database error: {}", e))?
        .ok_or_else(|| format!("File not found for image: {}", image.filename))?;

    // 3. Get all files for the release to calculate byte offsets
    let all_files = services
        .library_manager
        .get()
        .get_files_for_release(&image.release_id)
        .await
        .map_err(|e| format!("Database error: {}", e))?;

    // 4. Calculate where this file starts in the chunk stream
    // Files are stored sequentially in order
    let mut byte_offset: i64 = 0;
    for f in &all_files {
        if f.id == file.id {
            break;
        }
        byte_offset += f.file_size;
    }
    let file_size = file.file_size as usize;

    // 5. Calculate which chunks we need
    let chunk_size = services.chunk_size_bytes as i64;
    let start_chunk_index = (byte_offset / chunk_size) as i32;
    let end_chunk_index = ((byte_offset + file_size as i64 - 1) / chunk_size) as i32;

    debug!(
        "Image {} spans chunks {}-{}, byte offset {} size {}",
        image_id, start_chunk_index, end_chunk_index, byte_offset, file_size
    );

    // 6. Get the required chunks
    let chunks = services
        .library_manager
        .get()
        .get_chunks_in_range(&image.release_id, start_chunk_index..=end_chunk_index)
        .await
        .map_err(|e| format!("Database error: {}", e))?;

    if chunks.is_empty() {
        return Err("No chunks found for image".to_string());
    }

    // 7. Download and decrypt chunks
    let mut chunk_data_vec: Vec<(i32, Vec<u8>)> = Vec::new();
    for chunk in &chunks {
        let encrypted_data = match services.cache.get_chunk(&chunk.id).await {
            Ok(Some(data)) => data,
            Ok(None) => {
                // Download from cloud
                services
                    .cloud_storage
                    .download_chunk(&chunk.storage_location)
                    .await
                    .map_err(|e| format!("Failed to download chunk: {}", e))?
            }
            Err(e) => {
                warn!("Cache error: {}", e);
                services
                    .cloud_storage
                    .download_chunk(&chunk.storage_location)
                    .await
                    .map_err(|e| format!("Failed to download chunk: {}", e))?
            }
        };

        let decrypted = services
            .encryption_service
            .decrypt_chunk(&encrypted_data)
            .map_err(|e| format!("Failed to decrypt chunk: {}", e))?;

        chunk_data_vec.push((chunk.chunk_index, decrypted));
    }

    // Sort by chunk index
    chunk_data_vec.sort_by_key(|(idx, _)| *idx);
    let sorted_chunks: Vec<Vec<u8>> = chunk_data_vec.into_iter().map(|(_, data)| data).collect();

    // 8. Extract file bytes
    let start_byte_in_first_chunk = (byte_offset % chunk_size) as usize;
    let mut file_data = Vec::with_capacity(file_size);

    if sorted_chunks.len() == 1 {
        // File is entirely within a single chunk
        let end = start_byte_in_first_chunk + file_size;
        file_data.extend_from_slice(&sorted_chunks[0][start_byte_in_first_chunk..end]);
    } else {
        // File spans multiple chunks
        // First chunk: from offset to end
        file_data.extend_from_slice(&sorted_chunks[0][start_byte_in_first_chunk..]);

        // Middle chunks: entire chunks
        for chunk in &sorted_chunks[1..sorted_chunks.len() - 1] {
            file_data.extend_from_slice(chunk);
        }

        // Last chunk: from start to remaining bytes
        let remaining = file_size - file_data.len();
        file_data.extend_from_slice(&sorted_chunks[sorted_chunks.len() - 1][..remaining]);
    }

    // 9. Determine MIME type from filename
    let mime_type = std::path::Path::new(&image.filename)
        .extension()
        .and_then(|e| e.to_str())
        .map(mime_type_for_extension)
        .unwrap_or("application/octet-stream");

    debug!("Served image {} ({} bytes)", image_id, file_data.len());

    Ok((file_data, mime_type))
}

fn make_window() -> WindowBuilder {
    WindowBuilder::new()
        .with_title("bae")
        .with_always_on_top(false)
        .with_decorations(true)
        .with_inner_size(dioxus::desktop::LogicalSize::new(1200, 800))
}

pub fn launch_app(context: AppContext) {
    #[cfg(target_os = "macos")]
    setup_macos_window_activation();

    let chunk_size_bytes = context.config.chunk_size_bytes;

    LaunchBuilder::desktop()
        .with_cfg(make_config(&context, chunk_size_bytes))
        .with_context_provider(move || Box::new(context.clone()))
        .launch(App);
}
