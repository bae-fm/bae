//! How a piece of work takes one place of the installation's budget of open
//! files. Each holder that opens the person's files holds coven's
//! [`FileBudget`], which the app's composition hands it; a piece of work takes
//! every place it will hold at once, in one wait, and calls a lease only while
//! it holds no place of the budget.

use coven::{FileBudget, FileLease};

/// A lease of one place of `files`, for work that reads one file at a time: a
/// place is always there to take, since a budget holds at least one.
pub async fn lease_one(files: &FileBudget) -> FileLease {
    files.lease(1).await.expect("a budget holds at least one place")
}
