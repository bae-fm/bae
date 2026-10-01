//! MusicBrainz's rate as the client reads it: refusals its rate limiter
//! sends, failures that are not, and the window under sustained load.

use super::tests::{mb_raw_server, mb_response_server, served_by};
use super::*;
use std::sync::atomic::Ordering;
use std::sync::Arc;

/// A 503 from MusicBrainz's rate limiter, naming the zone that refused.
fn rate_refusal(extra_headers: &str) -> String {
    format!(
        "HTTP/1.1 503 Service Unavailable\r\nx-ratelimit-zone: per-ip\r\n{extra_headers}\
         Content-Length: 0\r\n\r\n"
    )
}

fn release_group_ok(id: &str) -> String {
    let body = format!(r#"{{"id":"{id}"}}"#);
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
}

/// A 503 from MusicBrainz's rate limiter is its rate, not its answer: the
/// request is sent again, however many times that takes — more than the retry
/// policy would ever repeat a failure — and the lookup answers.
#[tokio::test]
async fn a_rate_refusal_is_sent_again_until_it_answers_never_failed() {
    let refusals = RETRY.attempts() as usize + 2;
    let mut script = vec![rate_refusal("retry-after: 0\r\n"); refusals];
    script.push(release_group_ok("rg-after-refusals"));
    let (url, requests, _) = mb_raw_server(script).await;

    let json = served_by(&url)
        .fetch_release_group_json("rg-after-refusals", CallPriority::Interactive)
        .await
        .expect("a rate refusal is waited out, not failed");

    assert_eq!(json, r#"{"id":"rg-after-refusals"}"#);
    assert_eq!(requests.load(Ordering::SeqCst), refusals + 1);
}

/// A 503 that names no rate-limit zone is the servers failing: retried, then
/// the lookup's failure.
#[tokio::test]
async fn a_503_without_a_zone_is_a_failure() {
    let attempts = RETRY.attempts() as usize;
    let (url, requests) = mb_response_server(vec![(503, String::new()); attempts]).await;

    let error = served_by(&url)
        .fetch_release_group_json("rg-down", CallPriority::Interactive)
        .await
        .expect_err("a failing server fails the lookup");

    assert!(matches!(
        error,
        MusicBrainzError::Provider {
            status: Some(503),
            ..
        }
    ));
    assert_eq!(requests.load(Ordering::SeqCst), attempts);
}

/// A rate refusal holds the whole limiter until the refusing window starts
/// over, so every other MusicBrainz request waits with the refused one.
#[tokio::test]
async fn a_rate_refusal_holds_every_musicbrainz_request_until_its_reset() {
    let reset = chrono::Utc::now().timestamp() + 30;
    let (url, requests, _) = mb_raw_server(vec![rate_refusal(&format!(
        "x-ratelimit-reset: {reset}\r\n"
    ))])
    .await;
    let musicbrainz = Arc::new(served_by(&url));
    let asking = Arc::clone(&musicbrainz);
    let asked = tokio::spawn(async move {
        asking
            .fetch_release_group_json("rg-held", CallPriority::Interactive)
            .await
    });

    let opens_at = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(at) = musicbrainz.limiter.opens_at() {
                return at;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("the refusal reaches the limiter");
    asked.abort();

    assert!(opens_at >= tokio::time::Instant::now() + Duration::from_secs(25));
    assert_eq!(requests.load(Ordering::SeqCst), 1);
}

/// MusicBrainz's count of one address, read strictly: a request arriving
/// within a second of the last one let through is turned away with a 503
/// naming the `per-ip` zone. Every response also counts the servers' shared
/// window, here with room to spare, and states the server's clock in `Date`.
struct MusicBrainzSecond {
    start: tokio::time::Instant,
    /// The server's clock at `start`, in seconds since the epoch.
    epoch_at_start: i64,
    last: Option<tokio::time::Instant>,
}

impl crate::util::rate_limiter::load_model::Window for MusicBrainzSecond {
    fn arrive(
        &mut self,
        at: tokio::time::Instant,
    ) -> (reqwest::StatusCode, reqwest::header::HeaderMap) {
        let seconds = self.epoch_at_start + (at - self.start).as_secs() as i64;
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::DATE,
            chrono::DateTime::from_timestamp(seconds, 0)
                .unwrap()
                .to_rfc2822()
                .parse()
                .unwrap(),
        );
        headers.insert("x-ratelimit-limit", 1900.into());
        headers.insert("x-ratelimit-remaining", 1500.into());
        headers.insert("x-ratelimit-reset", (seconds + 2 - seconds % 2).into());
        if self
            .last
            .is_some_and(|last| at - last < Duration::from_secs(1))
        {
            headers.insert(RATE_ZONE, "per-ip".parse().unwrap());
            headers.insert(reqwest::header::RETRY_AFTER, 0.into());
            return (reqwest::StatusCode::SERVICE_UNAVAILABLE, headers);
        }
        self.last = Some(at);
        (reqwest::StatusCode::OK, headers)
    }
}

/// Requests spaced as MusicBrainz's limiter spaces them, reaching it after
/// trips that differ by up to 190 ms, arrive a second apart or more — and the
/// servers' shared count does not slow them while it has room.
#[tokio::test(start_paused = true)]
async fn sustained_load_keeps_a_second_between_arrivals() {
    let musicbrainz = MusicBrainz::new(Http::for_test());
    let limiter = Arc::new(musicbrainz.limiter);
    let requests = 200;

    let outcome = crate::util::rate_limiter::load_model::sustain(
        limiter,
        MusicBrainzSecond {
            start: tokio::time::Instant::now(),
            epoch_at_start: 1_790_000_000,
            last: None,
        },
        mb_rate_answer,
        crate::util::rate_limiter::load_model::Load {
            requests,
            latency: Duration::from_millis(40)..Duration::from_millis(230),
            others: Vec::new(),
            seed: 27,
        },
    )
    .await;

    assert_eq!(outcome.refused, 0, "MusicBrainz turned requests away");
    assert!(
        outcome.elapsed <= REQUEST_INTERVAL * requests as u32 + Duration::from_secs(1),
        "took {:?}: the shared count slowed requests while it had room",
        outcome.elapsed
    );
}
