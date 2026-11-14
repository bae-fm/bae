//! The clock and ids a test builds a library on.

use crate::Store;
use coven::{ClockRef, FileBudget, IdSourceRef};
use std::sync::Arc;

/// The clock, the ids it times and the budget of open files that one test
/// library runs on, made once as the app's composition makes them for a
/// process. The library's store is built from them and carries them, its
/// services open on the store's, and its installation is made with the same
/// budget, so a test cannot give them different ones.
#[derive(Clone)]
pub struct TestSources {
    clock: ClockRef,
    ids: IdSourceRef,
    files: FileBudget,
}

impl TestSources {
    /// The real clock and the app's time-ordered ids it times.
    pub fn system() -> Self {
        Self::on(&(Arc::new(coven::SystemClock) as ClockRef))
    }

    /// `clock` and the app's time-ordered ids it times.
    pub fn on(clock: &ClockRef) -> Self {
        Self::on_budget(clock, FileBudget::for_test())
    }

    /// [`Self::on`], opening files within `files`, a budget a test sizes and
    /// watches.
    pub fn on_budget(clock: &ClockRef, files: FileBudget) -> Self {
        let ids = Arc::new(bae_util::time_ordered_ids::TimeOrderedIds::new(clock.clone()));
        Self { clock: clock.clone(), ids, files }
    }

    /// An installation for a test, its stores under `app_dir`, whose bugs go
    /// to the store's test sink and whose places for open files are these
    /// sources'.
    pub fn installation(&self, app_dir: std::path::PathBuf) -> coven::Coven {
        coven::Coven::in_memory_sharing(
            app_dir,
            coven::task::HeardBugs::shared(),
            self.files.clone(),
        )
    }

    /// A builder over `installation` on these sources.
    pub fn builder(&self, installation: &coven::Coven) -> coven::CovenBuilder {
        crate::store_builder(installation, self.clock.clone(), self.ids.clone())
    }

    /// The store over `handle`, which a builder from these sources made. A
    /// bug one of its calls carries back fails the test as the store drops.
    pub fn store(&self, handle: coven::CovenHandle) -> Store {
        Store::new_on(handle, self.clone(), coven::task::HeardBugs::shared(), self.files.clone())
    }

    /// The clock.
    pub fn clock(&self) -> &ClockRef {
        &self.clock
    }

    /// The ids.
    pub fn ids(&self) -> &IdSourceRef {
        &self.ids
    }
}
