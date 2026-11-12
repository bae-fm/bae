//! The canvas runtime copy the boards render with, and the checks that it is
//! the runtime the canvas serves.
//!
//! `bae-shots/canvas/dc-runtime.js` is a copy of `artifact-type/dc-runtime.js`
//! of the design system's artifact, with `bae-shots/canvas/runtime.toml`
//! recording its sha256 and where it was read. The canvas's live hash is
//! recorded in `design/mocks/runtime.toml` by the design session at each canvas
//! publish; only a session with the person's login can read the canvas, so
//! this tool trusts that record. Rendering needs all three to agree.

use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

const COPY: &str = "bae-shots/canvas/dc-runtime.js";
const COPY_RECORD: &str = "bae-shots/canvas/runtime.toml";
const LIVE_RECORD: &str = "design/mocks/runtime.toml";

/// How to refresh the copy from the canvas at `canvas`.
fn refresh(canvas: &str) -> String {
    format!(
        "To refresh the copy: read artifact-type/dc-runtime.js of {canvas} with the \
Artifact tool (action read), save it as {COPY}, and set sha256, version and read_at in \
{COPY_RECORD} from that read; commit both files."
    )
}

/// `bae-shots/canvas/runtime.toml`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CopyRecord {
    sha256: String,
    version: String,
    /// The canvas the copy was read from.
    source: String,
    #[allow(dead_code)]
    read_at: String,
}

/// `design/mocks/runtime.toml`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LiveRecord {
    canvas: String,
    canvas_version: String,
    #[allow(dead_code)]
    type_release: String,
    #[allow(dead_code)]
    runtime: String,
    runtime_sha256: String,
    published_at: String,
    published_by: String,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub(crate) enum RuntimeError {
    #[error("cannot read {path}: {reason}")]
    Unreadable { path: PathBuf, reason: String },
    #[error("{path} is not a valid record: {reason}")]
    BadRecord { path: PathBuf, reason: String },
    #[error(
        "the live runtime record {path} is missing, so the copy cannot be checked against \
the canvas. The design session writes it at each canvas publish. {}", refresh(.canvas)
    )]
    LiveRecordMissing { path: PathBuf, canvas: String },
    #[error(
        "the runtime copy {COPY} hashes to {actual} but {COPY_RECORD} records {recorded} \
(version {version}); the copy was edited or the record is stale. {}", refresh(.canvas)
    )]
    CopyChanged { actual: String, recorded: String, version: String, canvas: String },
    #[error(
        "the runtime copy is version {copy_version} ({copy_sha256}) but the canvas was published with \
version {live_version} ({live_sha256}) at {published_at} by {published_by}. {}", refresh(.canvas)
    )]
    LiveChanged {
        copy_version: String,
        copy_sha256: String,
        live_version: String,
        live_sha256: String,
        published_at: String,
        published_by: String,
        canvas: String,
    },
}

/// A runtime copy that matched its record and the live record.
#[derive(Debug, PartialEq)]
pub(crate) struct Checked {
    /// The copy's sha256.
    pub sha256: String,
    pub version: String,
    /// When the live record says the canvas was published, for the report.
    pub live_published_at: String,
}

fn read(path: &Path) -> Result<Vec<u8>, RuntimeError> {
    std::fs::read(path)
        .map_err(|e| RuntimeError::Unreadable { path: path.to_path_buf(), reason: e.to_string() })
}

fn record<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, RuntimeError> {
    let text = String::from_utf8(read(path)?)
        .map_err(|e| RuntimeError::BadRecord { path: path.to_path_buf(), reason: e.to_string() })?;
    toml::from_str(&text)
        .map_err(|e| RuntimeError::BadRecord { path: path.to_path_buf(), reason: e.to_string() })
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// The path of the runtime copy under `root`.
pub(crate) fn copy_path(root: &Path) -> PathBuf {
    root.join(COPY)
}

/// Checks the copy against its own record and against the live record.
pub(crate) fn check(root: &Path) -> Result<Checked, RuntimeError> {
    let bytes = read(&copy_path(root))?;
    let recorded: CopyRecord = record(&root.join(COPY_RECORD))?;
    let actual = sha256(&bytes);
    if actual != recorded.sha256 {
        return Err(RuntimeError::CopyChanged {
            actual,
            recorded: recorded.sha256,
            version: recorded.version,
            canvas: recorded.source,
        });
    }
    let live_path = root.join(LIVE_RECORD);
    if !live_path.exists() {
        return Err(RuntimeError::LiveRecordMissing { path: live_path, canvas: recorded.source });
    }
    let live: LiveRecord = record(&live_path)?;
    if live.runtime_sha256 != actual {
        return Err(RuntimeError::LiveChanged {
            copy_version: recorded.version,
            copy_sha256: actual,
            live_version: live.canvas_version,
            live_sha256: live.runtime_sha256,
            published_at: live.published_at,
            published_by: live.published_by,
            canvas: live.canvas,
        });
    }
    Ok(Checked { sha256: actual, version: recorded.version, live_published_at: live.published_at })
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;
