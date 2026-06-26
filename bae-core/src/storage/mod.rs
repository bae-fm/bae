//! bae's storage: coven's cloud backends + encrypted layout, with bae's local
//! remote-storage layer (pin/unpin, deferred cleanup) on top.
pub use coven::storage::cloud;
pub mod local;
pub mod offline;
pub mod readable_path;

pub use offline::OfflineSyncStorage;

// The ranged, scope-aware cloud blob reader lives in coven, which owns the blob
// format. Playback streams a track and pin downloads a file one window at a
// time through it.
pub use coven::sync::cloud_storage::BlobRangeReader;
