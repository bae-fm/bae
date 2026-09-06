//! A watched folder as the store names it, and the rules every spelling of
//! a root or a candidate path is held to before it is stored or compared.

use std::path::{Component, Path};
use tracing::warn;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchedFolder {
    pub path: String,
    pub name: String,
}

impl WatchedFolder {
    pub(crate) fn from_path(path: String) -> Self {
        let name = match Path::new(&path).file_name().and_then(|name| name.to_str()) {
            Some(name) => name.to_string(),
            None => {
                warn!(
                    "watched folder {path:?} has no usable final path component; \
                     using the full path as its group name"
                );
                path.clone()
            }
        };
        Self { path, name }
    }
}

/// The one spelling of `path` this device stores for the folder it names.
///
/// A watched root is a durable key: it addresses rows in three tables and is
/// compared as a string. So there has to be exactly one spelling per folder,
/// and deciding it is this function's job rather than every caller's. The same
/// folder reaches core written several ways — a picker gives the host's own
/// form, a `file://` drop and a `bae://import` link give whatever the URL
/// carried (on Windows `C:/Music`, forward slashes and all), and a person
/// typing one adds a trailing separator as often as not.
///
/// Rejoining the path's [`Component`]s settles all of that: separators become
/// the host's, runs of them collapse, `.` and trailing separators disappear.
/// Two things are refused instead of rewritten:
///
/// - A `..`, because resolving it lexically is wrong the moment a symlink is
///   above it, and resolving it truthfully means reading the filesystem — a
///   different promise, and one a folder that is merely offline would fail.
/// - A path that is not absolute *by the host's rule*, which on Windows means
///   a drive or UNC prefix. `\music` is rooted but drive-relative: the same
///   text names a different folder depending on the process's current drive,
///   so nothing durable can be keyed by it.
pub(crate) fn canonical_absolute_root(path: &str) -> Result<String, crate::import::ImportError> {
    let refuse = |reason: &str| {
        Err(crate::import::ImportError::WatchedFolder {
            detail: format!("watched folder {reason}: {path}"),
        })
    };
    if Path::new(path)
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return refuse("must not contain `..`");
    }
    let canonical: std::path::PathBuf = Path::new(path).components().collect();
    if !canonical.is_absolute() {
        return refuse("must be an absolute path");
    }
    // Every component came from a `&str` and the separators joining them are
    // ASCII, so this is the same text, never a replacement character.
    Ok(canonical.to_string_lossy().into_owned())
}

/// A stored root is canonical by construction, so one that is not is corrupt
/// durable state — read it loudly rather than quietly rewriting it, which
/// would hide however it got written and leave its dependent rows keyed by the
/// spelling this device no longer uses.
pub(crate) fn validate_absolute_root(path: &str) -> Result<(), crate::import::ImportError> {
    let canonical = canonical_absolute_root(path)?;
    if canonical != path {
        return Err(crate::import::ImportError::WatchedFolder {
            detail: format!(
                "stored watched folder is not its canonical spelling {canonical}: {path}"
            ),
        });
    }
    Ok(())
}

