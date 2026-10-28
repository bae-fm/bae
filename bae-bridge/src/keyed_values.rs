//! The one shape a host receives values core sends many times a second in:
//! a `KeyedValueStream`, keyed by file, release or track.
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
    values: Mutex<HashMap<String, BridgeFastValue>>,
    activity: Box<dyn KeyActivityCallback>,
}

impl StreamValues {
    pub(crate) fn new(activity: Box<dyn KeyActivityCallback>) -> Arc<Self> {
        Arc::new(Self {
            values: Mutex::new(HashMap::new()),
            activity,
        })
    }

    /// Change the values, then tell the keys `change` says started or
    /// stopped holding one.
    fn write(&self, change: impl FnOnce(&mut HashMap<String, BridgeFastValue>) -> Vec<String>) {
        let started_or_stopped = change(&mut self.values.lock().unwrap());
        if !started_or_stopped.is_empty() {
            self.activity.on_keys_changed(started_or_stopped);
        }
    }
}

/// The latest value of each key one fast stream names, written by the feed
/// that made it until the stream is cancelled or dropped, or the library
/// closes. Any thread reads it.
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
    pub fn latest(&self, key: String) -> Option<BridgeFastValue> {
        self.values.values.lock().unwrap().get(&key).cloned()
    }

    /// Stop writing the stream; it holds nothing once its feed has ended.
    pub fn cancel(&self) {
        self.stop.cancel();
    }
}

impl Drop for KeyedValueStream {
    fn drop(&mut self) {
        self.stop.cancel();
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
    pub(crate) fn set(&self, key: String, value: BridgeFastValue) {
        self.values
            .write(|values| match values.insert(key.clone(), value) {
                None => vec![key],
                Some(_) => vec![],
            });
    }

    /// The stream stopped naming `key`.
    pub(crate) fn remove(&self, key: &str) {
        self.values.write(|values| match values.remove(key) {
            Some(_) => vec![key.to_string()],
            None => vec![],
        });
    }

    /// The stream names every key in `all`, and none other.
    pub(crate) fn reset(&self, all: HashMap<String, BridgeFastValue>) {
        self.values.write(|values| {
            let mut changed: Vec<String> = values
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
