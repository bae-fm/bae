use crate::types::BridgeError;

/// Read a text file with automatic encoding detection.
#[uniffi::export]
pub fn read_text_file(path: String) -> Result<String, BridgeError> {
    let decoded = bae_core::text_encoding::read_text_file(std::path::Path::new(&path))
        .map_err(|e| BridgeError::internal(format!("Failed to read file: {e}")))?;
    Ok(decoded.text)
}
