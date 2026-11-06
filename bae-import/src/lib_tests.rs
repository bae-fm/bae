//! The crate's own test tools, shared by tests across its modules: the
//! checked-in fixture files, files a test makes misbehave, a gate a test holds
//! a worker behind, the import service a unit test starts, and how many times
//! this test binary has read each file.

use std::path::{Path, PathBuf};

pub(crate) mod files {
    //! Files a test makes misbehave.

    use std::path::Path;

    /// Keeps `path` from opening until dropped, without touching its size or
    /// modification time: no permission to read it on Unix, an exclusive handle
    /// on Windows.
    pub(crate) struct UnopenableFile {
        #[cfg(unix)]
        path: std::path::PathBuf,
        #[cfg(windows)]
        _exclusive: std::fs::File,
    }

    impl UnopenableFile {
        pub(crate) fn block(path: &Path) -> Self {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o000)).unwrap();
                Self { path: path.to_path_buf() }
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::OpenOptionsExt;
                let exclusive =
                    std::fs::OpenOptions::new().read(true).share_mode(0).open(path).unwrap();
                Self { _exclusive: exclusive }
            }
        }
    }

    #[cfg(unix)]
    impl Drop for UnopenableFile {
        fn drop(&mut self) {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&self.path, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
    }
}

pub(crate) mod gate {
    //! A gate a test holds a worker thread behind, so it can act while that work
    //! is in flight.

    use std::sync::mpsc::{Receiver, Sender};
    use std::sync::{Arc, Condvar, Mutex};

    type Opened = Arc<(Mutex<bool>, Condvar)>;

    /// Holds every [`Held::pass`] until opened. Dropping it opens it, so a test
    /// that fails first never leaves a worker blocked.
    pub(crate) struct Gate(Opened);

    /// The worker's side of a [`Gate`].
    pub(crate) struct Held {
        entered: Sender<()>,
        opened: Opened,
    }

    /// A closed gate, its worker side, and a report of each pass entering.
    pub(crate) fn closed() -> (Gate, Held, Receiver<()>) {
        let opened = Opened::default();
        let (entered, entries) = std::sync::mpsc::channel();
        (Gate(opened.clone()), Held { entered, opened }, entries)
    }

    impl Gate {
        pub(crate) fn open(&self) {
            let (open, changed) = &*self.0;
            *open.lock().unwrap() = true;
            changed.notify_all();
        }
    }

    impl Drop for Gate {
        fn drop(&mut self) {
            self.open();
        }
    }

    impl Held {
        /// Report entering, then wait until the gate is open.
        pub(crate) fn pass(&self) {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "a test that stopped watching for entry needs no report"
            )]
            let _ = self.entered.send(());
            let (open, changed) = &*self.opened;
            let _open = changed.wait_while(open.lock().unwrap(), |open| !*open).unwrap();
        }
    }
}

#[cfg(desktop)]
pub(crate) mod import {
    //! The import service a unit test starts over a bare library store, from the
    //! parts the app's composition root would hand it: the catalog sessions, the
    //! remote image cache, the clock, ids and diagnostics, and the token that
    //! ends it with the library.

    use crate::{CatalogSessions, ImportServiceHandle};
    use bae_catalogs::remote_image::RemoteImageCache;
    use bae_diagnostics::Diagnostics;
    use bae_store::Store;
    use coven::{ClockRef, IdSourceRef};
    use tokio_util::sync::CancellationToken;

    /// What an import service over one test store is started from. Every service
    /// started from it shares its catalog sessions, so a Discogs key one stores
    /// is the next one's too.
    pub(crate) struct TestImport {
        db: Store,
        catalogs: CatalogSessions,
        remote_images: RemoteImageCache,
        clock: ClockRef,
        ids: IdSourceRef,
        diagnostics: Diagnostics,
        closing: CancellationToken,
    }

