//! Asking an artwork host for an image or a gallery document: the retries,
//! the answers that mean no image, and the checks on what comes back.

use super::RemoteImageError;
use bae_util::content_type::ContentType;
use bae_util::http::Http;
use bae_util::retry::{retry_classified, ClassifiedAttempt, Repeat, RetryPolicy};
use std::time::Duration;
use tracing::debug;

/// How an artwork host is asked again after a network error, a 5xx or a 429:
/// four tries, the waits doubling from one second, jittered.
const RETRY: RetryPolicy =
    RetryPolicy::exponential(4, Duration::from_secs(1), Duration::from_secs(8));

/// What one image request returned. A 404 is an ordinary answer because cover
/// addresses are derived without knowing whether their host has bytes there.
pub(super) enum ImageResponse {
    Body { bytes: Vec<u8>, content_type: ContentType },
    Nothing,
}

/// GET an image URL, repeating transient failures (network errors, 5xx) as
/// [`RETRY`] says.
pub(super) async fn send_image_request(
    http: &Http,
    image_url: &str,
    operation: &str,
) -> Result<ImageResponse, RemoteImageError> {
    match send_artwork_request(http, image_url, operation).await? {
        Some(response) => read_image_response(response).await,
        None => Ok(ImageResponse::Nothing),
    }
}

/// Shared transport policy for image bytes and artwork-list documents.
pub(super) async fn send_artwork_request(
    http: &Http,
    url: &str,
    operation: &str,
) -> Result<Option<reqwest::Response>, RemoteImageError> {
    retry_classified(http.clock().as_ref(), RETRY, operation, || async {
        let request = match http.get(url).timeout(bae_util::http::READ_TIMEOUT).build() {
            Ok(request) => request,
            Err(error) => {
                return ClassifiedAttempt::Failed(
                    RemoteImageError::Transport { context: "Failed to fetch image", error },
                    Repeat::Never,
                );
            }
        };
        let response = match http.execute(request).await {
            Ok(response) => response,
            Err(error) => {
                let repeat = if is_permanent_request_error(&error) {
                    Repeat::Never
                } else {
                    Repeat::AfterBackoff
                };
                return ClassifiedAttempt::Failed(
                    RemoteImageError::Transport { context: "Failed to fetch image", error },
                    repeat,
                );
            }
        };

        let status = response.status();
        if status.is_success() {
            return ClassifiedAttempt::Done(Some(response));
        }
        if status == reqwest::StatusCode::NOT_FOUND {
            return ClassifiedAttempt::Done(None);
        }
        let told_wait = bae_util::http::told_wait(response.headers(), http.clock().now());
        let repeat = Repeat::for_status(status, told_wait);
        ClassifiedAttempt::Failed(
            RemoteImageError::Status {
                status: status.as_u16(),
                detail: format!("Image download failed with status {status}"),
            },
            repeat,
        )
    })
    .await
}

/// Errors that fail the same way every attempt — a URL-parse / request-builder
/// failure, or a redirect loop bottoming out. Network, timeout, and connection
/// errors remain retryable.
fn is_permanent_request_error(error: &reqwest::Error) -> bool {
    bae_util::http::TransportFailure::of(error) == bae_util::http::TransportFailure::Unmakeable
}

pub(super) fn artwork_body_error(
    error: bae_util::http::HttpBodyError,
    context: &'static str,
) -> RemoteImageError {
    match error {
        bae_util::http::HttpBodyError::Read(error) => {
            RemoteImageError::Transport { context, error }
        }
        bae_util::http::HttpBodyError::TooLarge { .. } => {
            RemoteImageError::Unusable { detail: format!("{context}: {error}") }
        }
    }
}

/// Read bytes and content type from a successful image response.
async fn read_image_response(
    response: reqwest::Response,
) -> Result<ImageResponse, RemoteImageError> {
    let bytes = bae_util::http::read_body_capped(response, bae_util::http::MAX_IMAGE_BYTES)
        .await
        .map_err(|error| artwork_body_error(error, "Failed to read image response"))?;
    let (_, content_type) = bae_util::stored_image::decode_image(&bytes).map_err(|error| {
        RemoteImageError::Unusable {
            detail: format!("Downloaded file is not a valid image: {error}"),
        }
    })?;

    debug!("Downloaded cover art ({} bytes)", bytes.len());
    Ok(ImageResponse::Body { bytes, content_type })
}

#[cfg(test)]
#[path = "fetch_tests.rs"]
mod tests;
