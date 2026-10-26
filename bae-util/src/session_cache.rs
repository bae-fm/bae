use std::num::NonZeroUsize;
use std::sync::Mutex;

use lru::LruCache;

/// Capacity for a metadata provider's response cache. One entry per distinct
/// request URL, so a session's lookups, searches and the documents they settle
/// all share it. Eviction costs one network round-trip — the same as a cold
/// start.
pub const PROVIDER_RESPONSE_CAPACITY: usize = 512;

/// A bounded map of values kept for as long as its owner lives, least
/// recently used first out.
pub struct SessionCache<V> {
    name: &'static str,
    inner: Mutex<LruCache<String, V>>,
}

impl<V> SessionCache<V> {
    /// `capacity` is how many entries it holds before it starts evicting,
    /// sized by what it holds and how much of it one session touches, since
    /// eviction costs whatever producing the value cost.
    pub fn new(name: &'static str, capacity: usize) -> Self {
        let capacity = NonZeroUsize::new(capacity)
            .unwrap_or_else(|| panic!("{name} capacity must be greater than zero"));
        Self {
            name,
            inner: Mutex::new(LruCache::new(capacity)),
        }
    }

    pub fn get_cloned(&self, key: &str) -> Option<V>
    where
        V: Clone,
    {
        self.inner
            .lock()
            .unwrap_or_else(|_| panic!("{} mutex poisoned", self.name))
            .get(key)
            .cloned()
    }

    pub fn put(&self, key: impl Into<String>, value: V) {
        self.inner
            .lock()
            .unwrap_or_else(|_| panic!("{} mutex poisoned", self.name))
            .put(key.into(), value);
    }
}
