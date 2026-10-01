use super::*;
use std::sync::{Arc, Mutex};
use tokio::task::yield_now;

const INTERVAL: Duration = Duration::from_secs(1);

fn queued(limiter: &RateLimiter, priority: CallPriority) -> usize {
    limiter.queued_count(priority)
}

/// Spawned waiters reach the queue only when they run, so a test that cares
/// about arrival order has to let them get there first. Yielding keeps a
/// task runnable, so the paused clock does not advance while we wait.
async fn queued_reaches(limiter: &RateLimiter, priority: CallPriority, count: usize) {
    while queued(limiter, priority) < count {
        yield_now().await;
    }
}

fn spawn_wait(limiter: &Arc<RateLimiter>, priority: CallPriority) -> tokio::task::JoinHandle<()> {
    let limiter = Arc::clone(limiter);
    tokio::spawn(async move { drop(limiter.wait(priority).await) })
}

#[tokio::test(start_paused = true)]
async fn wait_spaces_calls_by_the_interval() {
    let limiter = RateLimiter::new(INTERVAL);

    // First call returns immediately — no previous stamp.
    let start = Instant::now();
    drop(limiter.wait(CallPriority::Interactive).await);
    assert!(start.elapsed() < Duration::from_millis(100));

    // Second call waits out the interval since the first.
    let start = Instant::now();
    drop(limiter.wait(CallPriority::Interactive).await);
    assert!(start.elapsed() >= Duration::from_millis(900));
}

/// The one that fails if the priority is removed: with a single FIFO the
/// interactive call is admitted 21 intervals in.
#[tokio::test(start_paused = true)]
async fn interactive_overtakes_a_queued_background_flood() {
    let limiter = Arc::new(RateLimiter::new(INTERVAL));
    // Spend the free first slot, so everything below has to queue.
    drop(limiter.wait(CallPriority::Background).await);

    for _ in 0..20 {
        spawn_wait(&limiter, CallPriority::Background);
    }
    queued_reaches(&limiter, CallPriority::Background, 20).await;

    let start = Instant::now();
    drop(limiter.wait(CallPriority::Interactive).await);
    assert!(
        start.elapsed() <= INTERVAL,
        "interactive call waited {:?}, behind the background queue",
        start.elapsed()
    );
}

/// The guarantee that forbids giving background work its own limiter.
#[tokio::test(start_paused = true)]
async fn the_interval_bounds_both_classes_together() {
    let limiter = Arc::new(RateLimiter::new(INTERVAL));
    let admissions: Arc<Mutex<Vec<Instant>>> = Arc::default();

    let mut handles = Vec::new();
    for i in 0..10 {
        let priority = if i % 2 == 0 {
            CallPriority::Interactive
        } else {
            CallPriority::Background
        };
        let limiter = Arc::clone(&limiter);
        let admissions = Arc::clone(&admissions);
        handles.push(tokio::spawn(async move {
            drop(limiter.wait(priority).await);
            admissions.lock().unwrap().push(Instant::now());
        }));
    }
    for handle in handles {
        handle.await.unwrap();
    }

    let mut times = admissions.lock().unwrap().clone();
    times.sort();
    assert_eq!(times.len(), 10);
    for pair in times.windows(2) {
        assert!(
            pair[1] - pair[0] >= INTERVAL,
            "admissions {:?} apart, closer than the interval",
            pair[1] - pair[0]
        );
    }
}

#[tokio::test(start_paused = true)]
async fn background_drains_in_arrival_order_once_interactive_is_idle() {
    let limiter = Arc::new(RateLimiter::new(INTERVAL));
    // Spend the free first slot, so every waiter below is queued.
    drop(limiter.wait(CallPriority::Interactive).await);

    let order: Arc<Mutex<Vec<usize>>> = Arc::default();
    let mut handles = Vec::new();
    for i in 0..5 {
        let limiter_task = Arc::clone(&limiter);
        let order = Arc::clone(&order);
        handles.push(tokio::spawn(async move {
            drop(limiter_task.wait(CallPriority::Background).await);
            order.lock().unwrap().push(i);
        }));
        queued_reaches(&limiter, CallPriority::Background, i + 1).await;
    }
    for handle in handles {
        handle.await.unwrap();
    }

    assert_eq!(*order.lock().unwrap(), vec![0, 1, 2, 3, 4]);
}

