//! The domain values bae's catalogs, identification, library and playback
//! share, with nothing in bae beneath them: what a pressing is, how two pieces
//! of written text name one thing, how a barcode is read, a time within a
//! track, and the storage moves a release can make.

#[cfg(desktop)]
pub mod barcode;
pub mod mediums;
pub mod pressing;
pub mod release_storage;
pub mod text_match;
pub mod track_time;
