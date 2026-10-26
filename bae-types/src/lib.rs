//! The domain values bae's catalogs, identification and library share, with
//! nothing in bae beneath them but bae-util: what a pressing is, how two
//! pieces of written text name one thing, and how a barcode is read.

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod barcode;
pub mod pressing;
pub mod text_match;
