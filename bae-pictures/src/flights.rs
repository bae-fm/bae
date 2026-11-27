//! One running job for each picture and size. Requests for the same key share
//! the job; it runs while some request still waits for it, at the highest turn
//! any of them asks, and stops, between two reads of the original, when the
//! last one goes away.

use std::collections::HashMap;
use std::future::Future;
use std::hash::Hash;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use bae_diagnostics::Diagnostics;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use crate::{AskedTurn, IsCancelled, PictureTurn};

/// How a job ended for the requests that shared it.
#[derive(Debug)]
pub enum FlightError<E> {
    /// The job returned this error.
    Failed(Arc<E>),
    /// The job panicked. The panic hook reported it.
    Panicked,
}

type Outcome<T, E> = Option<Result<Arc<T>, FlightError<E>>>;

/// The jobs running now, by key.
type Running<K, T, E> = Arc<Mutex<HashMap<K, Arc<Flight<T, E>>>>>;

/// One job and the requests waiting on it.
struct Flight<T, E> {
    cancel: CancellationToken,
    outcome: watch::Sender<Outcome<T, E>>,
    asks: Mutex<Asks>,
    /// The highest turn any waiting request asks, which the job reads.
    turn: watch::Sender<PictureTurn>,
}

/// The requests still waiting on a job.
#[derive(Default)]
struct Asks {
    /// The turn each of them asks now, by the request's number.
    turns: HashMap<u64, PictureTurn>,
    /// The number the next request gets.
    next: u64,
}

impl Asks {
    /// Adds a request that asks `turn`, and returns its number.
    fn add(&mut self, turn: PictureTurn) -> u64 {
        let number = self.next;
        self.next += 1;
        self.turns.insert(number, turn);
        number
    }
}

impl<T, E> Flight<T, E> {
    fn lock_asks(&self) -> MutexGuard<'_, Asks> {
        self.asks.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Sets the job's turn to the highest one `asks` holds. A job no request
    /// waits on keeps the turn it had. The caller holds the lock of `asks`, so
    /// that two changes reach the job in the order they were made.
    fn publish_turn(&self, asks: &Asks) {
        if let Some(&highest) = asks.turns.values().max() {
            self.turn.send_if_modified(|turn| std::mem::replace(turn, highest) != highest);
        }
    }

    fn join(&self, turn: PictureTurn) -> u64 {
        let mut asks = self.lock_asks();
        let number = asks.add(turn);
        self.publish_turn(&asks);
        number
    }

    /// The request `number` asks `turn` now.
    fn ask(&self, number: u64, turn: PictureTurn) {
        let mut asks = self.lock_asks();
        asks.turns.insert(number, turn);
        self.publish_turn(&asks);
    }
}

/// The jobs running now, by key.
pub struct Flights<K, T, E> {
    running: Running<K, T, E>,
    diagnostics: Diagnostics,
    runtime: tokio::runtime::Handle,
}

impl<K, T, E> Clone for Flights<K, T, E> {
    fn clone(&self) -> Self {
        Self {
            running: self.running.clone(),
            diagnostics: self.diagnostics.clone(),
            runtime: self.runtime.clone(),
        }
    }
}

