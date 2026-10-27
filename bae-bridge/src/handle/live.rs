//! The stream shapes behind `AppHandle`'s callback `subscribe_*` calls — a
//! channel of values core resolves, a watch — each wrapped in the
//! `LiveSubscription` the host cancels.

use super::*;

impl AppHandle {
    /// Run a subscription's delivery loop as the library's own work and hand
    /// back the object the host cancels it with. Closing the library ends the
    /// loop before its store closes, as it ends the library's other tasks.
    /// `body` receives the services and a runtime handle, and is built on the
    /// runtime rather than on the caller.
    pub(super) fn live_subscription<Fut>(
        &self,
        body: impl FnOnce(AppServices, tokio::runtime::Handle) -> Fut + Send + 'static,
    ) -> std::sync::Arc<crate::LiveSubscription>
    where
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let services = self.services.clone();
        let runtime = self.runtime.clone();
        let stop = bae_core::library::CancellationToken::new();
        let stopped = stop.clone();
        let delivery = async move {
            stopped.run_until_cancelled(body(services, runtime)).await;
        };
        // A host subscribes only while the library is open.
        if let Err(error) = self
            .services
            .spawn_library_task("live subscription", delivery)
        {
            self.services.report_bug("open a live subscription", &error);
        }
        std::sync::Arc::new(crate::LiveSubscription::new(stop))
    }

    /// Hand `deliver` every value core sends on a subscription channel. The loop
    /// ends when core drops the sender.
    pub(super) fn subscribe_channel<T>(
        &self,
        open: impl FnOnce(&AppServices, &tokio::runtime::Handle) -> tokio::sync::mpsc::UnboundedReceiver<T>
            + Send
            + 'static,
        deliver: impl Fn(T) + Send + 'static,
    ) -> std::sync::Arc<crate::LiveSubscription>
    where
        T: Send + 'static,
    {
        self.live_subscription(move |services, runtime| async move {
            let mut values = open(&services, &runtime);
            while let Some(value) = values.recv().await {
                deliver(value);
            }
        })
    }

    /// Hand `deliver` a watch's current value and then each later one. The loop
    /// ends when the sender is dropped.
    pub(super) fn subscribe_watch<T>(
        &self,
        open: impl FnOnce(&AppServices) -> tokio::sync::watch::Receiver<T> + Send + 'static,
        deliver: impl Fn(&T) + Send + 'static,
    ) -> std::sync::Arc<crate::LiveSubscription>
    where
        T: Send + Sync + 'static,
    {
        self.live_subscription(move |services, _| async move {
            let mut values = open(&services);
            deliver(&values.borrow_and_update());
            while values.changed().await.is_ok() {
                deliver(&values.borrow_and_update());
            }
        })
    }
}

impl AppHandle {
    pub(super) fn subscribe_query<T>(
        &self,
        open: impl FnOnce(&AppServices) -> coven::LiveQuery<T> + Send + 'static,
        deliver: impl Fn(coven::CovenResult<T>) + Send + 'static,
    ) -> std::sync::Arc<crate::LiveSubscription>
    where
        T: Send + Clone + PartialEq + 'static,
    {
        self.live_subscription(move |services, _| async move {
            let mut values = open(&services);
            loop {
                let result = values.next().await;
                deliver(result);
            }
        })
    }
}

#[cfg(test)]
#[path = "live_tests.rs"]
mod tests;
