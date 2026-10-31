//! The one policy for the path fragments bae stores and later joins onto a
//! local path.
//!
//! `release_files.original_filename` and `releases.source_folder_name` are synced
//! columns: another device wrote them, and this device joins them onto a
//! directory the user chose — the export copy-out (`<target>/<source_folder>/
//! <original_filename>`) and audio detachment (`<new_path>/<original_filename>`). coven
//! writes wherever the host's destination map points, so a fragment that escapes
//! that directory is an arbitrary local write on every device that pulls the row.
//!
//! A fragment is a **relative, `/`-separated path of ordinary components**. It is
//! validated where it enters the database and again at each join (so a row from
//! another device, or from an older build, is refused rather than followed).
//!
//! A backslash is the one character whose meaning depends on the device: a
//! separator on Windows, an ordinary character elsewhere. A file name that
//! isn't UTF-8 is stored as its escaped text (`Caf\xE9.flac`, from
//! `coven::path_text`), so a library can hold names with one. Linux and macOS
//! join such a name as the one component it is; Windows refuses it, so its
//! export or copy fails rather than writing into a folder the name never named.

use std::path::{Component, Path};

/// A stored fragment that is not a relative path of ordinary components. Carries
/// the release and column it came from, because the value is data another device
/// wrote and the reader needs to know which row to look at.
#[derive(Debug, thiserror::Error)]
#[error("invalid path fragment for release {release_id} {label} {value:?}: {reason}")]
pub struct PathFragmentError {
    pub release_id: String,
    pub label: String,
    pub value: String,
    pub reason: &'static str,
}

/// Accept a fragment only if joining it onto a directory stays inside that
/// directory and means the same thing on every device of the library.
///
/// The component rule is closed: every [`Component`] must be [`Component::Normal`].
/// That one condition subsumes an absolute path (`RootDir`), a Windows drive/UNC
/// prefix (`Prefix` — on Windows `Path::join` onto a drive-absolute path like
/// `C:/evil` *replaces* the base entirely), a `..` (`ParentDir`), and a `.`
/// (`CurDir`). Empty and NUL are rejected up front, and on Windows so is a
/// backslash: a separator there, where it would split the stored name into
/// folders it never named. Elsewhere a backslash is an ordinary character of
/// the component holding it, as in an escaped name that isn't UTF-8.
pub fn validate_path_fragment(
    release_id: &str,
    label: &str,
    value: &str,
) -> Result<(), PathFragmentError> {
    let reject = |reason: &'static str| {
        Err(PathFragmentError {
            release_id: release_id.to_string(),
            label: label.to_string(),
            value: value.to_string(),
            reason,
        })
    };
    if value.is_empty() {
        return reject("path is empty");
    }
    if value.contains('\0') {
        return reject("path contains a NUL byte");
    }
    if cfg!(windows) && value.contains('\\') {
        return reject("path contains a backslash, a separator on Windows");
    }
    if Path::new(value)
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return reject("path is absolute or contains a non-normal (.. / . / drive) component");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_normal_relative_fragments() {
        for ok in ["track.flac", "CD1/track.flac", "Disc 1/01 - Song.flac"] {
            validate_path_fragment("af63ef4c-8602-4cd5-82c0-3d334b916305", "l", ok)
                .unwrap_or_else(|e| panic!("{ok:?}: {e}"));
        }
    }

    #[test]
    fn rejects_empty_and_nul() {
        for bad in ["", "a\0b"] {
            validate_path_fragment("af63ef4c-8602-4cd5-82c0-3d334b916305", "l", bad)
                .expect_err(bad);
        }
    }

    /// A backslash is a separator on Windows, so a fragment holding one is
    /// refused there.
    #[cfg(windows)]
    #[test]
    fn rejects_a_backslash_on_windows() {
        for bad in [r"CD1\track.flac", r"C:\evil", r"..\evil", r"Caf\xE9.flac"] {
            validate_path_fragment("af63ef4c-8602-4cd5-82c0-3d334b916305", "l", bad)
                .expect_err(bad);
        }
    }

    /// Elsewhere a backslash is an ordinary character of its component, as
    /// in the escaped text of a name that isn't UTF-8, and joining the
    /// fragment stays inside the folder.
    #[cfg(not(windows))]
    #[test]
    fn accepts_a_backslash_as_a_character_elsewhere() {
        for ok in [r"Caf\xE9.flac", r"CD1/Caf\xE9.flac", r"..\evil", r"C:\evil"] {
            validate_path_fragment("af63ef4c-8602-4cd5-82c0-3d334b916305", "l", ok)
                .unwrap_or_else(|e| panic!("{ok:?}: {e}"));
            let joined = Path::new("/target").join(ok);
            assert!(joined.starts_with("/target"), "{ok:?} joins to {joined:?}");
            assert_eq!(
                joined.components().count(),
                Path::new("/target").components().count() + Path::new(ok).components().count()
            );
        }
    }

    #[test]
    fn rejects_absolute_and_traversal() {
        // Non-`Normal` components: an absolute `RootDir`, a `..` `ParentDir`
        // (leading or interior), and a leading `.` `CurDir`.
        for bad in ["/etc/passwd", "..", "../evil", "a/../etc/passwd", "./evil"] {
            validate_path_fragment("af63ef4c-8602-4cd5-82c0-3d334b916305", "l", bad)
                .expect_err(bad);
        }
    }

    // On Windows a drive-absolute (`C:/evil`) or UNC (`\\server\share`) value
    // parses to a `Prefix`/`RootDir` component, and `Path::join` onto it would
    // discard the base directory entirely; the all-`Normal` rule rejects it.
    // On Unix `C:` is just an ordinary directory name, so this case is
    // Windows-specific and cannot run on the macOS/Linux CI host.
    #[cfg(windows)]
    #[test]
    fn rejects_windows_drive_and_unc() {
        for bad in ["C:/evil", r"C:\evil", r"\\server\share\evil"] {
            validate_path_fragment("af63ef4c-8602-4cd5-82c0-3d334b916305", "l", bad)
                .expect_err(bad);
        }
    }
}
