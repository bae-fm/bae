//! The one shape a host receives values core sends many times a second in:
//! a `KeyedValueStream`, keyed by bytes: a file's, release's or track's id
//! as UTF-8, or a candidate's key, which need not be text.
//!
//! A host never receives such a value in a callback, a record field or a
//! subscription value. An `AppHandle` `feed_*` call starts writing one
//! stream on the bridge's side and hands back the `KeyedValueStream` it
//! writes; the host reads a key's latest value with `latest` when the native
//! view drawing that key draws a frame. The only callback a stream makes is
//! which keys started or stopped holding a value, so a view knows when to
//! start and stop drawing; a value moving tells nothing.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use bae_core::library::CancellationToken;

use crate::types::{BridgeFastValue, KeyActivityCallback};

/// The latest value of each key, and who hears keys come and go.
pub(crate) struct StreamValues {
    held: Mutex<Held>,
    activity: Box<dyn KeyActivityCallback>,
}

/// What a stream holds: each key's value until the stream is cancelled,
/// and nothing from then on.
enum Held {
    Open(HashMap<Vec<u8>, BridgeFastValue>),
    Cancelled,
}

impl StreamValues {
    pub(crate) fn new(activity: Box<dyn KeyActivityCallback>) -> Arc<Self> {
        Arc::new(Self {
            held: Mutex::new(Held::Open(HashMap::new())),
            activity,
        })
    }

    fn latest(&self, key: &[u8]) -> Option<BridgeFastValue> {
        match &*self.held.lock().unwrap() {
            Held::Open(values) => values.get(key).cloned(),
            Held::Cancelled => None,
        }
    }

    /// Change the values, then tell the keys `change` says started or
    /// stopped holding one. A cancelled stream takes no change.
    fn write(&self, change: impl FnOnce(&mut HashMap<Vec<u8>, BridgeFastValue>) -> Vec<Vec<u8>>) {
        let started_or_stopped = match &mut *self.held.lock().unwrap() {
            Held::Open(values) => change(values),
            Held::Cancelled => Vec::new(),
        };
        if !started_or_stopped.is_empty() {
            self.activity.on_keys_changed(started_or_stopped);
        }
    }

    /// Hold nothing from now on, whatever writes it, and tell the keys that
    /// held a value.
    fn cancel(&self) {
        let stopped = match std::mem::replace(&mut *self.held.lock().unwrap(), Held::Cancelled) {
            Held::Open(values) => {
                let mut keys: Vec<Vec<u8>> = values.into_keys().collect();
                keys.sort();
                keys
            }
            Held::Cancelled => Vec::new(),
        };
        if !stopped.is_empty() {
            self.activity.on_keys_changed(stopped);
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
}

impl KeyedValueStream {
    pub(crate) fn new(values: Arc<StreamValues>, stop: CancellationToken) -> Self {
        Self { values, stop }
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
        self.values
            .write(|values| match values.insert(key.clone(), value) {
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

    /// The stream names every key in `all`, and none other.
    pub(crate) fn reset(&self, all: HashMap<Vec<u8>, BridgeFastValue>) {
        self.values.write(|values| {
            let mut changed: Vec<Vec<u8>> = values
                .keys()
                .filter(|key| !all.contains_key(*key))
                .chain(all.keys().filter(|key| !values.contains_key(*key)))
                .cloned()
                .collect();
            changed.sort();
            *values = all;
            changed
        });
    }
}

impl Drop for StreamWriter {
    fn drop(&mut self) {
        self.reset(HashMap::new());
    }
}

#[cfg(test)]
#[path = "keyed_values_tests.rs"]
mod tests;
