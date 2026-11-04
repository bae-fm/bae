//! Work run apart from its caller — on the blocking pool, as its own task, or
//! as one message of a long-lived loop — whose panic comes back to that
//! caller as a value instead of an unwind: coven's, which bae and coven share.
//!
//! A panic turned into an `Err` at the nearest join lets the caller's own
//! failure path run: the rollback, the cleanup and the "this import failed"
//! record bae writes on `Err`, which an unwind would skip. The caller reports
//! the panic as a bug (bae-library's `Diagnostics::bug`); this crate has no bug
//! reporter of its own.

pub use coven::task::{blocking, caught, caught_sync, joined, panic_message, Bug, TaskFailed};
