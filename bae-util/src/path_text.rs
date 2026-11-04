//! The text that shows a path's own name.

use std::path::Path;

/// The last name of `path` as `coven::path_text` shows it, escaped where it
/// isn't UTF-8 (`Caf\\xE9`); the whole path's text for a path with no last
/// name, such as a root.
pub fn name_text(path: &Path) -> String {
    coven::path_text(path.file_name().map_or(path, Path::new))
}

#[cfg(test)]
#[path = "path_text_tests.rs"]
mod tests;
