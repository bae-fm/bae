//! Local blob storage helpers. coven owns the blob store, the content-addressed
//! path layout, the cloud key derivation, and the locality-aware read; bae keeps
//! the pin/unpin transfer queue on top.
pub mod transfer;
