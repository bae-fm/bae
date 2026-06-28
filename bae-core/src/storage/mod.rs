//! bae's storage helpers around coven's cloud backends + encrypted layout. The
//! locality-aware blob read/write, the pin/unpin queue, and the offline-home
//! stub all live behind [`coven::CovenHandle`] now; callers name coven's cloud
//! types (`CloudHome`, `S3CloudHome`, …) directly. What remains here is the
//! deferred local-cleanup manifest and the readable-path helper.
pub mod local;
pub mod readable_path;
