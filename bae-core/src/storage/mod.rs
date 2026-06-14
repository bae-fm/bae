//! bae's storage: coven's cloud backends + encrypted layout, with bae's local
//! managed-storage layer (pin/unpin, deferred cleanup) on top.
pub use coven::storage::cloud;
pub mod cloud_read;
pub mod local;

pub use cloud_read::CloudBlobReader;
