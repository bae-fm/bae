//! bae's storage helpers around coven's cloud backends. The locality-aware blob
//! read/write and the blob-store layout live behind [`coven::CovenHandle`], and
//! callers name coven's cloud types directly; what's here is bae's transfer
//! adapter and its readable-home cloud-key policy.
pub mod readable_path;
pub mod transfer;
