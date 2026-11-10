//! How a remote-image fetch that failed reads as a lookup failure: what the
//! artwork host answered with is the host's failure, and what bae met on its
//! own side is bae's.

#![expect(
    clippy::disallowed_methods,
    reason = "test code: what it spawns ends with the test, which has no crash reporter to report to"
)]

use super::*;

fn transport(error: reqwest::Error) -> RemoteImageError {
    RemoteImageError::Transport { context: "Failed to fetch image", error }
}

#[test]
fn a_status_the_host_answered_with_is_its_failure() {
    for status in [400, 401, 404, 429, 500] {
        let error = RemoteImageError::Status {
            status,
            detail: format!("Image download failed with status {status}"),
        };
        assert_eq!(
            remote_image_failure(&error),
            Some(LookupFailure::Provider { status: Some(status) })
        );
    }
}

#[tokio::test]
async fn a_request_that_reached_no_host_is_a_network_failure() {
    let error = reqwest::Client::new()
        .get("http://127.0.0.1:1/cover.jpg")
        .send()
        .await
        .expect_err("nothing listens on port 1");
    let error = transport(error);
    assert_eq!(remote_image_failure(&error), Some(LookupFailure::Network));
}

#[tokio::test]
async fn a_request_that_ran_out_of_time_is_a_timeout() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("test listener binds");
    let address = listener.local_addr().expect("test listener address");
    tokio::spawn(async move {
        let (_stream, _) = listener.accept().await.expect("test request connects");
        std::future::pending::<()>().await;
    });
    let error = reqwest::Client::new()
        .get(format!("http://{address}/cover.jpg"))
        .timeout(std::time::Duration::from_millis(30))
        .send()
        .await
        .expect_err("nothing answers in time");
    let error = transport(error);
    assert_eq!(remote_image_failure(&error), Some(LookupFailure::Timeout));
}

#[test]
fn a_request_bae_could_not_build_is_its_own_failure() {
    let error =
        reqwest::Client::new().get("not a URL").build().expect_err("a relative URL does not build");
    let error = transport(error);
    assert_eq!(remote_image_failure(&error), None);
}

/// No image where the caller needed one is the host's answer, not its
/// failure.
#[test]
fn no_image_at_an_address_is_no_failure_of_its_host() {
    let error = RemoteImageError::NoImage { url: "https://example.com/front".to_string() };
    assert_eq!(remote_image_failure(&error), None);
}

#[test]
fn bytes_that_are_not_an_image_are_unusable_data() {
    let error =
        RemoteImageError::Unusable { detail: "Downloaded file is not a valid image".to_string() };
    assert_eq!(remote_image_failure(&error), None);
}

#[test]
fn a_cache_task_that_did_not_finish_is_bae_s_failure() {
    let error = RemoteImageError::CacheTask(bae_util::task_failure::TaskFailed::Panicked(
        bae_util::task_failure::Bug {
            operation: "read the remote image cache",
            message: "the cache task fails".to_string(),
        },
    ));
    assert_eq!(remote_image_failure(&error), None);
}
