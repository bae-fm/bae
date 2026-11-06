//! The one shape a host receives values core sends many times a second in:
//! a `KeyedValueStream`, keyed by bytes: a file's, release's or track's id
//! as UTF-8, or a candidate's key, which need not be text.
//!
//! A host never receives such a value in a callback, a record field or a
//! subscription value. A stream built in [`crate::screens`] is written on the
//! bridge's side; the host reads a key's latest value with `latest` when the
//! native view drawing that key draws a frame. What the host pulls, as it
//! pulls every screen value, is which keys started or stopped holding a value,
//! so a view knows when to start and stop drawing, and a failed read of the
//! feed; a value moving tells nothing.

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use bae_library::CancellationToken;
use tokio::sync::Notify;

use crate::operation_runtime::OperationRuntime;
use crate::types::{BridgeError, BridgeFastValue};

/// The key of a stream that holds one value.
const ONLY_KEY: &[u8] = b"";

/// The latest value of each key, and what the host has not pulled yet.
pub(crate) struct StreamValues {
    /// Each change to what the stream holds is one insert, remove or whole
    /// replace of the map and of what is unpulled, none of which panics
    /// partway, so a poisoned lock says nothing about it; and both `Drop`s
    /// below take it, where a panic must not start.
    held: Mutex<Held>,
    /// Woken when something unpulled arrives. A wake with no pull waiting is
    /// kept for the next pull.
    unpulled: Notify,
}

struct Held {
    values: Values,
    /// The keys that started or stopped holding a value since the last pull,
    /// a key that came and went once.
    keys: BTreeSet<Vec<u8>>,
    /// The feed's latest failed read since the last pull.
    failure: Option<BridgeError>,
}

/// What a stream holds and what is unpulled; a poisoned lock says nothing
/// about it (see [`StreamValues::held`]).
fn lock(held: &Mutex<Held>) -> MutexGuard<'_, Held> {
    held.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What a stream holds: each key's value until the stream is cancelled,
/// and nothing from then on.
enum Values {
    Open(HashMap<Vec<u8>, BridgeFastValue>),
    Cancelled,
}

impl StreamValues {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            held: Mutex::new(Held {
                values: Values::Open(HashMap::new()),
                keys: BTreeSet::new(),
                failure: None,
            }),
            unpulled: Notify::new(),
        })
    }

    fn latest(&self, key: &[u8]) -> Option<BridgeFastValue> {
        match &lock(&self.held).values {
            Values::Open(values) => values.get(key).cloned(),
            Values::Cancelled => None,
        }
    }

    /// Change the values, and keep the keys `change` says started or stopped
    /// holding one for the next pull. A cancelled stream takes no change.
    fn write(&self, change: impl FnOnce(&mut HashMap<Vec<u8>, BridgeFastValue>) -> Vec<Vec<u8>>) {
        let mut held = lock(&self.held);
        let started_or_stopped = match &mut held.values {
            Values::Open(values) => change(values),
            Values::Cancelled => return,
        };
        if started_or_stopped.is_empty() {
            return;
        }
        held.keys.extend(started_or_stopped);
        drop(held);
        self.unpulled.notify_one();
    }

    /// The feed's read failed; the next pull after the keys it emptied
    /// answers with `error`. A cancelled stream takes none.
    fn fail(&self, error: BridgeError) {
        let mut held = lock(&self.held);
        if let Values::Cancelled = held.values {
            return;
        }
        held.failure = Some(error);
        drop(held);
        self.unpulled.notify_one();
    }

    /// Hold nothing from now on, whatever writes it; the next pull answers
    /// with the keys that held a value, and every one after it that the
    /// stream was cancelled.
    fn cancel(&self) {
        let mut held = lock(&self.held);
        if let Values::Open(values) = std::mem::replace(&mut held.values, Values::Cancelled) {
            held.keys.extend(values.into_keys());
        }
        held.failure = None;
        drop(held);
        self.unpulled.notify_one();
    }

    /// What the host has not pulled: the keys that started or stopped since
    /// the last pull, in byte order; else the feed's failed read; else, on a
    /// cancelled stream, that it was cancelled. Waits while there is none.
    async fn next(&self) -> Result<Vec<Vec<u8>>, BridgeError> {
        loop {
            let unpulled = self.unpulled.notified();
            {
                let mut held = lock(&self.held);
                if !held.keys.is_empty() {
                    return Ok(std::mem::take(&mut held.keys).into_iter().collect());
                }
                if let Some(failure) = held.failure.take() {
                    return Err(failure);
                }
                if let Values::Cancelled = held.values {
                    return Err(BridgeError::Cancelled);
                }
            }
            unpulled.await;
        }
    }
}

/// The latest value of each key one fast stream names, written by the feed
/// that made it until the stream is cancelled or dropped, or the library
/// closes. Cancelled, it holds nothing and nothing writes it. Any thread
/// reads it.
#[derive(uniffi::Object)]
pub struct KeyedValueStream {
    values: Arc<StreamValues>,
    stop: CancellationToken,
    /// Where a pull runs.
    runtime: OperationRuntime,
}

impl KeyedValueStream {
    pub(crate) fn new(
        values: Arc<StreamValues>,
        stop: CancellationToken,
        runtime: OperationRuntime,
    ) -> Self {
        Self { values, stop, runtime }
    }
}

