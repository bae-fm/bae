//! The turn a request asks, as a value that can change while it waits.

use tokio::sync::watch;

use crate::PictureTurn;

/// What one request asks of its turn, as long as it waits: what it asks now,
/// and each change to it.
#[derive(Debug, Clone)]
pub struct AskedTurn(watch::Receiver<PictureTurn>);

impl AskedTurn {
    /// A request that asks `turn` for as long as it waits.
    pub fn unchanging(turn: PictureTurn) -> Self {
        Self(watch::channel(turn).1)
    }

    /// The turn asked now.
    pub fn now(&self) -> PictureTurn {
        *self.0.borrow()
    }

    /// Returns when the turn asked differs from the one asked at the last
    /// call. Never returns once nothing can change it any more.
    pub async fn changed(&mut self) {
        if self.0.changed().await.is_err() {
            std::future::pending::<()>().await;
        }
    }
}

impl From<watch::Receiver<PictureTurn>> for AskedTurn {
    /// A request that asks what `turn` holds, as it changes.
    fn from(turn: watch::Receiver<PictureTurn>) -> Self {
        Self(turn)
    }
}
