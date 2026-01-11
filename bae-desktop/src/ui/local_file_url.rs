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

/// Convert a local file path to a bae://local URL.
///
/// The path is URL-encoded so it can contain spaces and special characters.
pub fn local_file_url(path: &Path) -> String {
    let path_str = path.to_string_lossy();
    let encoded = urlencoding::encode(&path_str);
    format!("bae://local{}", encoded)
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
        assert_eq!(url, "bae://local%2Fpath%2Fto%2Ffile.jpg");
    }

    #[test]
    fn test_local_file_url_with_spaces() {
        let url = local_file_url(Path::new("/Users/test/My Music/cover.jpg"));
        assert_eq!(url, "bae://local%2FUsers%2Ftest%2FMy%20Music%2Fcover.jpg");
    }
}
