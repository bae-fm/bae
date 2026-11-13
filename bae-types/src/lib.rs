//! The domain values bae's catalogs, identification, library and playback
//! share, with nothing in bae beneath them: what a pressing is, how two pieces
//! of written text name one thing, how a barcode is read, an audio file's
//! format, a time within a track, the storage moves a release can make, and a
//! count the UI words from a catalog key.

pub mod audio_format;
#[cfg(desktop)]
pub mod barcode;
pub mod count_label;
pub mod credit_role;
pub mod credits;
pub mod edit_validation;
pub mod mediums;
pub mod pressing;
pub mod release_storage;
pub mod text_match;
pub mod track_time;
