use crate::types::BridgeError;

/// Read a text file with automatic encoding detection.
#[uniffi::export]
pub fn read_text_file(path: String) -> Result<String, BridgeError> {
    let decoded = bae_core::text_encoding::read_text_file(std::path::Path::new(&path))
        .map_err(|e| BridgeError::internal(format!("Failed to read file: {e}")))?;
    Ok(decoded.text)
}

/// Format the time label for a given seek ratio and track duration.
#[uniffi::export]
pub fn format_time_at_ratio(ratio: f64, duration_ms: u64) -> String {
    bae_core::playback::format::format_time_at_ratio(ratio, duration_ms)
}

/// Format the remaining time label for a given seek ratio and track duration.
/// Used when the UI is in "remaining time" mode so the scrub preview matches
/// the current display mode.
#[uniffi::export]
pub fn format_remaining_at_ratio(ratio: f64, duration_ms: u64) -> String {
    bae_core::playback::format::format_remaining_at_ratio(ratio, duration_ms)
}
