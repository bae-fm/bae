//! A path on this device as it crosses the bridge: coven's bytes for it
//! (`coven::path_bytes`), so a name that isn't valid Unicode opens on both
//! sides. The app makes them from a URL's file-system representation and
//! opens a file from them the same way; core turns them back here.

use crate::types::BridgeError;
use std::path::PathBuf;

/// The path the app sent as `bytes`. Only bytes no path on this device has,
/// which neither core nor the file system gives the app, fail.
pub(crate) fn path_from_app(bytes: Vec<u8>) -> Result<PathBuf, BridgeError> {
    coven::path_from_bytes(bytes)
        .map_err(|error| BridgeError::internal(format!("a path from the app: {error}")))
}
