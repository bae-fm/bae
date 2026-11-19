//! The shots' names and declarations: board names, the shots the harnesses
//! declare, the record of the pairs confirmed against their boards, and the
//! shots and record entries that have no board. The shots lint needs only
//! these, so the commit hook compiles them without bae-shots' browser and
//! image code. bae-shots depends on this crate.

pub mod declared;
pub mod lint;
pub mod names;
pub mod record;

use std::path::{Path, PathBuf};

/// The checkout the command runs in: the nearest directory above the working
/// directory that holds the design system's components.
#[expect(
    clippy::disallowed_methods,
    reason = "file system: a dev tool that reads and writes the repo's files, never a library's"
)]
pub fn repository_root() -> Result<PathBuf, String> {
    let start = std::env::current_dir().map_err(|e| e.to_string())?;
    start
        .ancestors()
        .find(|d| d.join("design/system/components").is_dir())
        .map(Path::to_path_buf)
        .ok_or_else(|| format!("no design/system/components above {}", start.display()))
}
