//! The one shape a host receives values core sends many times a second in:
//! a `KeyedValueStream`, keyed by bytes: a file's, release's or track's id
//! as UTF-8, or a candidate's key, which need not be text.
//!
//! A host never receives such a value in a callback, a record field or a
//! subscription value. A stream built in [`crate::screens`] is written on the
//! bridge's side; the host reads a key's latest value with `latest` when the
//! native view drawing that key draws a frame. What the host pulls, as it
//! pulls every screen value, is which keys started or stopped holding a value
//! or moving, so a view knows when to start and stop drawing, and a failed
//! read of the feed; a value moving tells nothing. A key whose value is at
//! rest (a paused track's position) is not read every frame: its value
//! changing is pulled instead, since nothing else tells the view.

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use bae_library::CancellationToken;
use tokio::sync::Notify;
use tokio::time::{Duration, Instant};

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
    Open(HashMap<Vec<u8>, Entry>),
    Cancelled,
}

/// How long a value the feed writes only when it changes goes unchanged
/// before it counts as at rest: a stalled or paused download, a held bar.
/// Longer than a display frame, so a value moving at a steady pace is not
/// flipped between moving and resting; the key rests when the feed has
/// said nothing of it for this long.
pub(crate) const QUIET: Duration = Duration::from_secs(1);

/// One key's value, and whether it is at rest: not changing until the feed
/// says so, so a view need not read it each frame.
#[derive(Clone)]
struct Entry {
    value: BridgeFastValue,
    resting: bool,
    /// When the value last changed (when the key came, for a key at rest by
    /// the feed's say-so).
    changed_at: Instant,
}

impl Entry {
    /// Whether a view that last saw `before` of this key must be told: the key
    /// came, went, or flipped between moving and resting, or it rests and its
    /// value changed. A moving key's value changing is not told.
    fn tells_change_from(&self, before: Option<&Entry>) -> bool {
        match before {
            None => true,
            Some(before) => {
                before.resting != self.resting || (self.resting && before.value != self.value)
            }
        }
    }
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

    pub(crate) fn latest(&self, key: &[u8]) -> Option<BridgeFastValue> {
        match &lock(&self.held).values {
            Values::Open(values) => values.get(key).map(|entry| entry.value.clone()),
            Values::Cancelled => None,
        }
    }

    /// Whether the key holds a value that is not at rest.
    pub(crate) fn moving(&self, key: &[u8]) -> bool {
        match &lock(&self.held).values {
            Values::Open(values) => values.get(key).is_some_and(|entry| !entry.resting),
            Values::Cancelled => false,
        }
    }

    /// When the first key that is moving has gone [`QUIET`] unchanged, if any
    /// is moving: the feed rests it then (see [`StreamWriter::rest_quiet`]).
    fn next_rest(&self) -> Option<Instant> {
        match &lock(&self.held).values {
            Values::Open(values) => values
                .values()
                .filter(|entry| !entry.resting)
                .map(|entry| entry.changed_at + QUIET)
                .min(),
            Values::Cancelled => None,
        }
    }

