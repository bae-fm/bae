//! Shared content-type resolution for the image-fetch paths (cover art,
//! artist image).
use reqwest::header::{HeaderMap, CONTENT_TYPE};
use tracing::warn;

use crate::util::content_type::ContentType;
use crate::util::content_type_hint::ContentTypeHint;

/// Resolve an image's content type from an HTTP response's `CONTENT_TYPE`
/// header, falling back to guessing from the URL's file extension. Image
/// endpoints only ever return images, so a non-image result means a broken
/// URL — surfaced as `OctetStream` rather than a fabricated codec.
pub(crate) fn image_content_type_from_response(headers: &HeaderMap, url: &str) -> ContentType {
    headers
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .and_then(|ct| {
            let mime = ct.split(';').next().unwrap_or(ct).trim();
            mime.starts_with("image/")
                .then(|| ContentType::from_mime(mime))
        })
        .unwrap_or_else(|| {
            let ext = match reqwest::Url::parse(url) {
                Ok(parsed) => parsed
                    .path()
                    .rsplit('.')
                    .next()
                    .map(|e| e.to_lowercase())
                    .unwrap_or_default(),
                Err(e) => {
                    warn!("Failed to parse image URL {url}: {e}");
                    String::new()
                }
            };
            ContentTypeHint::from_extension(&ext)
                .image_content_type()
                .unwrap_or(ContentType::OctetStream)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers_with(content_type: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(CONTENT_TYPE, content_type.parse().unwrap());
        h
    }

    #[test]
    fn image_content_type_from_header_wins() {
        let h = headers_with("image/png; charset=binary");
        // URL says jpg, but a valid image header takes precedence.
        assert_eq!(
            image_content_type_from_response(&h, "https://example.com/a.jpg"),
            ContentType::from_mime("image/png")
        );
    }

    #[test]
    fn falls_back_to_url_extension_when_header_absent() {
        let h = HeaderMap::new();
        assert_eq!(
            image_content_type_from_response(&h, "https://example.com/cover.jpg?w=500"),
            ContentType::from_mime("image/jpeg")
        );
    }

    #[test]
    fn non_image_header_and_unknown_extension_is_octet_stream() {
        let h = headers_with("text/html");
        assert_eq!(
            image_content_type_from_response(&h, "https://example.com/broken"),
            ContentType::OctetStream
        );
    }
}
