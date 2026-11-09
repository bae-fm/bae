use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

use coven::{FixedClock, IdSource};

use super::*;

#[test]
fn ids_made_at_one_instant_are_distinct_and_ascending() {
    let ids = TimeOrderedIds::new(Arc::new(FixedClock::new(UNIX_EPOCH + Duration::from_secs(1))));
    let first = ids.new_id();
    let second = ids.new_id();
    assert_ne!(first, second);
    assert!(first < second, "{first} should sort before {second}");
}

#[test]
fn an_ids_timestamp_is_the_clocks_time_to_the_millisecond() {
    let now = UNIX_EPOCH + Duration::from_millis(1_762_560_000_123) + Duration::from_micros(456);
    let ids = TimeOrderedIds::new(Arc::new(FixedClock::new(now)));
    let id = ids.new_id();
    assert_eq!(id.get_version_num(), 7);
    let (seconds, nanos) = id.get_timestamp().expect("a v7 id carries a timestamp").to_unix();
    assert_eq!((seconds, nanos / 1_000_000), (1_762_560_000, 123));
}

#[test]
fn a_later_clock_reading_makes_a_later_id() {
    let clock = Arc::new(FixedClock::new(UNIX_EPOCH + Duration::from_secs(5)));
    let ids = TimeOrderedIds::new(clock.clone());
    let earlier = ids.new_id();
    clock.set(UNIX_EPOCH + Duration::from_secs(6));
    assert!(earlier < ids.new_id());
}
