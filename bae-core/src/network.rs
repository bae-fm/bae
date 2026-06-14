/// Upgrade HTTP URLs to HTTPS for App Transport Security compliance
pub fn upgrade_to_https(url: &str) -> String {
    if url.starts_with("http://") {
        url.replace("http://", "https://")
    } else {
        url.to_string()
    }
}