impl<K, T, E> Flights<K, T, E>
where
    K: Hash + Eq + Clone + Send + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    pub fn new(diagnostics: Diagnostics, runtime: tokio::runtime::Handle) -> Self {
        Self { running: Arc::default(), diagnostics, runtime }
    }

    /// The result of the job for `key`: the running one when there is one,
    /// else the one `start` builds. `turn` says how soon this request wants
    /// the result, and may change while it waits. `start` gets the flag that
    /// ends the job when no request wants it any more (a question the job asks
    /// between two reads) and the job's turn, the highest any waiting request
    /// asks. The job runs on the runtime, not in this future, so a request
    /// that goes away does not take the job with it while another still waits.
    pub async fn request<F, Fut>(
        &self,
        key: K,
        mut turn: AskedTurn,
        start: F,
    ) -> Result<Arc<T>, FlightError<E>>
    where
        F: FnOnce(IsCancelled, AskedTurn) -> Fut,
        Fut: Future<Output = Result<T, E>> + Send + 'static,
    {
        let waiter = self.join(key, turn.now(), start);
        let mut outcome = waiter.outcome.clone();
        let done = async move {
            match outcome.wait_for(Option::is_some).await {
                Ok(done) => match done.as_ref() {
                    Some(Ok(value)) => Ok(value.clone()),
                    Some(Err(FlightError::Failed(error))) => {
                        Err(FlightError::Failed(error.clone()))
                    }
                    Some(Err(FlightError::Panicked)) | None => Err(FlightError::Panicked),
                },
                // The job's task ended without a result: its runtime shut down.
                Err(_) => Err(FlightError::Panicked),
            }
        };
        tokio::pin!(done);
        loop {
            tokio::select! {
                biased;
                () = turn.changed() => waiter.flight.ask(waiter.number, turn.now()),
                result = &mut done => return result,
            }
        }
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<K, Arc<Flight<T, E>>>> {
        self.running.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn join<F, Fut>(&self, key: K, turn: PictureTurn, start: F) -> Waiter<K, T, E>
    where
        F: FnOnce(IsCancelled, AskedTurn) -> Fut,
        Fut: Future<Output = Result<T, E>> + Send + 'static,
    {
        let mut running = self.lock();
        if let Some(flight) = running.get(&key) {
            let number = flight.join(turn);
            return Waiter {
                number,
                running: self.running.clone(),
                outcome: flight.outcome.subscribe(),
                flight: flight.clone(),
                key,
            };
        }
        let (outcome, receiver) = watch::channel(None);
        let mut asks = Asks::default();
        let number = asks.add(turn);
        let (job_turn, job_asked) = watch::channel(turn);
        let flight = Arc::new(Flight {
            cancel: CancellationToken::new(),
            outcome,
            asks: Mutex::new(asks),
            turn: job_turn,
        });
        running.insert(key.clone(), flight.clone());
        let token = flight.cancel.clone();
        let job = start(Arc::new(move || token.is_cancelled()), AskedTurn::from(job_asked));
        let ended = flight.clone();
        let map = self.running.clone();
        let ended_key = key.clone();
        // The task is not aborted when its last request goes: its reads see
        // the flag, and it ends with whatever they return.
        drop(self.diagnostics.spawn_ending(&self.runtime, "a picture job", job, move |result| {
            let result = match result {
                Ok(Ok(value)) => Ok(Arc::new(value)),
                Ok(Err(error)) => Err(FlightError::Failed(Arc::new(error))),
                Err(_) => Err(FlightError::Panicked),
            };
            let mut running = map.lock().unwrap_or_else(PoisonError::into_inner);
            if running.get(&ended_key).is_some_and(|flight| Arc::ptr_eq(flight, &ended)) {
                running.remove(&ended_key);
            }
            drop(running);
            // No request may be left to hear it.
            ended.outcome.send_replace(Some(result));
        }));
        Waiter { running: self.running.clone(), outcome: receiver, flight, key, number }
    }
}

/// One request's place among those waiting on a job. The last one to go ends
/// the job, unless it has ended.
struct Waiter<K, T, E>
where
    K: Hash + Eq + Clone + Send + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    /// The jobs running now, where this request ends its job when it is the
    /// last to leave.
    running: Running<K, T, E>,
    key: K,
    flight: Arc<Flight<T, E>>,
    /// The request's number among the job's.
    number: u64,
    /// What the request waits on for the job's result.
    outcome: watch::Receiver<Outcome<T, E>>,
}

impl<K, T, E> Drop for Waiter<K, T, E>
where
    K: Hash + Eq + Clone + Send + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    fn drop(&mut self) {
        let mut running = self.running.lock().unwrap_or_else(PoisonError::into_inner);
        let mut asks = self.flight.lock_asks();
        asks.turns.remove(&self.number);
        self.flight.publish_turn(&asks);
        if asks.turns.is_empty() && self.flight.outcome.borrow().is_none() {
            self.flight.cancel.cancel();
            if running.get(&self.key).is_some_and(|flight| Arc::ptr_eq(flight, &self.flight)) {
                running.remove(&self.key);
            }
        }
    }
}
