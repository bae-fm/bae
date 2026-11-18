//! The cancel signal an app hands a call it may stop while the call runs.

use std::sync::Arc;

/// A cancel the app sends to one running call: the call watches it, and
/// dropping the call's future sends it too.
#[derive(uniffi::Object)]
pub struct BridgeCancellation {
    cancel: tokio::sync::watch::Sender<bool>,
}

#[uniffi::export]
impl BridgeCancellation {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        let (cancel, _) = tokio::sync::watch::channel(false);
        Arc::new(Self { cancel })
    }

    pub fn cancel(&self) {
        self.cancel.send_replace(true);
    }
}

impl BridgeCancellation {
    /// The signal for one call to watch, which turns true once the app
    /// cancels, with the guard that sends the cancel when the call's future
    /// drops.
    pub(crate) fn watched(self: Arc<Self>) -> (tokio::sync::watch::Receiver<bool>, CancelOnDrop) {
        (self.cancel.subscribe(), CancelOnDrop(self))
    }
}

/// Sends its cancellation when dropped.
pub(crate) struct CancelOnDrop(Arc<BridgeCancellation>);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}
