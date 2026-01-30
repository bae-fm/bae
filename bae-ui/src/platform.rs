//! Platform-specific constants and helpers

/// Returns the platform-idiomatic phrase for revealing a file in the native file manager.
///
/// - macOS: "Reveal in Finder"
/// - Windows: "Show in Explorer"
/// - Other: "Show in file manager"
pub fn reveal_in_file_manager() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "Reveal in Finder"
    }
    #[cfg(target_os = "windows")]
    {
        "Show in Explorer"
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        "Show in file manager"
    }
}
