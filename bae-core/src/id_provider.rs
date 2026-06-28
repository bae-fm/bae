//! Re-export of coven's id provider. The id primitive lives in coven now; this
//! keeps bae's `crate::id_provider::…` call sites resolving unchanged.
pub use coven::{IdProvider, IdRef, UuidProvider};
#[cfg(any(test, feature = "test-utils"))]
pub use coven::SequentialIdProvider;
