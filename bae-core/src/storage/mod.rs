//! bae's storage helpers around coven's cloud backends + encrypted layout. The
//! locality-aware blob read/write and blob-store path layout live behind
//! [`coven::CovenHandle`]; callers name coven's cloud types (`CloudHome`,
//! `S3CloudHome`, ...) directly. What remains here is bae's transfer adapter
//! and readable-home cloud-key policy.
pub mod readable_path;
pub mod transfer;