    /// Change the values, and keep the keys `change` says a view must be told
    /// of (see [`Entry::tells_change_from`]) for the next pull. A cancelled stream takes no change.
    fn write(&self, change: impl FnOnce(&mut HashMap<Vec<u8>, Entry>) -> Vec<Vec<u8>>) {
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

    /// Whether the key holds a value that is changing: a view reads such a
    /// key each frame, and a key at rest only when a pull names it.
    pub fn moving(&self, key: Vec<u8>) -> bool {
        self.values.moving(&key)
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

/// What `feed_outbox_transfers` hands the host: the two streams one read of
/// the outbox's transfers writes.
#[derive(uniffi::Enum)]
pub enum BridgeOutboxFeeds {
    Streams {
        /// Each release with files being sent, by release id. A failed read
        /// empties the stream and is pulled as an error; the next read fills
        /// it again.
        releases: Arc<KeyedValueStream>,
        /// The files being sent together, under the one key `""` while any
        /// file is being sent; with none, it holds nothing. A failed read is
        /// told as the releases' stream's is.
        total: Arc<KeyedValueStream>,
    },
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

    /// The stream names `key` with `value`, a value that keeps changing until
    /// the feed has said nothing of it for [`QUIET`] (see [`Self::rest_quiet`]).
    /// The value it already holds changes nothing.
    pub(crate) fn set(&self, key: impl Into<Vec<u8>>, value: BridgeFastValue) {
        self.set_at(key, value, Instant::now());
    }

    fn set_at(&self, key: impl Into<Vec<u8>>, value: BridgeFastValue, now: Instant) {
        let key = key.into();
        self.values.write(|values| {
            if values.get(&key).is_some_and(|held| held.value == value) {
                return vec![];
            }
            let entry = Entry { value, resting: false, changed_at: now };
            let tell = entry.tells_change_from(values.get(&key));
            values.insert(key.clone(), entry);
            if tell {
                vec![key]
            } else {
                vec![]
            }
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

    /// The stream names every key in `all`, and none other, each with a value
    /// that keeps changing until the feed has said nothing of it for
    /// [`QUIET`]. A key whose value is the one it holds stays as it is.
    pub(crate) fn reset(&self, all: HashMap<Vec<u8>, BridgeFastValue>) {
        self.reset_at(all, Instant::now());
    }

    fn reset_at(&self, all: HashMap<Vec<u8>, BridgeFastValue>, now: Instant) {
        self.values.write(|values| {
            let all: HashMap<Vec<u8>, Entry> = all
                .into_iter()
                .map(|(key, value)| {
                    let entry = match values.get(&key) {
                        Some(held) if held.value == value => held.clone(),
                        _ => Entry { value, resting: false, changed_at: now },
                    };
                    (key, entry)
                })
                .collect();
            replace_values(values, all)
        });
    }

    /// Every key whose value has gone [`QUIET`] without changing rests, and
    /// a view is told of each; a value that changes again moves it again.
    /// Answers when the next moving key will have been quiet that long.
    pub(crate) fn rest_quiet(&self) -> Option<Instant> {
        self.rest_quiet_at(Instant::now())
    }

    fn rest_quiet_at(&self, now: Instant) -> Option<Instant> {
        self.values.write(|values| {
            values
                .iter_mut()
                .filter(|(_, entry)| !entry.resting && entry.changed_at + QUIET <= now)
                .map(|(key, entry)| {
                    entry.resting = true;
                    key.clone()
                })
                .collect()
        });
        self.values.next_rest()
    }

    /// The stream names every key in `all`, and none other; `resting` says
    /// which of them stay as they are until the feed writes again.
    pub(crate) fn reset_resting(
        &self,
        all: HashMap<Vec<u8>, BridgeFastValue>,
        resting: impl Fn(&[u8], &BridgeFastValue) -> bool,
    ) {
        let now = Instant::now();
        self.values.write(|values| {
            let all: HashMap<Vec<u8>, Entry> = all
                .into_iter()
                .map(|(key, value)| {
                    let resting = resting(&key, &value);
                    (key, Entry { value, resting, changed_at: now })
                })
                .collect();
            replace_values(values, all)
        });
    }

    /// The feed's read failed: the stream holds nothing until the next read
    /// fills it, and the host pulls `error`.
    pub(crate) fn fail(&self, error: BridgeError) {
        self.reset(HashMap::new());
        self.values.fail(error);
    }
}

/// Hold `all` in place of `values`, and answer the keys a view must be told
/// of: those that left, and those that came or flipped (see
/// [`Entry::tells_change_from`]).
fn replace_values(
    values: &mut HashMap<Vec<u8>, Entry>,
    all: HashMap<Vec<u8>, Entry>,
) -> Vec<Vec<u8>> {
    let changed: Vec<Vec<u8>> = values
        .keys()
        .filter(|key| !all.contains_key(*key))
        .chain(
            all.iter()
                .filter(|(key, entry)| entry.tells_change_from(values.get(*key)))
                .map(|(key, _)| key),
        )
        .cloned()
        .collect();
    *values = all;
    changed
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
