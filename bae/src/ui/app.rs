use crate::encryption::EncryptionService;
use crate::library::SharedLibraryManager;
use crate::storage::create_storage_reader;
use crate::ui::components::import::ImportWorkflowManager;
use crate::ui::components::*;
#[cfg(target_os = "macos")]
use crate::ui::window_activation::setup_macos_window_activation;
use crate::ui::AppContext;
use dioxus::desktop::{wry, Config as DioxusConfig, WindowBuilder};
use dioxus::prelude::*;
use std::borrow::Cow;
use tracing::{debug, warn};
use wry::http::Response as HttpResponse;
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
    AlbumDetail { album_id: String, release_id: String },
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
/// Services needed for image retrieval
#[derive(Clone)]
struct ImageServices {
    library_manager: SharedLibraryManager,
    encryption_service: EncryptionService,
}
pub fn make_config(context: &AppContext) -> DioxusConfig {
    let services = ImageServices {
        library_manager: context.library_manager.clone(),
        encryption_service: context.encryption_service.clone(),
    };
    DioxusConfig::default()
        .with_window(make_window())
        .with_disable_drag_drop_handler(false)
        .with_custom_protocol("bae", move |_webview_id, request| {
            let uri = request.uri().to_string();
            if uri.starts_with("bae://local") {
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
                let image_id = uri.strip_prefix("bae://image/").unwrap_or("");
                if image_id.is_empty() {
                    return HttpResponse::builder()
                        .status(400)
                        .body(Cow::Borrowed(b"Missing image ID" as &[u8]))
                        .unwrap();
                }
                let services_clone = services.clone();
                let image_id_owned = image_id.to_string();
                let result = std::thread::spawn(move || {
                    let rt = tokio::runtime::Runtime::new().unwrap();
                    rt.block_on(serve_image(&image_id_owned, &services_clone))
                })
                .join()
                .unwrap_or_else(|_| Err("Thread panicked".to_string()));
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
/// Serve an image from storage
async fn serve_image(
    image_id: &str,
    services: &ImageServices,
) -> Result<(Vec<u8>, &'static str), String> {
    debug!("Serving image: {}", image_id);
    let image = services
        .library_manager
        .get()
        .get_image_by_id(image_id)
        .await
        .map_err(|e| format!("Database error: {}", e))?
        .ok_or_else(|| format!("Image not found: {}", image_id))?;
    let filename_only = std::path::Path::new(&image.filename)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&image.filename);
    let file = services
        .library_manager
        .get()
        .get_file_by_release_and_filename(&image.release_id, filename_only)
        .await
        .map_err(|e| format!("Database error: {}", e))?
        .ok_or_else(|| format!("File not found for image: {}", image.filename))?;
    if let Some(ref source_path) = file.source_path {
        debug!("Serving image from storage: {}", source_path);
        let storage_profile = services
            .library_manager
            .get()
            .get_storage_profile_for_release(&image.release_id)
            .await
            .map_err(|e| format!("Database error: {}", e))?;
        let raw_data = if source_path.starts_with("s3://") {
            let profile = storage_profile
                .as_ref()
                .ok_or_else(|| "No storage profile for cloud image".to_string())?;
            let storage = create_storage_reader(profile)
                .await
                .map_err(|e| format!("Failed to create storage reader: {}", e))?;
            storage
                .download(source_path)
                .await
                .map_err(|e| format!("Failed to download image from cloud: {}", e))?
        } else {
            tokio::fs::read(source_path)
                .await
                .map_err(|e| format!("Failed to read image file: {}", e))?
        };
        let data = if storage_profile
            .as_ref()
            .map(|p| p.encrypted)
            .unwrap_or(false)
        {
            services
                .encryption_service
                .decrypt(&raw_data)
                .map_err(|e| format!("Failed to decrypt image: {}", e))?
        } else {
            raw_data
        };
        let mime_type = match image.filename.rsplit('.').next() {
            Some("jpg") | Some("jpeg") => "image/jpeg",
            Some("png") => "image/png",
            Some("gif") => "image/gif",
            Some("webp") => "image/webp",
            _ => "application/octet-stream",
        };
        return Ok((data, mime_type));
    }

    // File has no source_path, which shouldn't happen
    Err(format!(
        "File {} has no source_path",
        file.original_filename
    ))
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
    LaunchBuilder::desktop()
        .with_cfg(make_config(&context))
        .with_context_provider(move || Box::new(context.clone()))
        .launch(App);
}
