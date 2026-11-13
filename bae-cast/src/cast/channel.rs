//! The CASTV2 control channel to one connected Cast device.
//!
//! [`CastChannel`] is the Cast implementation of the shared
//! [`RendererChannel`] the session drives, over bae's own Cast client
//! ([`CastClient`]). The session calls it from a thread of its own, so each
//! command waits for the receiver's answer on the app runtime without
//! blocking it. A Cast receiver says its status as it changes, so the channel
//! is a [`PushedRenderer`]: the client reports to the session's sink.

use std::sync::Arc;
use std::time::Duration;

use bae_diagnostics::Diagnostics;
use bae_playback::renderer::{
    PushedRenderer, RendererChannel, RendererError, RendererMedia, StatusSink,
};
use tokio::runtime::Handle;

use super::client::CastClient;
use crate::LONGEST_RECEIVER_TIME;

/// The Cast [`RendererChannel`] and [`PushedRenderer`]. Driven from the session's std thread, which
/// is never one of the runtime's.
pub(crate) struct CastChannel {
    /// Shared with the task that closes the cast's connections when the
    /// channel is dropped.
    client: Arc<CastClient>,
    runtime: Handle,
    diagnostics: Diagnostics,
}

impl CastChannel {
    pub(crate) fn new(client: CastClient, runtime: Handle, diagnostics: Diagnostics) -> Self {
        Self { client: Arc::new(client), runtime, diagnostics }
    }
}

impl Drop for CastChannel {
    /// Close the cast's connections in order on the runtime. Dropping may
    /// happen on a runtime thread, where nothing may block, and nothing
    /// waits for the close: it runs to its end.
    fn drop(&mut self) {
        let client = self.client.clone();
        drop(self.diagnostics.spawn_reported(
            &self.runtime,
            "close the Cast connections",
            async move { client.close().await },
            |_| {},
        ));
    }
}

impl RendererChannel for CastChannel {
    fn load(&mut self, media: &RendererMedia) -> Result<(), RendererError> {
        self.runtime.block_on(self.client.load(media))
    }

    fn play(&mut self) -> Result<(), RendererError> {
        self.runtime.block_on(self.client.play())
    }

    fn pause(&mut self) -> Result<(), RendererError> {
        self.runtime.block_on(self.client.pause())
    }

    fn seek(&mut self, position: Duration) -> Result<(), RendererError> {
        self.runtime.block_on(self.client.seek(position))
    }

    fn set_volume(&mut self, level: bae_playback::Volume) -> Result<(), RendererError> {
        self.runtime.block_on(self.client.set_volume(level))
    }

    fn stop(&mut self) -> Result<(), RendererError> {
        self.runtime.block_on(self.client.stop())
    }
}

impl PushedRenderer for CastChannel {
    fn report_to(&mut self, sink: StatusSink) {
        self.client.report_to(sink);
    }
}

/// A time in seconds a receiver stated, when it is a time: finite, not
/// negative and no longer than [`LONGEST_RECEIVER_TIME`].
pub(super) fn receiver_time(seconds: f64) -> Option<Duration> {
    Duration::try_from_secs_f64(seconds).ok().filter(|time| *time <= LONGEST_RECEIVER_TIME)
}
