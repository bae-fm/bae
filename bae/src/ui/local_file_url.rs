//! Helper for converting local file paths and image IDs to bae:// URLs
//!
//! The bae:// custom protocol is registered in app.rs and serves:
//! - Local files: bae://local/path/to/file
//! - Images from chunk storage: bae://image/{image_id}

/// Convert a local file path to a bae:// URL for serving via custom protocol.
///
/// The path will be URL-encoded to handle special characters like spaces,
/// but forward slashes are preserved to maintain the path structure.
///
/// # Example
/// ```
/// let url = local_file_url("/Users/me/Music/cover.jpg");
/// // Returns: bae://local/Users/me/Music/cover.jpg
/// ```
pub fn local_file_url(path: &str) -> String {
    // Encode each path segment separately to preserve slashes
    let encoded_path: String = path
        .split('/')
        .map(|segment| urlencoding::encode(segment).into_owned())
        .collect::<Vec<_>>()
        .join("/");
    format!("bae://local{}", encoded_path)
}

/// Convert a DbImage ID to a bae:// URL for serving from chunk storage.
///
/// The image will be reconstructed from encrypted chunks on demand.
///
/// # Example
/// ```
/// let url = image_url("abc123-def456");
/// // Returns: bae://image/abc123-def456
/// ```
pub fn image_url(image_id: &str) -> String {
    format!("bae://image/{}", image_id)
}
