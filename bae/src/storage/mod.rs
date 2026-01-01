//! Storage abstraction layer
//!
//! Provides flexible storage options for releases. Storage is configured via
//! StorageProfile (location + encrypted) and implemented by a single
//! ReleaseStorageImpl that applies transforms based on the profile.
mod reader;
mod traits;

pub use reader::create_storage_reader;
pub use traits::{ReleaseStorage, ReleaseStorageImpl};
