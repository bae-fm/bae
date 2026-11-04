//! How the client keeps to MusicBrainz's rate: the pace requests leave at,
//! what a response says of the rate, and which failures are asked again.

use super::MusicBrainzError;
use crate::rate_limiter::{RateAnswer, WindowCount};
use bae_util::retry::{Repeat, RetryPolicy};
use reqwest::header::HeaderMap;
use reqwest::StatusCode;
use std::time::Duration;

/// A request every 1.2 seconds: a sixth under the one a second MusicBrainz
/// allows an address (<https://musicbrainz.org/doc/MusicBrainz_API/Rate_Limiting>).
/// Requests are spaced as they leave but counted as they arrive, and two can
/// arrive closer than they left when the network takes one longer than the
/// other; at this pace they still arrive a second apart unless one trip runs
/// 200 ms longer than the next.
pub(super) const REQUEST_INTERVAL: Duration = Duration::from_millis(1200);

/// The span MusicBrainz's rate headers count over: two seconds, after which
/// the count starts over (MetaBrainz's answer on what the headers mean,
/// <https://community.metabrainz.org/t/what-do-the-x-ratelimit-headers-mean-and-how-should-they-be-used/259737>).
/// A refusal that names no reset is held this long.
pub(super) const RATE_WINDOW: Duration = Duration::from_secs(2);

/// Retry only what a retry can fix. `NotFound` is MusicBrainz's answer, not a
/// fault — and it's the ordinary answer for a disc it doesn't have, so retrying
/// buys a round trip and a rate-limit wait per try to learn it again. `Request`
/// and `Other` are local (a request reqwest would not build, a JSON parse, a
/// missing search field): either no request was made, or the same bytes will
/// parse the same way.
///
/// A transient status is repeated after the wait the response asked for, when
/// it asked for one.
pub(super) fn repeat_mb(error: &MusicBrainzError) -> Repeat {
    match error {
        MusicBrainzError::Network(_) | MusicBrainzError::Timeout => Repeat::AfterBackoff,
        MusicBrainzError::Provider { status, told_wait } => {
            Repeat::for_recorded_status(*status, *told_wait)
        }
        MusicBrainzError::NotFound(_)
        | MusicBrainzError::Request(_)
        | MusicBrainzError::Other(_) => Repeat::Never,
    }
}

/// How MusicBrainz is asked again after a server error or a lost connection.
/// A 503 its rate limiter sends is not repeated here — the limiter holds every
/// request until the refusing window starts over (see [`MusicBrainz::send`]);
/// a 503 without a rate-limit zone is the servers failing. The waits double
/// from one second, giving a failing server some fifteen seconds in all to
/// recover, and each is jittered, since a server failing everyone at once
/// hears every client's repeats together otherwise. A response that does
/// state a wait in `Retry-After` is waited out as it asks instead (see
/// [`Repeat`]).
pub(super) const RETRY: RetryPolicy =
    RetryPolicy::exponential(5, Duration::from_secs(1), Duration::from_secs(10));

/// Wrap one request in the client's own retry policy — a caller shouldn't have to
/// know which of these failures are worth repeating. (Discogs does the same.)
pub(super) async fn mb_retry<F, Fut, T>(label: &str, f: F) -> Result<T, MusicBrainzError>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, MusicBrainzError>>,
{
    bae_util::retry::retry_with_backoff_if(RETRY, label, repeat_mb, f).await
}

/// The header MusicBrainz's rate limiter names the refusing zone in: `per-ip`
/// for this address over its rate, `global` for the servers over theirs. Only
/// a refusal carries it.
pub(super) const RATE_ZONE: &str = "x-ratelimit-zone";

/// What a MusicBrainz response says of its rate.
///
/// Its rate limiter turns a request away with a 503 naming the zone that
/// refused it in `X-RateLimit-Zone`, when that zone's window starts over in
/// `X-RateLimit-Reset` (whole seconds since the epoch), and a `Retry-After`;
/// the request is held until the later of the two. A 503 without a zone is
/// the servers failing, an error like any other.
///
/// Every other response counts the servers' shared window:
/// `X-RateLimit-Remaining` of the `X-RateLimit-Limit` requests (some
/// thousands) every client together may make in a two-second window that
/// starts over at `X-RateLimit-Reset`. It is everyone's count, not this
/// address's — answers a moment apart report it hundreds apart — so it says
/// when the servers are about to turn every client away, and this address's
/// own rate is the interval's to keep.
pub(super) fn mb_rate_answer(status: StatusCode, headers: &HeaderMap) -> RateAnswer {
    let resets_in = bae_util::http::wait_until_epoch(headers, "x-ratelimit-reset");
    if status == StatusCode::SERVICE_UNAVAILABLE && headers.contains_key(RATE_ZONE) {
        return RateAnswer::Refused {
            wait: [bae_util::http::told_wait(headers), resets_in].into_iter().flatten().max(),
        };
    }
    let remaining = headers
        .get("x-ratelimit-remaining")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse().ok());
    match (remaining, resets_in) {
        (Some(remaining), Some(resets_in)) => {
            RateAnswer::Counted(WindowCount::Fixed { remaining, resets_in })
        }
        _ => RateAnswer::Silent,
    }
}

#[cfg(test)]
#[path = "rate_tests.rs"]
mod tests;
