//! How `AppHandle` runs the work behind a fast-value stream: as one of the
//! library's tasks, so closing the library ends it before the store closes.

use super::*;

impl AppHandle {
    /// Run `body`, a feed writing a stream, as the library's own work until
    /// `stop` is cancelled or the library closes. `body` is lent the
    /// services to start what it follows, and is built on the runtime rather
    /// than on the caller. A panic in it, a bug, ends that one feed and is
    /// reported; the library's other work goes on.
    pub(super) fn deliver_until<Fut>(
        &self,
        stop: bae_library::CancellationToken,
        body: impl FnOnce(&AppServices) -> Fut + Send + 'static,
    ) where
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let services = self.services.clone();
        let diagnostics = self.diagnostics.clone();
        let delivery = async move {
            let feeding = async { body(&services).await };
            if let Err(failed) = bae_util::task_failure::caught("feed a stream", feeding).await {
                diagnostics.panicked(failed);
            }
        };
        // A host feeds a stream only while the library is open.
        if let Err(error) = self.services.spawn_library_task("stream feed", stop, delivery) {
            self.services.report_bug("start a stream feed", &error);
        }
    }
}

#[cfg(test)]
#[path = "live_tests.rs"]
mod tests;
