//! Re-export of coven's clock. The clock primitive lives in coven now; this
//! keeps bae's `crate::clock::…` call sites resolving unchanged.
pub use coven::{Clock, ClockRef, SystemClock};
#[cfg(any(test, feature = "test-utils"))]
pub use coven::FixedClock;
