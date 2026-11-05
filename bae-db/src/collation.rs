//! The collation bae's schema names for folder paths.

/// The collation bae's schema names for folder paths in the order a person
/// reads them.
pub(crate) const NATURAL_ORDER: &str = "natural_order";

/// The version of [`bae_util::natural_order::compare`] coven records. A
/// change to what it compares bumps it, and coven rebuilds the indexes that
/// use it on the next writable open.
pub(crate) const NATURAL_ORDER_VERSION: u32 = 1;
