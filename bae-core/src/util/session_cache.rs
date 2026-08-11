use std::num::NonZeroUsize;
use std::sync::{Mutex, OnceLock};

use lru::LruCache;

/// Capacity for a metadata-provider lookup cache. Sized for a session of a few
/// imports, each touching 1-3 releases. Eviction costs one network round-trip —
/// the same as a cold start.
pub const PROVIDER_LOOKUP_CAPACITY: usize = 25;

pub struct SessionCache<V> {
    name: &'static str,
    /// How many entries the cache holds before it starts evicting. Sized per
    /// cache by what it holds and how much of it one session touches, since
    /// eviction costs whatever producing the value cost.
    capacity: usize,
    inner: OnceLock<Mutex<LruCache<String, V>>>,
}

impl<V> SessionCache<V> {
    pub const fn new(name: &'static str, capacity: usize) -> Self {
        Self {
            name,
            capacity,
            inner: OnceLock::new(),
        }
    }

    pub fn get_cloned(&self, key: &str) -> Option<V>
    where
        V: Clone,
    {
        self.inner
            .get_or_init(|| new_cache(self.name, self.capacity))
            .lock()
            .unwrap_or_else(|_| panic!("{} mutex poisoned", self.name))
            .get(key)
            .cloned()
    }

    pub fn put(&self, key: impl Into<String>, value: V) {
        self.inner
            .get_or_init(|| new_cache(self.name, self.capacity))
            .lock()
            .unwrap_or_else(|_| panic!("{} mutex poisoned", self.name))
            .put(key.into(), value);
    }
}

fn new_cache<V>(name: &str, capacity: usize) -> Mutex<LruCache<String, V>> {
    Mutex::new(LruCache::new(NonZeroUsize::new(capacity).unwrap_or_else(
        || panic!("{name} capacity must be greater than zero"),
    )))
}