/// The waiter behind a cancelled one keeps the schedule it would have had if
/// the cancelled call had never been made: it is admitted one interval after
/// the last *real* call, not one interval after the cancellation, and it is
/// admitted at all rather than stuck behind an abandoned ticket.
#[tokio::test(start_paused = true)]
async fn a_cancelled_waiter_costs_no_slot() {
    let limiter = Arc::new(RateLimiter::new(INTERVAL));
    // Spend the free first slot, so every waiter below is queued.
    drop(limiter.wait(CallPriority::Interactive).await);

    let cancelled = spawn_wait(&limiter, CallPriority::Interactive);
    queued_reaches(&limiter, CallPriority::Interactive, 1).await;
    // Cancel partway into the interval, so a slot wrongly spent here would
    // push the next admission out by the part already served.
    tokio::time::sleep(INTERVAL / 2).await;
    cancelled.abort();
    // Joining an aborted task means its future — and the ticket it holds —
    // is really dropped.
    assert!(cancelled.await.unwrap_err().is_cancelled());

    let start = Instant::now();
    drop(
        tokio::time::timeout(INTERVAL * 5, limiter.wait(CallPriority::Background))
            .await
            .expect("the cancelled waiter left its ticket in the queue"),
    );
    assert!(
        start.elapsed() <= INTERVAL / 2,
        "waited {:?}: the cancelled waiter ate a slot",
        start.elapsed()
    );
}

#[tokio::test(start_paused = true)]
async fn an_uncontended_wait_neither_sleeps_nor_spawns_a_task() {
    let limiter = RateLimiter::new(INTERVAL);
    let alive_before = tokio::runtime::Handle::current()
        .metrics()
        .num_alive_tasks();

    let start = Instant::now();
    drop(limiter.wait(CallPriority::Interactive).await);

    assert_eq!(start.elapsed(), Duration::ZERO);
    assert_eq!(
        tokio::runtime::Handle::current()
            .metrics()
            .num_alive_tasks(),
        alive_before
    );
    assert!(!limiter.has_waiters_for_test());
}

/// A provider's limiter outlives any one runtime that asks through it —
/// the import worker builds and drops its own while the app's keeps going.
/// A waiter dying with its runtime must leave nothing behind that a later
/// runtime waits on, which is why admission cannot belong to a spawned
/// task.
#[test]
fn a_dropped_runtime_leaves_the_limiter_usable() {
    let limiter = Arc::new(RateLimiter::new(INTERVAL));

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .start_paused(true)
            .build()
            .unwrap()
    }

    let first = runtime();
    first.block_on(async {
        // Spend the free slot, then leave a waiter queued behind it.
        drop(limiter.wait(CallPriority::Interactive).await);
        spawn_wait(&limiter, CallPriority::Background);
        queued_reaches(&limiter, CallPriority::Background, 1).await;
    });
    drop(first);

    let second = runtime();
    second.block_on(async {
        let start = Instant::now();
        drop(
            tokio::time::timeout(INTERVAL * 5, limiter.wait(CallPriority::Interactive))
                .await
                .expect("the dropped runtime wedged the limiter"),
        );
        assert!(
            start.elapsed() <= INTERVAL,
            "waited {:?} behind a waiter that no longer exists",
            start.elapsed()
        );
    });
}

// ── What a provider says about its rate ─────────────────────────────────────

const WINDOW: Duration = Duration::from_secs(60);

fn spawn_stamped(
    limiter: &Arc<RateLimiter>,
    priority: CallPriority,
    admissions: &Arc<Mutex<Vec<Instant>>>,
) -> tokio::task::JoinHandle<()> {
    let limiter = Arc::clone(limiter);
    let admissions = Arc::clone(admissions);
    tokio::spawn(async move {
        drop(limiter.wait(priority).await);
        admissions.lock().unwrap().push(Instant::now());
    })
}

/// One request turned away for the rate holds every waiter — interactive and
/// background, queued before the refusal came back or after it — until the
/// wait it asked for passes, rather than only the refused request waiting
/// while the rest keep sending.
#[tokio::test(start_paused = true)]
async fn a_refusal_holds_every_waiter_until_its_wait_passes() {
    let limiter = Arc::new(RateLimiter::counted(INTERVAL, WINDOW));
    let admissions: Arc<Mutex<Vec<Instant>>> = Arc::default();
    let refused = limiter.wait(CallPriority::Interactive).await;

    let mut handles = Vec::new();
    for priority in [
        CallPriority::Interactive,
        CallPriority::Background,
        CallPriority::Background,
    ] {
        handles.push(spawn_stamped(&limiter, priority, &admissions));
    }
    queued_reaches(&limiter, CallPriority::Interactive, 1).await;
    queued_reaches(&limiter, CallPriority::Background, 2).await;

    let start = Instant::now();
    let told = Duration::from_secs(30);
    refused.answered(RateAnswer::Refused { wait: Some(told) });
    handles.push(spawn_stamped(
        &limiter,
        CallPriority::Interactive,
        &admissions,
    ));
    for handle in handles {
        handle.await.unwrap();
    }

    let mut times = admissions.lock().unwrap().clone();
    times.sort();
    assert_eq!(times.len(), 4);
    assert!(
        times[0] >= start + told,
        "admitted {:?} into a {told:?} hold",
        times[0] - start
    );
    for pair in times.windows(2) {
        assert!(
            pair[1] - pair[0] >= INTERVAL,
            "the hold let a burst through"
        );
    }
}

