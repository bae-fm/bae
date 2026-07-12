//! Per-candidate cancellation registry for the extraction service.
//!
//! Each in-flight extraction is keyed by its candidate key and carries a generation
//! plus a cancellation token. Starting a new extraction for a key cancels and
//! replaces any prior one; a finishing task releases its own entry only when the
//! stored generation still matches, so a cancelled task racing its successor on the
//! way out can't evict the newer entry.

use std::collections::HashMap;
use std::sync::Mutex;
use tokio_util::sync::CancellationToken;

/// The map from candidate key to its in-flight extraction's `(generation, token)`,
/// plus the counter that hands out generations. Both live under one mutex, so a
/// generation and its map entry advance together.
#[derive(Default)]
struct RegistryState {
    cancel_tokens: HashMap<String, (u64, CancellationToken)>,
    next_generation: u64,
}

#[derive(Default)]
pub(super) struct CancellationRegistry {
    state: Mutex<RegistryState>,
}

impl CancellationRegistry {
    /// A fresh generation and token for `key`, cancelling and replacing any prior
    /// one. The task carries both back.
    pub(super) fn register(&self, key: String) -> (CancellationToken, u64) {
        let token = CancellationToken::new();

        // The generation and the map entry advance under one lock hold, so two
        // concurrent registers for a key serialize and the higher generation always
        // wins the map.
        let mut state = self.state.lock().unwrap();
        let generation = state.next_generation;
        state.next_generation += 1;
        let prior = state.cancel_tokens.insert(key, (generation, token.clone()));
        drop(state);

        if let Some((_, prior_token)) = prior {
            prior_token.cancel();
        }

        (token, generation)
    }

    pub(super) fn cancel(&self, key: &str) {
        let entry = self.state.lock().unwrap().cancel_tokens.remove(key);
        if let Some((_, token)) = entry {
            token.cancel();
        }
    }

    /// Remove `key`'s entry only if it still refers to `generation` — so an older
    /// task's teardown can't erase the entry of the newer task that replaced it.
    pub(super) fn release_if_current(&self, key: &str, generation: u64) {
        let mut state = self.state.lock().unwrap();
        if let Some((current_generation, _)) = state.cancel_tokens.get(key) {
            if *current_generation == generation {
                state.cancel_tokens.remove(key);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn re_registering_a_key_advances_the_generation_and_replaces_the_token() {
        let registry = CancellationRegistry::default();

        let (first_token, first_gen) = registry.register("cand".to_string());
        let (second_token, second_gen) = registry.register("cand".to_string());

        assert!(second_gen > first_gen);
        assert!(first_token.is_cancelled());
        assert!(!second_token.is_cancelled());
    }

    #[test]
    fn generations_are_unique_across_keys() {
        let registry = CancellationRegistry::default();

        let (_, a_gen) = registry.register("a".to_string());
        let (_, b_gen) = registry.register("b".to_string());

        assert_ne!(a_gen, b_gen);
    }

    #[test]
    fn release_if_current_only_removes_the_matching_generation() {
        let registry = CancellationRegistry::default();

        let (_, stale_gen) = registry.register("cand".to_string());
        let (live_token, live_gen) = registry.register("cand".to_string());

        // A teardown carrying the older generation must not evict the newer entry
        // that already overwrote it.
        registry.release_if_current("cand", stale_gen);
        assert!(!live_token.is_cancelled());

        // The live generation's own teardown removes its entry, after which a repeat
        // release is a no-op rather than a hit on some later entry.
        registry.release_if_current("cand", live_gen);
        let (_, post_gen) = registry.register("cand".to_string());
        registry.release_if_current("cand", live_gen);
        assert!(post_gen > live_gen);
        registry.cancel("cand");
    }
}