#[uniffi::export]
impl KeyedValueStream {
    /// The key's latest value; `None` while the stream names none for it.
    pub fn latest(&self, key: Vec<u8>) -> Option<BridgeFastValue> {
        self.values.latest(&key)
    }

    /// End the stream's feed: from now on it holds nothing, and nothing
    /// writes it.
    pub fn cancel(&self) {
        self.stop.cancel();
        self.values.cancel();
    }
}

#[uniffi::export(async_runtime = "tokio", cancellable)]
impl KeyedValueStream {
    /// The keys that started or stopped holding a value since the last
    /// pull, in byte order, a key that came and went once: `latest` says
    /// which. Waits until one does. A failed read of the feed answers as that
    /// error once the keys it emptied are pulled; the stream fills again with
    /// the next read. Once cancelled, the next pull answers with the keys
    /// that held a value then, and every pull after it that it was cancelled.
    pub async fn next(self: Arc<Self>) -> Result<Vec<Vec<u8>>, BridgeError> {
        let runtime = self.runtime.clone();
        runtime.run("KeyedValueStream.next", move || async move { self.values.next().await }).await
    }
}

impl Drop for KeyedValueStream {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// The feed's writing of a stream. Dropping it, as the feed ends, empties
/// the stream: what it held is no longer known to be current.
pub(crate) struct StreamWriter {
    values: Arc<StreamValues>,
}

impl StreamWriter {
    pub(crate) fn new(values: Arc<StreamValues>) -> Self {
        Self { values }
    }

    /// The stream names `key` with `value`.
    pub(crate) fn set(&self, key: impl Into<Vec<u8>>, value: BridgeFastValue) {
        let key = key.into();
        self.values.write(|values| match values.insert(key.clone(), value) {
            None => vec![key],
            Some(_) => vec![],
        });
    }

    /// The stream stopped naming `key`.
    pub(crate) fn remove(&self, key: &[u8]) {
        self.values.write(|values| match values.remove(key) {
            Some(_) => vec![key.to_vec()],
            None => vec![],
        });
    }

    /// A stream of one value holds `value` under its one key, `""`.
    pub(crate) fn set_only(&self, value: BridgeFastValue) {
        self.set(ONLY_KEY, value);
    }

    /// A stream of one value holds nothing.
    pub(crate) fn remove_only(&self) {
        self.remove(ONLY_KEY);
    }

    /// The stream names every key in `all`, and none other.
    pub(crate) fn reset(&self, all: HashMap<Vec<u8>, BridgeFastValue>) {
        self.values.write(|values| {
            let changed: Vec<Vec<u8>> = values
                .keys()
                .filter(|key| !all.contains_key(*key))
                .chain(all.keys().filter(|key| !values.contains_key(*key)))
                .cloned()
                .collect();
            *values = all;
            changed
        });
    }

    /// The feed's read failed: the stream holds nothing until the next read
    /// fills it, and the host pulls `error`.
    pub(crate) fn fail(&self, error: BridgeError) {
        self.reset(HashMap::new());
        self.values.fail(error);
    }
}

impl Drop for StreamWriter {
    fn drop(&mut self) {
        self.reset(HashMap::new());
    }
}

/// The stream of one value a call reports how far it has got in: the one the
/// host last handed out, pulled on the host's runtime. A call that starts
/// after the host takes a new stream writes that one; one nobody reads writes
/// nowhere.
pub(crate) struct ProgressStream {
    /// Where the stream's pulls run.
    runtime: OperationRuntime,
    /// The stream [`Self::feed`] last handed out.
    values: Mutex<Option<Arc<StreamValues>>>,
}

impl ProgressStream {
    pub(crate) fn new(runtime: OperationRuntime) -> Self {
        Self { runtime, values: Mutex::new(None) }
    }

    /// A new stream, which calls from now on write. A cancelled stream holds
    /// nothing from then on.
    pub(crate) fn feed(&self) -> Arc<KeyedValueStream> {
        let values = StreamValues::new();
        *self.values.lock().unwrap() = Some(values.clone());
        Arc::new(KeyedValueStream::new(values, CancellationToken::new(), self.runtime.clone()))
    }

    /// Where one call writes: the stream last handed out, or nowhere before
    /// the host took one.
    pub(crate) fn writer(&self) -> ProgressWriter {
        ProgressWriter { writer: self.values.lock().unwrap().clone().map(StreamWriter::new) }
    }
}

/// One call's writing of a [`ProgressStream`]. Dropping it, as the call
/// ends, empties the stream.
pub(crate) struct ProgressWriter {
    writer: Option<StreamWriter>,
}

impl ProgressWriter {
    /// Where nothing reads the call's progress.
    pub(crate) fn unread() -> Self {
        Self { writer: None }
    }

    /// The stream holds `value`.
    pub(crate) fn set(&self, value: BridgeFastValue) {
        if let Some(writer) = &self.writer {
            writer.set_only(value);
        }
    }

    /// The stream holds nothing.
    pub(crate) fn clear(&self) {
        if let Some(writer) = &self.writer {
            writer.remove_only();
        }
    }
}

#[cfg(test)]
#[path = "keyed_values_tests.rs"]
mod tests;
