//! Re-export of coven's library directory layout. The on-disk layout primitive
//! lives in coven now; this keeps bae's `crate::library_dir::…` call sites
//! resolving unchanged.
pub use coven::library_dir::*;
