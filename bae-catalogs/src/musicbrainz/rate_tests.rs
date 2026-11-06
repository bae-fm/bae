//! MusicBrainz's rate as the client reads it: refusals its rate limiter
//! sends, failures that are not, and the window under sustained load.

use super::*;
use crate::musicbrainz::tests::{mb_raw_server, mb_response_server, served_by};
use crate::musicbrainz::*;
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

    assert!(matches!(error, MusicBrainzError::Provider { status: Some(503), .. }));
    assert_eq!(requests.load(Ordering::SeqCst), attempts);
}

/// A rate refusal holds the whole limiter until the refusing window starts
/// over, so every other MusicBrainz request waits with the refused one.
#[tokio::test]
async fn a_rate_refusal_holds_every_musicbrainz_request_until_its_reset() {
    // The reset is measured from the refusal's own `Date`, so the wait is
    // thirty seconds whatever this machine's clock says.
    const DATE: &str = "Sat, 26 Sep 2026 06:51:00 GMT";
    let reset =
        chrono::DateTime::parse_from_rfc2822(DATE).expect("the date parses").timestamp() + 30;
    let (url, requests, _) = mb_raw_server(vec![rate_refusal(&format!(
        "Date: {DATE}\r\nx-ratelimit-reset: {reset}\r\n"
    ))])
    .await;
    let musicbrainz = Arc::new(served_by(&url));
    let asking = Arc::clone(&musicbrainz);
    let asked_at = tokio::time::Instant::now();
    let asked = tokio::spawn(async move {
        asking.fetch_release_group_json("rg-held", CallPriority::Interactive).await
    });

    let opens_at =
        tokio::time::timeout(bae_util::test_channels::HANG_GUARD, musicbrainz.limiter.held_until())
            .await
            .expect("the refusal reaches the limiter");
    let seen_at = tokio::time::Instant::now();
    asked.abort();

    // The limiter held itself for thirty seconds when the refusal settled,
    // which was after the request was asked and before the hold was seen.
    assert!(opens_at >= asked_at + Duration::from_secs(30));
    assert!(opens_at <= seen_at + Duration::from_secs(30));
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

impl crate::rate_limiter::load_model::Window for MusicBrainzSecond {
    fn arrive(
        &mut self,
        at: tokio::time::Instant,
    ) -> (reqwest::StatusCode, reqwest::header::HeaderMap) {
        let seconds = self.epoch_at_start + (at - self.start).as_secs() as i64;
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::DATE,
            chrono::DateTime::from_timestamp(seconds, 0).unwrap().to_rfc2822().parse().unwrap(),
        );
        headers.insert("x-ratelimit-limit", 1900.into());
        headers.insert("x-ratelimit-remaining", 1500.into());
        headers.insert("x-ratelimit-reset", (seconds + 2 - seconds % 2).into());
        if self.last.is_some_and(|last| at - last < Duration::from_secs(1)) {
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

    let outcome = crate::rate_limiter::load_model::sustain(
        limiter,
        MusicBrainzSecond {
            start: tokio::time::Instant::now(),
            epoch_at_start: 1_790_000_000,
            last: None,
        },
        mb_rate_answer,
        crate::rate_limiter::load_model::Load {
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

/// Only a failure a retry could fix is retried. A `NotFound` is the ordinary
/// answer for a disc MusicBrainz doesn't have, and each extra attempt costs
/// another round trip plus a 1s rate-limit wait; the "at least one search field"
/// error is raised before a request is even built.
#[tokio::test]
async fn only_transient_musicbrainz_failures_are_retried() {
    let provider = |status: Option<u16>, told_wait: Option<Duration>| {
        repeat_mb(&MusicBrainzError::Provider { status, told_wait })
    };
    assert_eq!(repeat_mb(&MusicBrainzError::Timeout), Repeat::AfterBackoff);
    assert_eq!(
        repeat_mb(&MusicBrainzError::Network(bae_util::http::refused_request_error().await)),
        Repeat::AfterBackoff
    );
    assert_eq!(provider(Some(503), None), Repeat::AfterBackoff);
    assert_eq!(
        provider(Some(503), Some(Duration::from_secs(3))),
        Repeat::AfterToldWait(Duration::from_secs(3)),
        "a stated wait replaces the backoff"
    );
    assert_eq!(provider(Some(429), None), Repeat::AfterBackoff);

    assert_eq!(repeat_mb(&MusicBrainzError::NotFound("disc".into())), Repeat::Never);
    assert_eq!(provider(Some(404), None), Repeat::Never);
    assert_eq!(provider(Some(400), None), Repeat::Never);
    assert_eq!(
        repeat_mb(&MusicBrainzError::Other("At least one search field must be provided".into())),
        Repeat::Never
    );
    assert_eq!(
        repeat_mb(&MusicBrainzError::Request(bae_util::http::unbuildable_request_error())),
        Repeat::Never
    );
}
