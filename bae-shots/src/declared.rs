//! The shots the harnesses declare, read from their source: Kotlin
//! `shot("…")` and `component("…")` first arguments under
//! `bae-android/app/src/test/java/fm/bae/app/shots/`, and Swift string
//! literals of board form under `bae-macos/bae/baeShots/`.

use crate::names::{Name, NameError};
use regex::Regex;
use std::path::{Path, PathBuf};

const KOTLIN_DIRECTORY: &str = "bae-android/app/src/test/java/fm/bae/app/shots";
const SWIFT_DIRECTORY: &str = "bae-macos/bae/baeShots";

#[derive(Debug, thiserror::Error)]
pub(crate) enum DeclaredError {
    #[error("{0}: {1}")]
    Name(PathBuf, NameError),
    #[error("cannot read {0}: {1}")]
    Read(PathBuf, std::io::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Declared {
    pub(crate) name: Name,
    pub(crate) file: PathBuf,
}

/// Every shot both harnesses declare, in name order, each once.
pub(crate) fn read(root: &Path) -> Result<Vec<Declared>, DeclaredError> {
    let mut all = Vec::new();
    for (directory, extension, find) in [
        (KOTLIN_DIRECTORY, "kt", kotlin as fn(&str) -> Vec<String>),
        (SWIFT_DIRECTORY, "swift", swift),
    ] {
        let directory = root.join(directory);
        let entries =
            std::fs::read_dir(&directory).map_err(|e| DeclaredError::Read(directory.clone(), e))?;
        for entry in entries {
            let path = entry.map_err(|e| DeclaredError::Read(directory.clone(), e))?.path();
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            // A test of the harness writes its own boards; it declares none.
            if path.extension().and_then(|e| e.to_str()) != Some(extension)
                || stem.ends_with("Test")
                || stem.ends_with("Tests")
            {
                continue;
            }
            let source =
                std::fs::read_to_string(&path).map_err(|e| DeclaredError::Read(path.clone(), e))?;
            for text in find(&source) {
                let name = Name::parse(&text).map_err(|e| DeclaredError::Name(path.clone(), e))?;
                all.push(Declared { name, file: path.clone() });
            }
        }
    }
    all.sort_by(|a, b| a.name.cmp(&b.name).then(a.file.cmp(&b.file)));
    all.dedup_by(|a, b| a.name == b.name);
    Ok(all)
}

/// The first string argument of each `shot(` and `component(` call.
fn kotlin(source: &str) -> Vec<String> {
    let call = Regex::new(r#"\b(?:shot|component)\(\s*"([^"]*)""#).unwrap();
    call.captures_iter(source).map(|c| c[1].to_string()).collect()
}

/// Each string literal that begins like a Mac board name.
fn swift(source: &str) -> Vec<String> {
    let literal = Regex::new(r#""(mac\.[^"\s]*)""#).unwrap();
    literal.captures_iter(source).map(|c| c[1].to_string()).collect()
}

#[cfg(test)]
#[path = "declared_tests.rs"]
mod tests;
