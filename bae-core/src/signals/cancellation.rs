//! Per-candidate cancellation registry for the extraction service.
//!
//! Each in-flight extraction is keyed by its candidate key and carries a
//! generation plus a cancellation token. Starting a new extraction for a key
//! cancels and replaces any prior one; a finishing task only releases its own
//! entry when the stored generation still matches, so an already-cancelled
//! task racing its successor on the way out can't evict the newer entry.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use tokio_util::sync::CancellationToken;

/// Generation-guarded map of candidate key to its in-flight extraction's
/// `(generation, cancellation token)`. The generation counter is atomic so
/// `register` can allocate a fresh id without holding the map lock.
#[derive(Default)]
pub(super) struct CancellationRegistry {
    cancel_tokens: Mutex<HashMap<String, (u64, CancellationToken)>>,
    next_generation: AtomicU64,
}

impl CancellationRegistry {
    /// Allocate a fresh generation and cancellation token for `key`, inserting
    /// the entry and cancelling + replacing any prior token for the same key.
    /// Returns the new token and generation for the task to carry.
    pub(super) fn register(&self, key: String) -> (CancellationToken, u64) {
        let token = CancellationToken::new();
        let generation = self.next_generation.fetch_add(1, Ordering::Relaxed);

        // Swap in the new entry atomically, cancelling any prior one — this
        // keeps register() consistent with cancel() even when a previous task
        // is mid-teardown.
        let prior = self
            .cancel_tokens
            .lock()
            .unwrap()
            .insert(key, (generation, token.clone()));
        if let Some((_, prior_token)) = prior {
            prior_token.cancel();
        }

        (token, generation)
    }

    /// Remove the entry for `key` and cancel its token.
    pub(super) fn cancel(&self, key: &str) {
        let entry = self.cancel_tokens.lock().unwrap().remove(key);
        if let Some((_, token)) = entry {
            token.cancel();
        }
    }

    /// Remove the entry for `key` only if it still refers to `generation`.
    /// Prevents a teardown from an older task erasing the entry for a newer
    /// task that already overwrote it.
    pub(super) fn release_if_current(&self, key: &str, generation: u64) {
        let mut tokens = self.cancel_tokens.lock().unwrap();
        if let Some((current_generation, _)) = tokens.get(key) {
            if *current_generation == generation {
                tokens.remove(key);
            }
        }
    }
}
