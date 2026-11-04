//! What the app sends that core reads back here: a path on this device as
//! coven's bytes for it (`coven::path_bytes`), so a name that isn't valid
//! Unicode opens on both sides — the app makes them from a URL's file-system
//! representation and opens a file from them the same way — and the ids and
//! names it passes as text.

use crate::types::BridgeError;
use std::path::PathBuf;

/// The path the app sent as `bytes`. Only bytes no path on this device has,
/// which neither core nor the file system gives the app, fail.
pub(crate) fn path(bytes: Vec<u8>) -> Result<PathBuf, BridgeError> {
    coven::path_from_bytes(bytes)
        .map_err(|error| BridgeError::internal(format!("a path from the app: {error}")))
}

/// The library the app names by `library_id`; text that is no library id is
/// the app's mistake, refused as a configuration error.
pub(crate) fn store(library_id: &str) -> Result<coven::StoreId, BridgeError> {
    Ok(coven::StoreId(library_id.parse().map_err(BridgeError::config)?))
}

/// The member the app names by `member_id`, refused as [`store`]
/// refuses a library's.
pub(crate) fn member(member_id: &str) -> Result<coven::MemberId, BridgeError> {
    Ok(coven::MemberId(member_id.parse().map_err(BridgeError::config)?))
}

/// A name the person typed in the app, refused as a configuration error when
/// it is blank.
pub(crate) fn name(name: &str) -> Result<bae_db::EnteredName, BridgeError> {
    bae_db::EnteredName::parse(name).map_err(BridgeError::config)
}
