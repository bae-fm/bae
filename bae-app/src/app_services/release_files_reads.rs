//! The Storage sheet's live read of a release's files on [`AppServices`].

use super::*;

impl AppServices {
    /// The files of the release the subscription is set to, as they change;
    /// see [`crate::live_reads::subscribe_release_files`].
    pub fn subscribe_release_files(&self) -> crate::live_reads::LiveReleaseFiles {
        crate::live_reads::subscribe_release_files(&self.inner.manager, &self.inner.views)
    }
}
