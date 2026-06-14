//! Re-export of coven's id provider. The id primitive lives in coven now; this
//! keeps bae's `crate::id_provider::…` call sites resolving unchanged.
pub use coven::id_provider::*;
