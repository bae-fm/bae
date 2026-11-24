//! Pictures through libvips: a copy of a picture at one of a few sizes. The
//! original is read once, top to bottom, through plain reads that a job can
//! stop between, and a few rows of it are held in memory rather than the
//! whole picture.

mod copy;
#[cfg(libvips)]
mod ffi;
mod flights;
mod gate;
mod origin;
mod original;
#[cfg(libvips)]
mod source;
mod step;
mod types;
#[cfg(libvips)]
mod vips;

pub use copy::{make_copy, CopyFormat, PictureCopy};
pub use flights::{FlightError, Flights};
pub use gate::{Permit, Priority, Volumes};
pub use origin::PictureOrigin;
pub use original::read_original;
pub use step::PictureStep;
pub use types::{PictureSize, PictureSource, SourceFile};

/// Asks whether the job that holds it should stop: what a reader checks before
/// each read of the original.
pub type IsCancelled = std::sync::Arc<dyn Fn() -> bool + Send + Sync>;

/// Why a picture job failed.
#[derive(Debug, thiserror::Error)]
pub enum PicturesError {
    #[error("reading the picture failed: {0}")]
    Read(std::io::Error),
    #[error("libvips: {0}")]
    Vips(String),
    /// This build has no libvips: only macOS and Android link it.
    #[error("this build of bae makes no picture copies")]
    NoLibvips,
    /// Nobody wants the picture any more, and the job stopped between two
    /// reads.
    #[error("the picture's job was cancelled")]
    Cancelled,
}

#[cfg(test)]
mod flights_tests;
#[cfg(test)]
mod gate_tests;
#[cfg(all(test, libvips))]
mod tests;
