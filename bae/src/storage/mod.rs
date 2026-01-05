//! Storage abstraction layer
//!
//! Provides flexible storage options for releases. Storage is configured via
//! StorageProfile (location + encrypted) and implemented by a single
//! ReleaseStorageImpl that applies transforms based on the profile.
mod reader;
mod traits;

pub use reader::{
    create_storage_reader, download_encrypted_to_streaming_buffer, download_to_streaming_buffer,
    download_to_streaming_buffer_with_range,
};
pub use traits::{ReleaseStorage, ReleaseStorageImpl};
