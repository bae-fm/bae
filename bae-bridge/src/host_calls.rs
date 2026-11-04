//! Calls into the host through a uniffi callback interface.
//!
//! A host implementation that throws — any exception a Kotlin method raises,
//! or a failure in uniffi's generated Swift glue — reaches Rust as a panic on
//! the calling thread: uniffi panics with "Callback interface failure" for a
//! method that returns no `Result`, and for one whose error type has no
//! conversion from an unexpected callback error. A host call runs under
//! [`host_call`] or [`host_call_async`], which catch that panic and report it
//! as the host's bug; the caller answers with its own failure value, so a
//! host throw never unwinds through core.

use bae_util::task_failure::{caught_sync, Bug};

/// Make the host call `call`. A throw comes back as the failure of
/// `operation`, handed to `bug` to report first.
pub(crate) fn host_call<T>(
    bug: impl FnOnce(Bug) -> Bug,
    operation: &'static str,
    call: impl FnOnce() -> T,
) -> Result<T, Bug> {
    caught_sync(operation, call).map_err(bug)
}

/// [`host_call`] for a host method that answers later: the native sign-in
/// sheet, which only a phone's host presents.
#[cfg(any(test, all(feature = "oauth-providers", not(desktop))))]
pub(crate) async fn host_call_async<T>(
    bug: impl FnOnce(Bug) -> Bug,
    operation: &'static str,
    call: impl std::future::Future<Output = T>,
) -> Result<T, Bug> {
    bae_util::task_failure::caught(operation, call).await.map_err(bug)
}
