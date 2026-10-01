//! Sustained load against a fake of a provider's window, for tests: many
//! waiters at once, each request reaching the provider after its own latency
//! and its answer coming back after another, the way a real one's do. A
//! provider's tests supply the fake, which counts as that provider does, and
//! the client's own reading of its headers.

use std::ops::Range;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rand::{Rng, SeedableRng};
use reqwest::header::HeaderMap;
use reqwest::StatusCode;
use tokio::time::Instant;

use super::{CallPriority, RateAnswer, RateLimiter};

/// A provider's count of requests from one address.
pub(crate) trait Window: Send + 'static {
    /// A request arriving `at`: the status and headers the provider answers.
    fn arrive(&mut self, at: Instant) -> (StatusCode, HeaderMap);
}

pub(crate) struct Load {
    /// Requests, all waiting from the start.
    pub requests: usize,
    /// How long a request takes to reach the provider, and its answer to come
    /// back — each drawn apart, per request.
    pub latency: Range<Duration>,
    /// When another client on the same address asks, from the start.
    pub others: Vec<Duration>,
    pub seed: u64,
}

pub(crate) struct Outcome {
    /// Times the provider turned one of these requests away for its rate.
    pub refused: usize,
    /// From the start until the last request was answered.
    pub elapsed: Duration,
}

/// Every request of `load` made through `limiter` against `window`, each
/// answer read by `read` and handed back to the limiter, a refused one asked
/// again until it is answered.
pub(crate) async fn sustain(
    limiter: Arc<RateLimiter>,
    window: impl Window,
    read: fn(StatusCode, &HeaderMap) -> RateAnswer,
    load: Load,
) -> Outcome {
    let start = Instant::now();
    let window = Arc::new(Mutex::new(window));
    let refused = Arc::new(Mutex::new(0usize));
    let mut rng = rand::rngs::StdRng::seed_from_u64(load.seed);

    let others = {
        let window = Arc::clone(&window);
        tokio::spawn(async move {
            for at in load.others {
                tokio::time::sleep_until(start + at).await;
                window.lock().unwrap().arrive(Instant::now());
            }
        })
    };

    let mut requests = Vec::new();
    for index in 0..load.requests {
        let priority = if index % 3 == 0 {
            CallPriority::Interactive
        } else {
            CallPriority::Background
        };
        let mut latencies: Vec<(Duration, Duration)> = (0..64)
            .map(|_| {
                (
                    rng.random_range(load.latency.clone()),
                    rng.random_range(load.latency.clone()),
                )
            })
            .collect();
        let limiter = Arc::clone(&limiter);
        let window = Arc::clone(&window);
        let refused = Arc::clone(&refused);
        requests.push(tokio::spawn(async move {
            loop {
                let (there, back) = latencies.pop().expect("a request is refused fewer times");
                let admitted = limiter.wait(priority).await;
                tokio::time::sleep(there).await;
                let (status, headers) = window.lock().unwrap().arrive(Instant::now());
                tokio::time::sleep(back).await;
                let answer = read(status, &headers);
                admitted.answered(answer);
                if !matches!(answer, RateAnswer::Refused { .. }) {
                    return;
                }
                *refused.lock().unwrap() += 1;
            }
        }));
    }
    for request in requests {
        request.await.expect("a request task finishes");
    }
    let elapsed = start.elapsed();
    others.abort();

    let refused = *refused.lock().unwrap();
    Outcome { refused, elapsed }
}
