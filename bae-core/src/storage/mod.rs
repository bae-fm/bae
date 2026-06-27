//! bae's storage helpers around coven's cloud backends + encrypted layout. The
//! locality-aware blob read/write, the pin/unpin queue, and the offline-home
//! stub all live behind [`coven::CovenHandle`] now; what remains here is the
//! cloud-provider setup re-export and the deferred local-cleanup manifest.
pub use coven::storage::cloud;
pub mod local;
pub mod readable_path;
