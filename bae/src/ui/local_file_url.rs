//! Helper for converting image IDs to bae:// URLs
//!
//! The bae:// custom protocol is registered in app.rs and serves:
//! - Images from storage: bae://image/{image_id}

/// Convert a DbImage ID to a bae:// URL for serving from storage.
///
/// The image will be read and decrypted on demand.
///
/// # Example
/// ```
/// # use bae::ui::local_file_url::image_url;
/// let url = image_url("abc123-def456");
/// assert_eq!(url, "bae://image/abc123-def456");
/// ```
pub fn image_url(image_id: &str) -> String {
    format!("bae://image/{}", image_id)
}
