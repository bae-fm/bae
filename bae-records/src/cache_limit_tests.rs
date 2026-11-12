use super::CacheLimit;

/// Each choice is decimal gigabytes and reads back from its bytes; a limit
/// no choice names reads as none.
#[test]
fn each_choice_round_trips_through_its_bytes() {
    let bytes: Vec<u64> = CacheLimit::ALL.into_iter().map(CacheLimit::bytes).collect();
    assert_eq!(
        bytes,
        [1_000_000_000, 5_000_000_000, 10_000_000_000, 20_000_000_000, 50_000_000_000]
    );
    for limit in CacheLimit::ALL {
        assert_eq!(CacheLimit::from_bytes(limit.bytes()), Some(limit));
    }
    assert_eq!(CacheLimit::from_bytes(20 * 1024 * 1024 * 1024), None);
    assert_eq!(CacheLimit::from_bytes(0), None);
}

/// The default is one of the choices.
#[test]
fn the_default_is_a_choice() {
    assert!(CacheLimit::ALL.contains(&CacheLimit::DEFAULT));
    assert_eq!(CacheLimit::DEFAULT.bytes(), 20_000_000_000);
}
