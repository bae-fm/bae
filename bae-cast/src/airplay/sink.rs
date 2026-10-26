//! The receiver connections playback's AirPlay output pushes PCM through.
//!
//! bae-core's AirPlay output decodes locally and hands its PCM to an
//! [`AirPlaySink`], then drives pause, resume and seek through the
//! [`AirPlayStreamControl`] the sink returns. These are the two real sinks, one
//! per dialect: [`RaopSink`] opens a RAOP session, [`Ap2Sink`] an AirPlay 2 one.

use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use bae_core::playback::airplay_output::{AirPlaySink, AirPlayStreamControl, PcmSource};
use bae_core::playback::AudioError;
use tracing::warn;

use super::airplay2::TimingProtocol;
use super::ap2_session::Ap2Session;
use super::capabilities::RaopEncryption;
use super::crypto::{apple_public_key, RaopCipher};
use super::session::RaopSession;
use super::stream::SystemClock;

/// The parameters an [`AirPlaySink`] needs to open a RAOP session to a receiver.
pub(crate) struct RaopSink {
    pub receiver: IpAddr,
    pub rtsp_port: u16,
    /// The encryption the receiver requires — `None` for `et=0`, `RsaAes` for
    /// `et=1`.
    pub encryption: RaopEncryption,
    pub initial_volume: f32,
}

impl AirPlaySink for RaopSink {
    fn start(
        &self,
        source: Box<dyn PcmSource>,
    ) -> Result<Arc<dyn AirPlayStreamControl>, AudioError> {
        let cipher = match self.encryption {
            RaopEncryption::None => RaopCipher::none(),
            RaopEncryption::RsaAes => RaopCipher::new_aes(&apple_public_key())
                .map_err(|e| AudioError::Unexpected(format!("RAOP key wrap: {e}")))?,
        };
        let session = RaopSession::start(
            self.receiver,
            self.rtsp_port,
            source,
            cipher,
            self.initial_volume,
            Arc::new(SystemClock::new()),
        )
        .map_err(|e| AudioError::AirPlayRefused(e.to_string()))?;

        Ok(Arc::new(session))
    }
}

impl AirPlayStreamControl for RaopSession {
    fn flush(&self) {
        if let Err(e) = self.flush() {
            warn!("airplay FLUSH failed: {e}");
        }
    }

    fn reanchor(&self) {
        self.reanchor();
    }

    fn failure(&self) -> Option<String> {
        self.failure()
    }

    fn latency(&self) -> Duration {
        self.latency()
    }
}

/// The parameters an [`AirPlaySink`] needs to open an AirPlay 2 session: pair
/// (transient) then push ChaCha-encrypted audio to a HomePod-class receiver.
pub(crate) struct Ap2Sink {
    pub receiver: IpAddr,
    /// The receiver's AirPlay control port (`_airplay._tcp`, usually 7000).
    pub airplay_port: u16,
    /// The clock the receiver requires, decided from its features — PTP or NTP.
    pub timing: TimingProtocol,
}

impl AirPlaySink for Ap2Sink {
    fn start(
        &self,
        source: Box<dyn PcmSource>,
    ) -> Result<Arc<dyn AirPlayStreamControl>, AudioError> {
        let session = Ap2Session::start(
            self.receiver,
            self.airplay_port,
            source,
            self.timing,
            Arc::new(SystemClock::new()),
        )
        .map_err(|e| AudioError::AirPlayRefused(e.to_string()))?;

        Ok(Arc::new(session))
    }
}

impl AirPlayStreamControl for Ap2Session {
    fn flush(&self) {
        if let Err(e) = self.flush() {
            warn!("airplay 2 pause (rate 0) failed: {e}");
        }
    }

    fn reanchor(&self) {
        if let Err(e) = self.reanchor() {
            warn!("airplay 2 resume (rate 1) failed: {e}");
        }
    }

    fn failure(&self) -> Option<String> {
        self.failure()
    }

    fn latency(&self) -> Duration {
        self.latency()
    }
}
