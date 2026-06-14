//! Local managed blob storage. The blob store and content-addressed path layout
//! live in coven; bae keeps the pin/unpin transfer queue and deferred cleanup on
//! top. `ReleaseStorageImpl` is bae's name for coven's `BlobStore`.
pub mod cleanup;
pub mod transfer;

pub use coven::storage::local::{storage_path, BlobStore as ReleaseStorageImpl};
