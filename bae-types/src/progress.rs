//! How much of a piece of work is done, as the bar that shows it fills.

/// The share of a whole of `total` units that `done` of them make, from 0
/// to 1, which a bar fills to. A whole of no units has none of them done.
/// Whatever counts the units keeps `done` within `total`.
pub fn share_done(done: u64, total: u64) -> f64 {
    if total == 0 {
        return 0.0;
    }
    done as f64 / total as f64
}

/// The share a step's bar fills to: the share of its units done, or `None`
/// for a step of no units. Such a step counts nothing, so its bar moves
/// without a count.
pub fn step_share(done: u64, total: u64) -> Option<f64> {
    (total > 0).then(|| share_done(done, total))
}

#[cfg(test)]
#[path = "progress_tests.rs"]
mod tests;
