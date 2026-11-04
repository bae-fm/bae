use super::*;

/// A stall is reported once it has lasted the threshold, then at most once
/// a second, and at its end; a short one never is.
#[test]
fn a_stall_is_reported_after_a_while_then_once_a_second_and_at_its_end() {
    let start = Instant::now();
    let at = |ms: u64| start + Duration::from_millis(ms);
    let mut stall = Stall::default();

    assert_eq!(stall.waiting(at(0)), None);
    assert_eq!(stall.waiting(at(100)), None);
    assert_eq!(stall.waiting(at(300)), Some(Duration::from_millis(300)));
    assert_eq!(stall.waiting(at(900)), None);
    assert_eq!(stall.waiting(at(1_300)), Some(Duration::from_millis(1_300)));
    assert_eq!(stall.ended(at(1_500)), Some(Duration::from_millis(1_500)));
    assert_eq!(stall.ended(at(1_600)), None, "no stall in progress");

    stall.waiting(at(2_000));
    assert_eq!(stall.ended(at(2_100)), None, "a short stall is not reported");

    stall.waiting(at(3_000));
    stall.clear();
    assert_eq!(stall.ended(at(4_000)), None);
}
