//! Re-export of coven's clock. The clock primitive lives in coven now; this
//! keeps bae's `crate::clock::…` call sites resolving unchanged.
pub use coven::clock::*;
