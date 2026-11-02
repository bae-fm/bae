//! Provider images as import fetches them.

use bae_catalogs::remote_image::{RemoteImage, RemoteImageCache, RemoteImageError};

/// The app's one cache of provider images, and where a cache task that
/// panicked is reported: the cache cannot report its own.
#[derive(Clone)]
pub(crate) struct RemoteImages {
    cache: RemoteImageCache,
    diagnostics: bae_diagnostics::Diagnostics,
}

impl RemoteImages {
    pub(crate) fn new(cache: RemoteImageCache, diagnostics: bae_diagnostics::Diagnostics) -> Self {
        Self { cache, diagnostics }
    }

    /// The image at `url`, or `None` when the host serves no image there.
    pub(crate) async fn fetch(&self, url: &str) -> Result<Option<RemoteImage>, RemoteImageError> {
        self.cache.fetch(url).await.map_err(|error| self.reported(error))
    }

    /// The image at `url`, which must be served.
    pub(crate) async fn fetch_required(&self, url: &str) -> Result<RemoteImage, RemoteImageError> {
        self.cache.fetch_required(url).await.map_err(|error| self.reported(error))
    }

    fn reported(&self, error: RemoteImageError) -> RemoteImageError {
        error.reporting_cache_task(|failed| self.diagnostics.bug(failed))
    }
}
