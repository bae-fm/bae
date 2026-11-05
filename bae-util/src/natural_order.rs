//! The order a person reads names in.

/// Natural order: "Disc 2" before "Disc 10" and letters without regard to
/// case, then by bytes where that finds two strings alike, so two different
/// strings never tie.
pub fn compare(left: &str, right: &str) -> std::cmp::Ordering {
    natord::compare_ignore_case(left, right).then_with(|| left.cmp(right))
}
