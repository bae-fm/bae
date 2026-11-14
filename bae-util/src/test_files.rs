//! Places of the open-files budget for a test that opens a file without a
//! budget to watch.

use coven::{FileBudget, FileLease, LeasePlace};

/// All the places of a budget of this call's own.
pub fn lease() -> FileLease {
    let budget = FileBudget::for_test();
    budget.lease_blocking(budget.size()).expect("a test budget holds its own size")
}

/// One place of a budget of this call's own, taken on a thread of its own: a
/// test running on a runtime's thread would otherwise wait inside the
/// runtime's budget of work per task, which a long test can have spent.
pub fn place() -> LeasePlace {
    let lease = lease();
    std::thread::scope(|threads| {
        threads.spawn(|| lease.place_blocking()).join().expect("a place is taken")
    })
}
