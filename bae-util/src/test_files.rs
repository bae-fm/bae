//! The files a test works with: a folder of its own, the fixture files
//! committed to the repo, and the open-files budget a test that opens a file
//! takes its places from.
//!
//! bae reads and writes a library's files only through coven, and a test
//! follows the same rule: [`TestFolder`] and [`read_fixture`] are the only
//! test code that touches the file system. A `TestFolder` makes a fresh
//! temporary folder and writes, reads and deletes files inside it, and
//! `read_fixture` reads a file committed to the repo. Neither reaches any
//! other file.

use std::fs::{File, Metadata, OpenOptions, Permissions};
use std::path::{Component, Path, PathBuf};

use coven::{FileBudget, FileLease, LeasePlace};

/// Creates a test budget and leases all its places.
pub fn lease() -> FileLease {
    let budget = FileBudget::for_test();
    budget.lease_blocking(budget.size()).expect("a test budget holds its own size")
}

/// Takes one place of a new test budget on a separate thread. tokio limits
/// how much a task runs before it must yield, and a long test can reach that
/// limit, which would stall a wait on the runtime's thread.
pub fn place() -> LeasePlace {
    let lease = lease();
    std::thread::scope(|threads| {
        threads
            .spawn(|| lease.place_blocking())
            .join()
            .expect("the thread that takes a place panicked")
    })
}

