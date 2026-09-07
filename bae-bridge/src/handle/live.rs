//! The stream shapes behind `AppHandle`'s `subscribe_*` calls — a coven live
//! query, a channel of values core resolves, a watch — each wrapped in the
//! `LiveSubscription` the host cancels.

use super::*;

impl AppHandle {
    /// Spawn a subscription's delivery loop on the app runtime and hand back the
    /// object the host cancels it with. `body` receives the services and a
    /// runtime handle, and is built on the runtime rather than on the caller.
    pub(super) fn live_subscription<Fut>(
        &self,
        body: impl FnOnce(AppServices, tokio::runtime::Handle) -> Fut + Send + 'static,
    ) -> std::sync::Arc<crate::LiveSubscription>
    where
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let services = self.services.clone();
        let runtime = self.runtime.handle().clone();
        let body_runtime = runtime.clone();
        let task = crate::operation_runtime::spawn(runtime, move || body(services, body_runtime));
        std::sync::Arc::new(crate::LiveSubscription::new(task))
    }

    /// Hand `deliver` every value a coven live query produces and every query
    /// failure. A live query has no end of its own: the loop runs until the host
    /// cancels the subscription.
    pub(super) fn subscribe_live_query<T>(
        &self,
        open: impl FnOnce(&AppServices) -> coven::LiveQuery<T> + Send + 'static,
        deliver: impl Fn(&AppServices, Result<T, coven::CovenError>) + Send + 'static,
    ) -> std::sync::Arc<crate::LiveSubscription>
    where
        T: Clone + PartialEq + Send + 'static,
    {
        self.live_subscription(move |services, _| async move {
            let mut query = open(&services);
            loop {
                let value = query.next().await;
                deliver(&services, value);
            }
        })
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
