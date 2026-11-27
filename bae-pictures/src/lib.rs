//! Pictures through libvips: a copy of a picture at one of a few sizes, and
//! the tiles a viewer zooms it from. The original is read once, top to
//! bottom, through plain reads that a job can stop between, and a few rows of
//! it are held in memory rather than the whole picture.

mod copy;
#[cfg(libvips)]
mod ffi;
mod flights;
mod gate;
#[cfg(libvips)]
mod orientation;
mod origin;
#[cfg(libvips)]
mod source;
pub mod tile_cache;
pub mod tiles;
mod turn;
#[cfg(libvips)]
mod vips;

pub use bae_types::picture_step::PictureStep;
pub use bae_types::picture_turn::PictureTurn;
pub use copy::{make_copy, make_copy_of_bytes, CopyFormat, PictureCopy};
pub use flights::{FlightError, Flights};
pub use gate::{Permit, Volumes};
pub use origin::{HeldOrigin, PictureOrigin};
pub use turn::AskedTurn;

include!(concat!(env!("OUT_DIR"), "/schema.rs"));

#[cfg(all(test, libvips))]
const TEST_SCHEMA: bae_store::Schema = bae_store::Schema::new(&[SCHEMA_PARTS]);

/// Asks whether the job that holds it should stop: what a reader checks before
/// each read of the original.
pub type IsCancelled = std::sync::Arc<dyn Fn() -> bool + Send + Sync>;

/// Why a picture job failed.
#[derive(Debug, thiserror::Error)]
pub enum PicturesError {
    #[error("reading the picture failed: {0}")]
    Read(std::io::Error),
    #[error("writing the picture's tiles failed: {0}")]
    Write(std::io::Error),
    #[error("libvips: {0}")]
    Vips(String),
    /// This build has no libvips: every target but iOS links it.
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
