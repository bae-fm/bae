//! Whether the releases an import pane offers are already in the library,
//! read through one live query whose checks change as the offers do.

use bae_records::{LibraryChecks, LibraryStatus};
use bae_store::live_query::LiveRead;

/// One value the subscription delivered: a status for each release it was
/// asked about, and the revision of the request that asked.
#[derive(Debug, Clone)]
pub struct LibraryStatusSnapshot {
    pub statuses: Vec<LibraryStatus>,
    pub request_revision: u64,
}

/// The library membership of a set of offered releases. A new set of offers
/// is a new request on the same query, and one read per database change
/// answers every release in it.
pub type LiveLibraryStatus = LiveRead<LibraryChecks, LibraryStatusSnapshot>;
