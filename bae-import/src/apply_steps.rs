//! What an apply of a catalog release waits on over the network, told to the
//! surface that started it while it waits.

use std::sync::Arc;

use bae_catalogs::catalog::Catalog;

/// A network read an apply of a catalog release waits on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogApplyStep {
    /// Reading a release's documents from its catalog.
    ReadingRelease(Catalog),
    /// Fetching the cover the release is read with from the catalog that
    /// offers it.
    FetchingCover(Catalog),
}

/// Where an apply of a catalog release tells the step it waits on: the step
/// as it starts, then `None` as it ends. A release or cover the library
/// already holds is read without a step.
#[derive(Clone)]
pub struct CatalogApplySteps {
    report: Option<Arc<dyn Fn(Option<CatalogApplyStep>) + Send + Sync>>,
}

impl CatalogApplySteps {
    /// Steps told to `report`.
    pub fn new(report: impl Fn(Option<CatalogApplyStep>) + Send + Sync + 'static) -> Self {
        Self { report: Some(Arc::new(report)) }
    }

    /// Where nothing reads an apply's steps.
    pub fn unread() -> Self {
        Self { report: None }
    }

    /// `work`, told as `step` while it runs, however it ends.
    pub(crate) async fn during<T>(
        &self,
        step: CatalogApplyStep,
        work: impl std::future::Future<Output = T>,
    ) -> T {
        let _shown = Shown::start(self.report.as_deref(), step);
        work.await
    }
}

/// A step told as running until this is dropped.
struct Shown<'a> {
    report: Option<&'a (dyn Fn(Option<CatalogApplyStep>) + Send + Sync)>,
}

impl<'a> Shown<'a> {
    fn start(
        report: Option<&'a (dyn Fn(Option<CatalogApplyStep>) + Send + Sync)>,
        step: CatalogApplyStep,
    ) -> Self {
        if let Some(report) = report {
            report(Some(step));
        }
        Self { report }
    }
}

impl Drop for Shown<'_> {
    fn drop(&mut self) {
        if let Some(report) = self.report {
            report(None);
        }
    }
}
