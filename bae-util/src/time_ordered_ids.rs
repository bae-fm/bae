//! bae's ids: UUIDv7s made from the clock the owner was handed.

use std::sync::Mutex;
use std::time::UNIX_EPOCH;

use coven::{ClockRef, IdSource};
use uuid::{ContextV7, Timestamp, Uuid};

/// Ids whose first 48 bits are the clock's time in milliseconds, so rows one
/// write makes sit together in the store's indexes. Ids made within one
/// millisecond still ascend in the order they were made.
pub struct TimeOrderedIds {
    clock: ClockRef,
    /// Keeps ids of one millisecond in order. It is not safe to share, so
    /// calls take turns.
    context: Mutex<ContextV7>,
}

impl TimeOrderedIds {
    /// Ids timed by `clock`.
    pub fn new(clock: ClockRef) -> Self {
        Self { clock, context: Mutex::new(ContextV7::new()) }
    }
}

impl IdSource for TimeOrderedIds {
    fn new_id(&self) -> Uuid {
        let since_epoch = self
            .clock
            .now()
            .duration_since(UNIX_EPOCH)
            .expect("an id is made at a clock reading after the Unix epoch");
        let context = self.context.lock().expect("the id context's lock is never poisoned");
        Uuid::new_v7(Timestamp::from_unix(
            &*context,
            since_epoch.as_secs(),
            since_epoch.subsec_nanos(),
        ))
    }
}

#[cfg(test)]
#[path = "time_ordered_ids_tests.rs"]
mod tests;
