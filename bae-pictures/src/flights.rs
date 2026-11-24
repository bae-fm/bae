//! One running job for each picture and size. Requests for the same key share
//! the job; it runs while some request still waits for it, and stops, between
//! two reads of the original, when the last one goes away.

use std::collections::HashMap;
use std::future::Future;
use std::hash::Hash;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use bae_diagnostics::Diagnostics;
use tokio::sync::watch;

use crate::{CancelFlag, IsCancelled};

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
    cancel: CancelFlag,
    outcome: watch::Sender<Outcome<T, E>>,
    /// The requests still waiting. Guarded by the map's lock.
    waiters: Mutex<usize>,
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
    /// else the one `start` builds. `start` gets the flag that ends the job
    /// when no request wants it any more (a question the job asks between two
    /// reads), and runs on the runtime, not in
    /// this future, so a request that goes away does not take the job with
    /// it while another still waits.
    pub async fn request<F, Fut>(&self, key: K, start: F) -> Result<Arc<T>, FlightError<E>>
    where
        F: FnOnce(IsCancelled) -> Fut,
        Fut: Future<Output = Result<T, E>> + Send + 'static,
    {
        let mut waiter = self.join(key, start);
        let result = waiter.outcome.wait_for(Option::is_some).await;
        match result {
            Ok(done) => match done.as_ref() {
                Some(Ok(value)) => Ok(value.clone()),
                Some(Err(FlightError::Failed(error))) => Err(FlightError::Failed(error.clone())),
                Some(Err(FlightError::Panicked)) | None => Err(FlightError::Panicked),
            },
            // The job's task ended without a result: its runtime shut down.
            Err(_) => Err(FlightError::Panicked),
        }
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<K, Arc<Flight<T, E>>>> {
        self.running.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn join<F, Fut>(&self, key: K, start: F) -> Waiter<'_, K, T, E>
    where
        F: FnOnce(IsCancelled) -> Fut,
        Fut: Future<Output = Result<T, E>> + Send + 'static,
    {
        let mut running = self.lock();
        if let Some(flight) = running.get(&key) {
            *flight.waiters.lock().unwrap_or_else(PoisonError::into_inner) += 1;
            return Waiter {
                flights: self,
                outcome: flight.outcome.subscribe(),
                flight: flight.clone(),
                key,
            };
        }
        let (outcome, receiver) = watch::channel(None);
        let flight =
            Arc::new(Flight { cancel: CancelFlag::new(), outcome, waiters: Mutex::new(1) });
        running.insert(key.clone(), flight.clone());
        let job = start(flight.cancel.watcher());
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
        Waiter { flights: self, outcome: receiver, flight, key }
    }
}

/// One request's place among those waiting on a job. The last one to go ends
/// the job, unless it has ended.
struct Waiter<'a, K, T, E>
where
    K: Hash + Eq + Clone + Send + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    flights: &'a Flights<K, T, E>,
    key: K,
    flight: Arc<Flight<T, E>>,
    /// What the request waits on for the job's result.
    outcome: watch::Receiver<Outcome<T, E>>,
}

impl<K, T, E> Drop for Waiter<'_, K, T, E>
where
    K: Hash + Eq + Clone + Send + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    fn drop(&mut self) {
        let mut running = self.flights.lock();
        let mut waiters = self.flight.waiters.lock().unwrap_or_else(PoisonError::into_inner);
        *waiters -= 1;
        if *waiters == 0 && self.flight.outcome.borrow().is_none() {
            self.flight.cancel.cancel();
            if running.get(&self.key).is_some_and(|flight| Arc::ptr_eq(flight, &self.flight)) {
                running.remove(&self.key);
            }
        }
    }
}
