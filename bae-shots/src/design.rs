//! The design files the boards render from: the design system's builds, a
//! board's frame, and the hash of everything a board's picture depends on.

use crate::capture::{CaptureError, Step};
use regex::Regex;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub(crate) const MOCKS: &str = "design/mocks";
pub(crate) const TOKENS: &str = "design/system/tokens.css";
pub(crate) const BUNDLE_CSS: &str = "design/system/components/bundle.css";
pub(crate) const BUNDLE_JS: &str = "design/system/components/bundle.js";

#[derive(Debug, thiserror::Error)]
pub(crate) enum DesignError {
    #[error(transparent)]
    Build(#[from] CaptureError),
    #[error("cannot read {path}: {reason}")]
    Unreadable { path: PathBuf, reason: String },
    #[error("{0} is not valid canvas.json: {1}")]
    BadCanvas(PathBuf, String),
    #[error("canvas.json has no board {0}")]
    NoFrame(String),
    #[error("board {board} imports {part}, but {file} does not exist")]
    MissingPart { board: String, part: String, file: PathBuf },
}

/// A board's frame on the canvas, in points.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub(crate) struct Frame {
    pub(crate) w: u32,
    pub(crate) h: u32,
}

#[derive(Deserialize)]
struct Canvas {
    boards: BTreeMap<String, Frame>,
}

fn read(path: &Path) -> Result<Vec<u8>, DesignError> {
    std::fs::read(path)
        .map_err(|e| DesignError::Unreadable { path: path.to_path_buf(), reason: e.to_string() })
}

/// Builds the design system's script, style sheet and token sheet, which the
/// boards load and git does not hold.
pub(crate) fn build(root: &Path) -> Result<(), DesignError> {
    Step::new("design system bundle", root, "python3", &["design/system/build-bundle.py"]).run()?;
    Step::new("design tokens", root, "python3", &["scripts/design/tokens-css.py"]).run()?;
    Ok(())
}

/// The file of the board `name`.
pub(crate) fn board_file(root: &Path, name: &str) -> PathBuf {
    root.join(MOCKS).join(format!("{name}.dc.html"))
}

/// The board's frame from `canvas.json`.
pub(crate) fn frame(root: &Path, name: &str) -> Result<Frame, DesignError> {
    let path = root.join(MOCKS).join("canvas.json");
    let canvas: Canvas = serde_json::from_slice(&read(&path)?)
        .map_err(|e| DesignError::BadCanvas(path, e.to_string()))?;
    canvas
        .boards
        .get(&format!("{name}.dc.html"))
        .copied()
        .ok_or_else(|| DesignError::NoFrame(name.to_string()))
}

/// The board's file and every part it mounts with `dc-import`, parts of parts
/// included, by name.
fn reached(root: &Path, name: &str) -> Result<BTreeMap<String, PathBuf>, DesignError> {
    let import = Regex::new(r#"<dc-import\s[^>]*?\bname="([^"]+)""#).unwrap();
    let mut files = BTreeMap::new();
    let mut pending = vec![(name.to_string(), name.to_string())];
    let mut seen = BTreeSet::new();
    while let Some((part, importer)) = pending.pop() {
        if !seen.insert(part.clone()) {
            continue;
        }
        let file = board_file(root, &part);
        if !file.is_file() {
            return Err(DesignError::MissingPart { board: importer, part, file });
        }
        let html = String::from_utf8_lossy(&read(&file)?).into_owned();
        for captures in import.captures_iter(&html) {
            pending.push((captures[1].to_string(), part.clone()));
        }
        files.insert(part, file);
    }
    Ok(files)
}

fn feed(hasher: &mut Sha256, label: &str, bytes: &[u8]) {
    hasher.update(label.as_bytes());
    hasher.update([0]);
    hasher.update((bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

/// What a board's picture depends on: its file and the parts it mounts, the
/// built token sheet, style sheet and script, the runtime copy and the
/// browser.
pub(crate) fn board_hash(
    root: &Path,
    name: &str,
    runtime_sha256: &str,
    browser_version: &str,
) -> Result<String, DesignError> {
    let mut hasher = Sha256::new();
    for (part, file) in reached(root, name)? {
        feed(&mut hasher, &format!("board {part}"), &read(&file)?);
    }
    for built in [TOKENS, BUNDLE_CSS, BUNDLE_JS] {
        feed(&mut hasher, built, &read(&root.join(built))?);
    }
    feed(&mut hasher, "runtime", runtime_sha256.as_bytes());
    feed(&mut hasher, "browser", browser_version.as_bytes());
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

#[cfg(test)]
#[path = "design_tests.rs"]
mod tests;
