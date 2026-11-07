//! How to stop each transfer making a release local that this process runs.
//!
//! Which transition a release is in is its `release_storage_action` row; a
//! token can't be a row, so the one transition this process stops between
//! files keeps its token here, for exactly as long as the transfer runs.

use crate::CancellationToken;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub(crate) struct DetachCancels {
    tokens: Arc<Mutex<HashMap<String, CancellationToken>>>,
}

/// Holds a release's token for as long as its transfer runs. Dropping it,
/// however the transfer ends, removes the token.
pub(crate) struct DetachRegistration {
    cancels: DetachCancels,
    release_id: String,
}

impl Drop for DetachRegistration {
    fn drop(&mut self) {
        self.cancels.tokens.lock().unwrap().remove(&self.release_id);
    }
}

impl DetachCancels {
    pub(crate) fn new() -> Self {
        Self { tokens: Arc::new(Mutex::new(HashMap::new())) }
    }

    /// Make `cancel` what stops `release_id`'s transfer until the returned
    /// registration drops. `None` while another transfer of the release
    /// holds its token.
    pub(crate) fn register(
        &self,
        release_id: &str,
        cancel: CancellationToken,
    ) -> Option<DetachRegistration> {
        match self.tokens.lock().unwrap().entry(release_id.to_string()) {
            std::collections::hash_map::Entry::Occupied(_) => None,
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(cancel);
                Some(DetachRegistration {
                    cancels: self.clone(),
                    release_id: release_id.to_string(),
                })
            }
        }
    }

    /// Stop `release_id`'s transfer; whether one was registered. The lookup
    /// and the fire share one lock, so a registration dropping cannot slip
    /// between them.
    pub(crate) fn cancel(&self, release_id: &str) -> bool {
        let tokens = self.tokens.lock().unwrap();
        let Some(cancel) = tokens.get(release_id) else {
            return false;
        };
        cancel.cancel();
        true
    }
}
