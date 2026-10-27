use bae_core::library::CancellationToken;

/// A live subscription's delivery, which the host stops by cancelling or
/// dropping this. Closing the library stops it as well.
#[derive(uniffi::Object)]
pub struct LiveSubscription {
    stop: CancellationToken,
}

impl LiveSubscription {
    /// The subscription whose delivery runs until `stop` is cancelled.
    pub(crate) fn new(stop: CancellationToken) -> Self {
        Self { stop }
    }
}

#[uniffi::export]
impl LiveSubscription {
    pub fn cancel(&self) {
        self.stop.cancel();
    }
}

impl Drop for LiveSubscription {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}