/// A refusal that names no wait holds until the window it counts over has
/// passed: every request the provider counted has left it by then.
#[tokio::test(start_paused = true)]
async fn a_refusal_without_a_wait_holds_one_window() {
    let limiter = RateLimiter::counted(INTERVAL, WINDOW);
    let start = Instant::now();
    limiter
        .wait(CallPriority::Background)
        .await
        .answered(RateAnswer::Refused { wait: None });

    drop(limiter.wait(CallPriority::Interactive).await);
    assert_eq!(start.elapsed(), WINDOW);
}

/// A fixed window nearly spent holds admission until it starts over; one
/// with room leaves only the interval.
#[tokio::test(start_paused = true)]
async fn a_spent_fixed_window_holds_until_it_resets() {
    let resets_in = Duration::from_millis(1500);
    for (remaining, expected) in [(3, resets_in), (10, INTERVAL)] {
        let limiter = RateLimiter::counted(INTERVAL, Duration::from_secs(2));
        let start = Instant::now();
        limiter
            .wait(CallPriority::Background)
            .await
            .answered(RateAnswer::Counted(WindowCount::Fixed {
                remaining,
                resets_in,
            }));

        drop(limiter.wait(CallPriority::Background).await);
        assert_eq!(start.elapsed(), expected, "{remaining} remaining");
    }
}

/// In a moving window, a count nearly spent waits for the first of this
/// limiter's own counted requests to leave it — a window after its answer
/// came back — rather than for the whole count to end, since the requests
/// the count holds that are not its own may stay that long.
#[tokio::test(start_paused = true)]
async fn a_spent_rolling_window_waits_for_its_own_request_to_leave() {
    let limiter = RateLimiter::counted(INTERVAL, WINDOW);
    let start = Instant::now();
    let answer_after = Duration::from_millis(100);

    let first = limiter.wait(CallPriority::Background).await;
    tokio::time::sleep(answer_after).await;
    first.answered(RateAnswer::Silent);

    let counted = limiter.wait(CallPriority::Background).await;
    tokio::time::sleep(answer_after).await;
    counted.answered(RateAnswer::Counted(WindowCount::Rolling { remaining: 4 }));

    // One more fits above the reserve; after it the count is spent.
    let last = limiter.wait(CallPriority::Background).await;
    assert_eq!(start.elapsed(), INTERVAL * 2);
    tokio::time::sleep(answer_after).await;
    last.answered(RateAnswer::Silent);

    drop(limiter.wait(CallPriority::Background).await);
    assert_eq!(start.elapsed(), answer_after + WINDOW);
}

/// A count read off an earlier request's late answer says less than the one
/// already taken from a later request, and does not replace it.
#[tokio::test(start_paused = true)]
async fn a_late_count_does_not_replace_a_newer_one() {
    let limiter = RateLimiter::counted(Duration::ZERO, WINDOW);
    let earlier = limiter.wait(CallPriority::Background).await;
    let later = limiter.wait(CallPriority::Background).await;

    later.answered(RateAnswer::Counted(WindowCount::Rolling { remaining: 50 }));
    earlier.answered(RateAnswer::Counted(WindowCount::Rolling { remaining: 0 }));

    assert_eq!(limiter.opens_at(), None);
}

/// The limiter keeps only the requests a current count could still hold, so
/// a long sweep does not grow it.
#[tokio::test(start_paused = true)]
async fn requests_no_count_can_hold_are_let_go() {
    let limiter = RateLimiter::counted(INTERVAL, WINDOW);
    for _ in 0..500 {
        limiter
            .wait(CallPriority::Background)
            .await
            .answered(RateAnswer::Counted(WindowCount::Rolling { remaining: 50 }));
    }
    let held_at_most = (WINDOW * 2).as_secs() as usize / INTERVAL.as_secs() as usize + 1;
    assert!(
        limiter.sent_in_ledger() <= held_at_most,
        "{} requests kept",
        limiter.sent_in_ledger()
    );
}
