//! The catalogs bae asks about releases — MusicBrainz, Discogs, Wikidata —
//! and what their answers are made of: the catalogs bae knows and the pages
//! they publish, the covers they offer and the cache those images are read
//! through, and the rate every request to them is admitted at.
//!
//! The clients serve the import pipeline, so a mobile build compiles only
//! what a library on any device names: the catalogs, the covers and their
//! cache, and the admission a call is made with.

pub mod catalog;
pub mod rate_limiter;
pub mod remote_image;

#[cfg(desktop)]
pub mod discogs;
#[cfg(desktop)]
pub mod musicbrainz;
#[cfg(desktop)]
mod serde_helpers;
#[cfg(desktop)]
pub mod wikidata;