pub(crate) fn validate_relative_path(path: &str) -> Result<(), crate::import::ImportError> {
    let normalized = Path::new(path)
        .components()
        .map(|component| match component {
            Component::Normal(value) => value.to_str().ok_or(()),
            _ => Err(()),
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|components| components.join("/"));
    if normalized.as_deref() != Ok(path) {
        return Err(crate::import::ImportError::WatchedFolder {
            detail: format!("candidate path must be normalized and root-relative: {path}"),
        });
    }
    Ok(())
}

pub(crate) fn candidate_relative_path(
    watched_folder_path: &str,
    candidate_path: &Path,
) -> Result<String, crate::import::ImportError> {
    let relative = candidate_path
        .strip_prefix(watched_folder_path)
        .map_err(|_| crate::import::ImportError::WatchedFolder {
            detail: format!(
                "{} is outside watched folder {watched_folder_path}",
                candidate_path.display()
            ),
        })?;
    let components: Result<Vec<_>, _> = relative
        .components()
        .map(|component| match component {
            Component::Normal(value) => value.to_str().map(str::to_string).ok_or_else(|| {
                crate::import::ImportError::WatchedFolder {
                    detail: format!(
                        "candidate path is not valid Unicode: {}",
                        candidate_path.display()
                    ),
                }
            }),
            _ => Err(crate::import::ImportError::WatchedFolder {
                detail: format!(
                    "candidate path is not normalized below its watched folder: {}",
                    candidate_path.display()
                ),
            }),
        })
        .collect();
    let relative = components?.join("/");
    validate_relative_path(&relative)?;
    Ok(relative)
}

pub(crate) fn paths_overlap(left: &Path, right: &Path) -> bool {
    left.starts_with(right) || right.starts_with(left)
}

/// Rewrite a `/`-spelled stand-in root in the running host's own spelling.
///
/// A watched root is stored exactly as the OS writes it, and what counts as
/// absolute is the OS's rule: Windows needs a drive or UNC prefix, so a
/// `/`-rooted literal is drive-relative there and [`canonical_absolute_root`]
/// refuses it. Tests that need a root no filesystem has to back ask for one
/// here rather than writing a literal that is only absolute on Unix.
///
/// Only the rooting changes. A path that is non-canonical for another reason —
/// a trailing separator, a doubled one, a `.` or `..` — stays non-canonical
/// after the rewrite, so the tests that check those forms are refused still
/// hand over a form this host refuses.
#[cfg(test)]
pub(crate) fn host_root(posix: &str) -> String {
    #[cfg(windows)]
    {
        format!("C:{}", posix.replace('/', "\\"))
    }
    #[cfg(not(windows))]
    {
        posix.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_path_uses_the_full_path_as_name() {
        let folder = WatchedFolder::from_path("/".to_string());
        assert_eq!(folder.name, "/");
    }

    /// A drive-lettered path written with forward slashes — what Windows hands
    /// over for `bae://import?path=C:/music/rips` and for a
    /// `file:///C:/music/rips` drop. It names `/music/rips` under [`host_root`]
    /// and has no counterpart on a host whose separator is already `/`.
    #[cfg(windows)]
    const URL_SPELLINGS: &[&str] = &["C:/music/rips"];
    #[cfg(not(windows))]
    const URL_SPELLINGS: &[&str] = &[];

    /// The spellings a folder picker, a `file://` drop, and a `bae://import`
    /// link each hand over for the same folder. Every one of them names a
    /// directory this device can address, so every one is accepted — and all
    /// of them settle on the single spelling the row is keyed by.
    #[test]
    fn a_root_has_one_stored_spelling_however_it_was_written() {
        let canonical = host_root("/music/rips");
        let spellings = [
            canonical.clone(),
            host_root("/music/rips/"),
            host_root("/music//rips"),
            host_root("/music/./rips"),
        ];

        for spelling in spellings
            .iter()
            .map(String::as_str)
            .chain(URL_SPELLINGS.iter().copied())
        {
            assert_eq!(
                canonical_absolute_root(spelling).unwrap(),
                canonical,
                "{spelling}"
            );
        }
    }

    /// Rooted but not absolute: on Windows a leading separator names the
    /// current drive, so the same text addresses a different folder depending
    /// on which drive the process happens to be on. Nothing durable can be
    /// keyed by it.
    #[cfg(windows)]
    #[test]
    fn a_drive_relative_root_is_refused() {
        for rooted in ["/music/rips", r"\music\rips"] {
            let error = canonical_absolute_root(rooted).unwrap_err();
            assert!(error.to_string().contains("absolute"), "{rooted}: {error}");
        }
    }

    /// A network share is a watched root like any other — it is what the
    /// folder-scan design's "network filesystem" case is about — and so is a
    /// verbatim path. Both keep their prefix; only what follows is rejoined.
    #[cfg(windows)]
    #[test]
    fn unc_and_verbatim_roots_keep_their_prefix() {
        for (written, stored) in [
            (r"\\storage\share\Music\", r"\\storage\share\Music"),
            (r"\\?\C:\Music", r"\\?\C:\Music"),
        ] {
            assert_eq!(
                canonical_absolute_root(written).unwrap(),
                stored,
                "{written}"
            );
        }
    }

    /// Whatever a root canonicalizes to is itself canonical, which is what
    /// lets the stored spelling be re-read by [`validate_absolute_root`]
    /// instead of canonicalized again on every load.
    #[test]
    fn canonicalizing_a_canonical_root_changes_nothing() {
        // A bare share root is in here because it is the one input whose
        // canonical form keeps a trailing separator (`\\storage\share\`, the
        // share's own root); re-reading it must still be a no-op.
        #[cfg(windows)]
        const SHARE_ROOTS: &[&str] = &[r"\\storage\share"];
        #[cfg(not(windows))]
        const SHARE_ROOTS: &[&str] = &[];

        let spellings = [
            host_root("/music/rips/"),
            host_root("/music//rips"),
            host_root("/music/./rips"),
        ];

        for written in spellings
            .iter()
            .map(String::as_str)
            .chain(URL_SPELLINGS.iter().copied())
            .chain(SHARE_ROOTS.iter().copied())
        {
            let stored = canonical_absolute_root(written).unwrap();
            assert_eq!(
                canonical_absolute_root(&stored).unwrap(),
                stored,
                "{written}"
            );
            validate_absolute_root(&stored).unwrap_or_else(|e| panic!("{written}: {e}"));
        }
    }

    /// `..` is the one lexical form that is not rewritten away: resolving it
    /// without touching the filesystem is wrong the moment a symlink is in the
    /// path, and resolving it against the filesystem is a different promise
    /// than this function makes.
    #[test]
    fn a_root_climbing_out_of_itself_is_refused() {
        let error = canonical_absolute_root(&host_root("/music/../rips")).unwrap_err();
        assert!(error.to_string().contains(".."), "{error}");
    }

    #[test]
    fn relative_paths_have_one_canonical_spelling() {
        for invalid in ["a//b", "a/./b", "a/../b", "/a"] {
            assert!(validate_relative_path(invalid).is_err(), "{invalid}");
        }
        assert!(validate_relative_path("").is_ok());
        assert!(validate_relative_path("a/b").is_ok());
    }
}
