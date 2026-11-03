//! The detail views' live reads on [`AppServices`]: one read per view, its
//! id moved in place as the view shows another album, release, artist,
//! composer, work, or folder that failed import validation.

use super::*;
use bae_core::album_detail::{
    AlbumDetail, ArtistDetail, ComposerDetail, ReleaseDetail, WorkDetail,
};
use bae_core::library::{live_reads, LiveDetail};

impl AppServices {
    /// The detail view's album as it changes; see
    /// [`live_reads::subscribe_album_detail`].
    pub fn subscribe_album_detail(&self) -> LiveDetail<AlbumDetail> {
        live_reads::subscribe_album_detail(&self.inner.manager, &self.inner.views)
    }

    /// The detail view's release as it changes; see
    /// [`live_reads::subscribe_release_detail`].
    pub fn subscribe_release_detail(&self) -> LiveDetail<ReleaseDetail> {
        live_reads::subscribe_release_detail(&self.inner.manager, &self.inner.views)
    }

    /// The artist detail view's artist as it changes; see
    /// [`live_reads::subscribe_artist_detail`].
    pub fn subscribe_artist_detail(&self) -> LiveDetail<ArtistDetail> {
        live_reads::subscribe_artist_detail(&self.inner.manager, &self.inner.views)
    }

    /// The composer detail view's composer as it changes; see
    /// [`live_reads::subscribe_composer_detail`].
    pub fn subscribe_composer_detail(&self) -> LiveDetail<ComposerDetail> {
        live_reads::subscribe_composer_detail(&self.inner.manager, &self.inner.views)
    }

    /// The work detail view's work as it changes; see
    /// [`live_reads::subscribe_work_detail`].
    pub fn subscribe_work_detail(&self) -> LiveDetail<WorkDetail> {
        live_reads::subscribe_work_detail(&self.inner.manager, &self.inner.views)
    }

    /// The folder that failed validation the import pane looks into, as it
    /// changes; another folder is a new key on the same read.
    #[cfg(desktop)]
    pub fn subscribe_invalid_candidate(
        &self,
    ) -> LiveDetail<bae_import::InvalidCandidateDetail, bae_import::CandidateKey> {
        let import = self.inner.import.clone();
        live_reads::detail_read(
            self.inner.manager.clone(),
            move |key| import.subscribe_invalid_candidate(key),
            |detail| detail,
        )
    }
}