/// The bytes of `path`, a file committed to the repo. Panics naming the file,
/// and how to get it back, when the checkout lacks it, and panics when `path`
/// is outside the repo.
pub fn read_fixture(path: impl AsRef<Path>) -> Vec<u8> {
    let path = path.as_ref();
    let repo = std::fs::canonicalize(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
        .expect("the repo's folder resolves");
    let file = std::fs::canonicalize(path).unwrap_or_else(|error| {
        panic!(
            "test fixture {} could not be found ({error}); it is committed, so \
             restore it with `git checkout -- {}`",
            path.display(),
            path.display()
        )
    });
    assert!(file.starts_with(&repo), "{} is not a file in the repo", path.display());
    std::fs::read(&file).unwrap_or_else(|error| {
        panic!("test fixture {} could not be read: {error}", path.display())
    })
}

/// A fresh folder in the system's temporary directory that a test owns,
/// removed with everything in it when the value drops.
///
/// Every method takes a path inside the folder: one relative to it, or a full
/// path that starts with [`TestFolder::path`], such as one the code under test
/// gave back. A path that leaves the folder, through `..` or by starting
/// elsewhere, panics, and so does every failure, naming the path.
#[derive(Debug)]
pub struct TestFolder {
    folder: tempfile::TempDir,
    /// The folder's path with its links resolved, which code that
    /// canonicalizes a path gives back: on macOS the temporary directory is
    /// under `/var`, a link to `/private/var`.
    resolved: PathBuf,
}

impl Default for TestFolder {
    fn default() -> Self {
        Self::new()
    }
}

impl TestFolder {
    /// A new, empty folder.
    pub fn new() -> Self {
        let folder = tempfile::Builder::new()
            .prefix("bae-test-")
            .tempdir()
            .expect("create a test's temporary folder");
        let resolved =
            std::fs::canonicalize(folder.path()).expect("the test's temporary folder resolves");
        Self { folder, resolved }
    }

    /// The folder itself, for the code under test to work in.
    pub fn path(&self) -> &Path {
        self.folder.path()
    }

    /// The full path of `path` in the folder. Panics when `path` leaves the
    /// folder.
    pub fn join(&self, path: impl AsRef<Path>) -> PathBuf {
        let path = path.as_ref();
        let relative = if path.is_absolute() {
            path.strip_prefix(self.folder.path())
                .or_else(|_| path.strip_prefix(&self.resolved))
                .unwrap_or_else(|_| {
                    panic!(
                        "{} is outside the test folder {}",
                        path.display(),
                        self.folder.path().display()
                    )
                })
        } else {
            path
        };
        assert!(
            relative
                .components()
                .all(|part| matches!(part, Component::Normal(_) | Component::CurDir)),
            "{} leaves the test folder",
            path.display()
        );
        self.folder.path().join(relative)
    }

    /// Write `contents` to the file at `path`, making the folders above it,
    /// and give back its full path.
    pub fn write(&self, path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> PathBuf {
        let path = self.join(path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .unwrap_or_else(|error| panic!("create {}: {error}", parent.display()));
        }
        std::fs::write(&path, contents)
            .unwrap_or_else(|error| panic!("write {}: {error}", path.display()));
        path
    }

    /// Copy the fixture file `fixture`, committed to the repo, to `path`,
    /// making the folders above it, and give back its full path.
    pub fn copy_fixture(&self, fixture: impl AsRef<Path>, path: impl AsRef<Path>) -> PathBuf {
        self.write(path, read_fixture(fixture))
    }

    /// Copy the file at `from` to `to`, making the folders above `to`, and
    /// give back its full path.
    pub fn copy(&self, from: impl AsRef<Path>, to: impl AsRef<Path>) -> PathBuf {
        let bytes = self.read(from);
        self.write(to, bytes)
    }

    /// Make the folder at `path` and every folder above it, and give back its
    /// full path.
    pub fn create_folder(&self, path: impl AsRef<Path>) -> PathBuf {
        let path = self.join(path);
        std::fs::create_dir_all(&path)
            .unwrap_or_else(|error| panic!("create {}: {error}", path.display()));
        path
    }

    /// The bytes of the file at `path`.
    pub fn read(&self, path: impl AsRef<Path>) -> Vec<u8> {
        let path = self.join(path);
        std::fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
    }

    /// The text of the file at `path`.
    pub fn read_text(&self, path: impl AsRef<Path>) -> String {
        let path = self.join(path);
        std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
    }

    /// Whether a file or a folder is at `path`.
    pub fn exists(&self, path: impl AsRef<Path>) -> bool {
        let path = self.join(path);
        path.try_exists().unwrap_or_else(|error| panic!("look for {}: {error}", path.display()))
    }

    /// The names of the files and folders in the folder at `path`, sorted.
    pub fn entries(&self, path: impl AsRef<Path>) -> Vec<String> {
        let path = self.join(path);
        let mut names: Vec<String> = std::fs::read_dir(&path)
            .unwrap_or_else(|error| panic!("list {}: {error}", path.display()))
            .map(|entry| {
                let entry =
                    entry.unwrap_or_else(|error| panic!("list {}: {error}", path.display()));
                entry.file_name().to_string_lossy().into_owned()
            })
            .collect();
        names.sort();
        names
    }

    /// The metadata of the file or folder at `path`, following a link.
    pub fn metadata(&self, path: impl AsRef<Path>) -> Metadata {
        let path = self.join(path);
        std::fs::metadata(&path).unwrap_or_else(|error| panic!("stat {}: {error}", path.display()))
    }

    /// Remove the file at `path`, or the folder at `path` with everything in
    /// it.
    pub fn remove(&self, path: impl AsRef<Path>) {
        let path = self.join(path);
        let removed = if self.metadata(&path).is_dir() {
            std::fs::remove_dir_all(&path)
        } else {
            std::fs::remove_file(&path)
        };
        removed.unwrap_or_else(|error| panic!("remove {}: {error}", path.display()));
    }

    /// Move the file or folder at `from` to `to`.
    pub fn rename(&self, from: impl AsRef<Path>, to: impl AsRef<Path>) {
        let from = self.join(from);
        let to = self.join(to);
        std::fs::rename(&from, &to)
            .unwrap_or_else(|error| panic!("move {} to {}: {error}", from.display(), to.display()));
    }

    /// Make `link` a second name of the file at `file`.
    pub fn hard_link(&self, file: impl AsRef<Path>, link: impl AsRef<Path>) {
        let file = self.join(file);
        let link = self.join(link);
        std::fs::hard_link(&file, &link).unwrap_or_else(|error| {
            panic!("link {} to {}: {error}", link.display(), file.display())
        });
    }

    /// The full path of `path` with every link in it resolved, as code that
    /// canonicalizes a path gives it back.
    pub fn canonicalize(&self, path: impl AsRef<Path>) -> PathBuf {
        let path = self.join(path);
        std::fs::canonicalize(&path)
            .unwrap_or_else(|error| panic!("resolve {}: {error}", path.display()))
    }

    /// Set the permissions of the file or folder at `path`.
    pub fn set_permissions(&self, path: impl AsRef<Path>, permissions: Permissions) {
        let path = self.join(path);
        std::fs::set_permissions(&path, permissions)
            .unwrap_or_else(|error| panic!("set the permissions of {}: {error}", path.display()));
    }

    /// Open the file at `path` for reading.
    pub fn open(&self, path: impl AsRef<Path>) -> File {
        let path = self.join(path);
        File::open(&path).unwrap_or_else(|error| panic!("open {}: {error}", path.display()))
    }

    /// Open the file at `path` for reading, giving back the open's error, for
    /// a test that checks what a failed open reports.
    pub fn try_open(&self, path: impl AsRef<Path>) -> std::io::Result<File> {
        File::open(self.join(path))
    }

    /// Open the file at `path` as `options` say.
    pub fn open_with(&self, path: impl AsRef<Path>, options: &OpenOptions) -> File {
        let path = self.join(path);
        options.open(&path).unwrap_or_else(|error| panic!("open {}: {error}", path.display()))
    }
}

#[cfg(test)]
#[path = "test_files_tests.rs"]
mod tests;
