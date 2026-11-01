//! The closure loop behind `AppHandle`'s remaining callback `subscribe_*`
//! calls, wrapped in the `LiveSubscription` the host cancels.

use super::*;

impl AppHandle {
    /// Run a subscription's delivery loop as the library's own work and hand
    /// back the object the host cancels it with. Closing the library ends the
    /// loop before its store closes, as it ends the library's other tasks.
    /// `body` receives the services, and is built on the runtime rather than
    /// on the caller.
    pub(super) fn live_subscription<Fut>(
        &self,
        body: impl FnOnce(AppServices) -> Fut + Send + 'static,
    ) -> std::sync::Arc<crate::LiveSubscription>
    where
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let stop = bae_core::library::CancellationToken::new();
        self.deliver_until(stop.clone(), body);
        std::sync::Arc::new(crate::LiveSubscription::new(stop))
    }

    /// Run `body` as the library's own work until `stop` is cancelled or the
    /// library closes. A panic in it — the host's callback throwing, which
    /// uniffi raises as a panic, or the bridge's own bug — ends that one
    /// delivery and is reported; the library's other work goes on.
    pub(super) fn deliver_until<Fut>(
        &self,
        stop: bae_core::library::CancellationToken,
        body: impl FnOnce(AppServices) -> Fut + Send + 'static,
    ) where
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let services = self.services.clone();
        let diagnostics = self.diagnostics.clone();
        let delivery = async move {
            if let Err(failed) =
                bae_util::task_failure::caught("deliver a live subscription", body(services)).await
            {
                diagnostics.bug(failed);
            }
        };
        // A host subscribes only while the library is open.
        if let Err(error) = self
            .services
            .spawn_library_task("live subscription", stop, delivery)
        {
            self.services.report_bug("open a live subscription", &error);
        }
    }
}

#[cfg(test)]
#[path = "live_tests.rs"]
mod tests;
