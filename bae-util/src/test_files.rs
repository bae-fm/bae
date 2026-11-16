//! This module gives a test that opens a file, and has no budget to watch,
//! its own open-files budget.

use coven::{FileBudget, FileLease, LeasePlace};

/// Creates a test budget and leases all its places.
pub fn lease() -> FileLease {
    let budget = FileBudget::for_test();
    budget.lease_blocking(budget.size()).expect("a test budget holds its own size")
}

/// Takes one place of a new test budget on a separate thread. tokio limits
/// how much a task runs before it must yield, and a long test can reach that
/// limit, which would stall a wait on the runtime's thread.
pub fn place() -> LeasePlace {
    let lease = lease();
    std::thread::scope(|threads| {
        threads
            .spawn(|| lease.place_blocking())
            .join()
            .expect("the thread that takes a place panicked")
    })
}
