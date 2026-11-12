//! The images catalogs serve: the covers they offer a release, the image sets
//! behind each cover with its downscaled copies, and the bounded cache every
//! surface reads those images through.

use bae_util::task_failure::TaskFailed;

#[cfg(desktop)]
mod archive;
mod cache;
mod covers;
mod fetch;

#[cfg(desktop)]
pub use archive::{musicbrainz_gallery, musicbrainz_group_gallery};
pub use cache::{RemoteImage, RemoteImageCache, RemoteImageWatch};
#[cfg(desktop)]
pub use covers::{musicbrainz_album_cover, musicbrainz_release_cover, push_unique_cover};
pub use covers::{
    offered_covers, preferred_cover, CoverStanding, DownscaledCopy, RemoteCover, RemoteImageSet,
    ARCHIVE,
};

/// Why a catalog's image or an archive gallery could not be read.
#[derive(Debug, thiserror::Error)]
pub enum RemoteImageError {
    /// What the host served is not a usable answer: bytes that are not an
    /// image bae can decode, a body over its size cap, a gallery document
    /// that does not parse.
    #[error("cover art failed: {detail}")]
    Unusable { detail: String },

    /// An artwork request got no answer: the connection, DNS, TLS, a timeout
    /// or a dropped body failed — or bae could not make the request at all (a
    /// URL reqwest would not build, a redirect loop), which whoever classifies
    /// the failure tells apart by reading the error. `context` names what was
    /// being fetched.
    #[error("{context}: {}", bae_util::error_text::CauseChain(error))]
    Transport {
        context: &'static str,
        #[source]
        error: reqwest::Error,
    },

    /// An artwork host answered with a status that is not an image: a server
    /// error or a refusal.
    #[error("cover art request failed: {detail}")]
    Status { status: u16, detail: String },

    /// The host serves no image at `url`, where the caller needs one: the
    /// person picked an address nothing had said holds an image.
    #[error("no image is served at {url}")]
    NoImage { url: String },

    /// The blocking task that reads or writes the disk cache did not report
    /// back: it panicked, a bug the cache has already reported, or the
    /// runtime is shutting down under it.
    #[error("{0}")]
    CacheTask(TaskFailed),
}

#[cfg(test)]
#[path = "remote_image_tests.rs"]
mod tests;