    impl TestImport {
        /// Over `database` on `clock` and `ids`, asking `providers`, which were
        /// built on `http`, reporting to `diagnostics`.
        pub(crate) fn new(
            db: Store,
            providers: bae_catalogs::providers::Providers,
            http: bae_util::http::Http,
            clock: ClockRef,
            ids: IdSourceRef,
            diagnostics: Diagnostics,
        ) -> Self {
            let remote_images = RemoteImageCache::for_test(http, diagnostics.clone());
            let catalogs = CatalogSessions::new(
                providers,
                db.clone(),
                bae_preferences::PreferencesRow::new(db.clone()),
                diagnostics.clone(),
                tokio::runtime::Handle::current(),
            );
            Self {
                db,
                catalogs,
                remote_images,
                clock,
                ids,
                diagnostics,
                closing: CancellationToken::new(),
            }
        }

        /// The catalog sessions the services ask through.
        pub(crate) fn catalogs(&self) -> &CatalogSessions {
            &self.catalogs
        }

        /// Start an import service, as opening the library does.
        pub(crate) async fn start(&self) -> Result<ImportServiceHandle, crate::ImportError> {
            crate::ImportService::start(
                tokio::runtime::Handle::current(),
                self.db.clone(),
                bae_records::test_fixtures::test_records(&self.db),
                self.catalogs.clone(),
                self.remote_images.clone(),
                self.diagnostics.clone(),
                crate::tables::ImportTables::new(self.db.clone()),
                self.clock.clone(),
                self.ids.clone(),
                self.closing.child_token(),
            )
            .await
        }

        /// [`Self::start`] reading folders' tags through `file_tags`.
        pub(crate) async fn start_reading_tags_with(
            &self,
            file_tags: std::sync::Arc<dyn crate::file_tag_reader::FileTagReader>,
        ) -> Result<ImportServiceHandle, crate::ImportError> {
            crate::ImportService::start_reading_tags_through(
                tokio::runtime::Handle::current(),
                self.db.clone(),
                bae_records::test_fixtures::test_records(&self.db),
                self.catalogs.clone(),
                self.remote_images.clone(),
                self.diagnostics.clone(),
                crate::tables::ImportTables::new(self.db.clone()),
                self.clock.clone(),
                self.ids.clone(),
                self.closing.child_token(),
                file_tags,
            )
            .await
        }

        /// Edit this library's preferences in one write of their row, as the
        /// settings screens do.
        pub(crate) async fn edit_preferences(
            &self,
            edit: impl FnOnce(&mut bae_preferences::Preferences) + Send + 'static,
        ) {
            bae_preferences::PreferencesRow::new(self.db.clone())
                .edit(move |prefs| {
                    edit(prefs);
                    Ok(())
                })
                .await
                .expect("the test's preferences are stored");
        }
    }
}

pub(crate) mod reads {
    //! How many times this test binary has read each file, by what read it, so a
    //! test can assert that asking about a folder again reads nothing off disk.

    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::sync::{Mutex, OnceLock};

    /// What read a file. A probe's opens are counted where the probe runs, by
    /// `bae_audio::test_reads`.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub(crate) enum FileRead {
        /// Its header was checked for the image its name says.
        ImageCheck,
        /// It was parsed as a CUE sheet.
        CueParse,
    }

    fn counts() -> &'static Mutex<HashMap<(FileRead, PathBuf), u64>> {
        static COUNTS: OnceLock<Mutex<HashMap<(FileRead, PathBuf), u64>>> = OnceLock::new();
        COUNTS.get_or_init(Default::default)
    }

    /// Count one `read` of `path`.
    pub(crate) fn record(read: FileRead, path: &Path) {
        *counts()
            .lock()
            .expect("file read counts mutex poisoned")
            .entry((read, path.to_path_buf()))
            .or_default() += 1;
    }

    /// How many times `read` has read `path`.
    pub(crate) fn count(read: FileRead, path: &Path) -> u64 {
        counts()
            .lock()
            .expect("file read counts mutex poisoned")
            .get(&(read, path.to_path_buf()))
            .copied()
            .unwrap_or(0)
    }
}

/// Resolve a checked-in test fixture by path relative to the crate root, so
/// tests don't depend on the process working directory.
pub(crate) fn fixture(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(rel)
}
