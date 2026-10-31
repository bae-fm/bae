use crate::types::BridgeError;

/// Read a text file with automatic encoding detection. `path` is the file's
/// path as coven's bytes (`coven::path_bytes`).
#[uniffi::export]
pub fn read_text_file(path: Vec<u8>) -> Result<String, BridgeError> {
    let path = crate::path_bytes::path_from_app(path)?;
    let decoded = bae_util::text_encoding::read_text_file(&path)
        .map_err(|e| BridgeError::internal(format!("Failed to read file: {e}")))?;
    Ok(decoded.text)
}
