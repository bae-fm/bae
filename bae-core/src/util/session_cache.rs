use std::num::NonZeroUsize;
use std::sync::{Mutex, OnceLock};

use lru::LruCache;

/// Capacity for each request-kind cache. Sized for a session of a few imports,
/// each touching 1-3 releases. Eviction costs one network round-trip — the same
/// as a cold start.
const SESSION_CACHE_CAPACITY: usize = 25;

pub struct SessionCache<V> {
    name: &'static str,
    inner: OnceLock<Mutex<LruCache<String, V>>>,
}

impl<V> SessionCache<V> {
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            inner: OnceLock::new(),
        }
    }

    pub fn get_cloned(&self, key: &str) -> Option<V>
    where
        V: Clone,
    {
        self.lock().get(key).cloned()
    }

    pub fn put(&self, key: impl Into<String>, value: V) {
        self.lock().put(key.into(), value);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, LruCache<String, V>> {
        self.inner
            .get_or_init(|| {
                Mutex::new(LruCache::new(
                    NonZeroUsize::new(SESSION_CACHE_CAPACITY).expect("SESSION_CACHE_CAPACITY > 0"),
                ))
            })
            .lock()
            .unwrap_or_else(|_| panic!("{} mutex poisoned", self.name))
    }
}
