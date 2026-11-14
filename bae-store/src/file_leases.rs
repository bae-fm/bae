//! How a piece of work takes places of the installation's budget of open
//! files, which the store holds.

use coven::{FileBudget, FileLease, LeaseTooLarge};

/// The way to the installation's places for open files, which the store
/// hands to work that opens the person's files on threads and in services of
/// its own. A piece of work takes every place it will hold at once, in one
/// wait (`lease`), and opens each file in one of them. It calls a lease only
/// while holding no place of the budget, and a wait inside its lease is only
/// ever for a place one of its own files gives back.
#[derive(Clone)]
pub struct FileLeases {
    files: FileBudget,
}

impl FileLeases {
    pub(crate) fn of(files: FileBudget) -> Self {
        Self { files }
    }

    /// How many places the budget has.
    pub fn places(&self) -> usize {
        self.files.size()
    }

    /// `count` places, in one wait behind every lease that began waiting
    /// before. A `count` above the budget's places fails at once.
    pub async fn lease(&self, count: usize) -> Result<FileLease, LeaseTooLarge> {
        self.files.lease(count).await
    }

    /// [`Self::lease`], blocking this thread: call it only on the blocking
    /// pool or a thread of the caller's own, never in an async task, which
    /// can wait forever once the runtime's budget of work for the task is
    /// spent. The same holds for taking a place of the lease it returns with
    /// `place_blocking`.
    pub fn lease_blocking(&self, count: usize) -> Result<FileLease, LeaseTooLarge> {
        self.files.lease_blocking(count)
    }

    /// A lease of one place, for work that reads one file at a time: a place
    /// is always there to take, since a budget holds at least one.
    pub async fn lease_one(&self) -> FileLease {
        self.files.lease(1).await.expect("a budget holds at least one place")
    }

    /// [`Self::lease_one`], blocking this thread: call it only off the async
    /// workers.
    pub fn lease_one_blocking(&self) -> FileLease {
        self.files.lease_blocking(1).expect("a budget holds at least one place")
    }

    /// The leases of `files`, a budget a test sizes and watches.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn for_test(files: FileBudget) -> Self {
        Self { files }
    }

    /// The leases of a budget of the places an installation made for a test
    /// has, with no other work competing for them.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn for_test_default() -> Self {
        Self::for_test(FileBudget::for_test())
    }
}
