//! Helpers for generating bae:// URLs
//!
//! The bae:// custom protocol is registered in app.rs and serves:
//! - Images from storage: bae://image/{image_id}
//! - Local files: bae://local{url_encoded_path}

use std::path::Path;

/// Convert a DbImage ID to a bae:// URL for serving from storage.
///
/// The image will be read and decrypted on demand.
pub fn image_url(image_id: &str) -> String {
    format!("bae://image/{}", image_id)
}

/// Convert a local file path to a bae://local/path URL.
///
/// Path components are URL-encoded so they can contain spaces and special characters.
/// The slashes are preserved so the webview recognizes this as a valid URL path.
pub fn local_file_url(path: &Path) -> String {
    let encoded_segments: Vec<String> = path
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(s) => s.to_str(),
            _ => None,
        })
        .map(|s| urlencoding::encode(s).into_owned())
        .collect();
    format!("bae://local/{}", encoded_segments.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_image_url() {
        assert_eq!(image_url("abc123"), "bae://image/abc123");
    }

    #[test]
    fn test_local_file_url_simple() {
        let url = local_file_url(Path::new("/path/to/file.jpg"));
        assert_eq!(url, "bae://local/path/to/file.jpg");
    }

    #[test]
    fn test_local_file_url_with_spaces() {
        let url = local_file_url(Path::new("/Users/test/My Music/cover.jpg"));
        assert_eq!(url, "bae://local/Users/test/My%20Music/cover.jpg");
    }
}
