//! The clock and ids a test builds a library on.

use crate::Store;
use coven::{ClockRef, IdSourceRef};
use std::sync::Arc;

/// The clock and the ids it times that one test library runs on, made once
/// as the app's composition makes them for a process. The library's store is
/// built from them and carries them, and its services open on the store's, so
/// a test cannot give the two different ones.
#[derive(Clone)]
pub struct TestSources {
    clock: ClockRef,
    ids: IdSourceRef,
}

impl TestSources {
    /// The real clock and the app's time-ordered ids it times.
    pub fn system() -> Self {
        Self::on(&(Arc::new(coven::SystemClock) as ClockRef))
    }

    /// `clock` and the app's time-ordered ids it times.
    pub fn on(clock: &ClockRef) -> Self {
        let ids = Arc::new(bae_util::time_ordered_ids::TimeOrderedIds::new(clock.clone()));
        Self { clock: clock.clone(), ids }
    }

    /// A builder over `installation` on these sources.
    pub fn builder(&self, installation: &coven::Coven) -> coven::CovenBuilder {
        crate::store_builder(installation, self.clock.clone(), self.ids.clone())
    }

    /// The store over `handle`, which a builder from these sources made. A
    /// bug one of its calls carries back fails the test as the store drops.
    pub fn store(&self, handle: coven::CovenHandle) -> Store {
        Store::new_on(handle, self.clone(), coven::task::HeardBugs::shared())
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
