//! The import pane's live read of one candidate, which tells the
//! identification queue which candidate the pane shows.

use bae_import::identification::ShownCandidate;
use bae_import::{CandidateKey, ImportCandidateDetail};
use bae_library::LiveDetail;
use bae_store::live_query::LiveQueryError;

/// The import pane's candidate read live, by key. While it is set to a
/// candidate, the identification queue starts that candidate's job ahead of
/// every other; set to another, or ended, it no longer does.
pub struct LiveImportCandidate {
    read: LiveDetail<ImportCandidateDetail, CandidateKey>,
    /// Gone once the read is cancelled, which says the pane shows nothing.
    shown: std::sync::Mutex<Option<ShownCandidate>>,
}

impl LiveImportCandidate {
    pub(crate) fn new(
        read: LiveDetail<ImportCandidateDetail, CandidateKey>,
        shown: ShownCandidate,
    ) -> Self {
        Self { read, shown: std::sync::Mutex::new(Some(shown)) }
    }

    /// Read `key` from now on, telling the queue before any value for it is
    /// delivered.
    pub fn set(&self, key: Option<CandidateKey>) -> Result<(), LiveQueryError> {
        if let Some(shown) =
            self.shown.lock().expect("import candidate shown mutex poisoned").as_mut()
        {
            shown.show(key.clone());
        }
        self.read.set(key)
    }

    /// The next value for the key last set, or the end of the read.
    pub async fn next(
        &self,
    ) -> Result<bae_library::DetailSnapshot<ImportCandidateDetail, CandidateKey>, LiveQueryError>
    {
        self.read.next().await
    }

    /// Tell the queue the pane shows nothing, then stop reading as
    /// [`LiveRead::cancel`](bae_store::live_query::LiveRead::cancel) does.
    pub async fn cancel(&self) {
        self.shown.lock().expect("import candidate shown mutex poisoned").take();
        self.read.cancel().await;
    }
}
