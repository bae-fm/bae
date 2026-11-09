//! The receiver connections playback's AirPlay output pushes PCM through.
//!
//! bae-library's AirPlay output decodes locally and hands its PCM to an
//! [`AirPlaySink`], then drives pause, resume and seek through the
//! [`AirPlayStreamControl`] the sink returns. These are the two real sinks, one
//! per dialect: [`RaopSink`] opens a RAOP session, [`Ap2Sink`] an AirPlay 2 one.

use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use bae_diagnostics::Diagnostics;
use bae_playback::airplay_output::{
    AirPlayFailed, AirPlaySink, AirPlayStreamControl, PcmSource, PcmWake,
};
use bae_playback::AudioError;
use tracing::warn;

use super::airplay2::TimingProtocol;
use super::ap2_session::Ap2Session;
use super::capabilities::RaopEncryption;
use super::crypto::{apple_public_key, RaopCipher};
use super::session::RaopSession;
use super::stream::{PcmFeed, SystemClock};

/// The parameters an [`AirPlaySink`] needs to open a RAOP session to a receiver.
pub(crate) struct RaopSink {
    receiver: IpAddr,
    rtsp_port: u16,
    /// The encryption the receiver requires — `None` for `et=0`, `RsaAes` for
    /// `et=1`.
    encryption: RaopEncryption,
    initial_volume: bae_playback::Volume,
    /// Where the session's stream threads report a panic.
    diagnostics: Diagnostics,
}

impl RaopSink {
    pub(crate) fn new(
        receiver: IpAddr,
        rtsp_port: u16,
        encryption: RaopEncryption,
        initial_volume: bae_playback::Volume,
        diagnostics: Diagnostics,
    ) -> Self {
        Self { receiver, rtsp_port, encryption, initial_volume, diagnostics }
    }
}

impl AirPlaySink for RaopSink {
    fn start(
        &self,
        source: Box<dyn PcmSource>,
        wake: PcmWake,
        failed: AirPlayFailed,
    ) -> Result<Arc<dyn AirPlayStreamControl>, AudioError> {
        let cipher = match self.encryption {
            RaopEncryption::None => RaopCipher::none(),
            RaopEncryption::RsaAes => RaopCipher::new_aes(&apple_public_key())
                .map_err(|e| AudioError::Unexpected(format!("RAOP key wrap: {e}")))?,
        };
        let session = RaopSession::start(
            &self.diagnostics,
            self.receiver,
            self.rtsp_port,
            PcmFeed::new(source, wake),
            failed,
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
    receiver: IpAddr,
    /// The receiver's AirPlay control port (`_airplay._tcp`, usually 7000).
    airplay_port: u16,
    /// The clock the receiver requires, decided from its features — PTP or NTP.
    timing: TimingProtocol,
    /// Where the session's ids come from.
    ids: coven::IdSourceRef,
    /// Where the session's stream threads report a panic.
    diagnostics: Diagnostics,
}

impl Ap2Sink {
    pub(crate) fn new(
        receiver: IpAddr,
        airplay_port: u16,
        timing: TimingProtocol,
        ids: coven::IdSourceRef,
        diagnostics: Diagnostics,
    ) -> Self {
        Self { receiver, airplay_port, timing, ids, diagnostics }
    }
}

impl AirPlaySink for Ap2Sink {
    fn start(
        &self,
        source: Box<dyn PcmSource>,
        wake: PcmWake,
        failed: AirPlayFailed,
    ) -> Result<Arc<dyn AirPlayStreamControl>, AudioError> {
        let session = Ap2Session::start(
            &self.diagnostics,
            self.receiver,
            self.airplay_port,
            PcmFeed::new(source, wake),
            failed,
            self.timing,
            Arc::new(SystemClock::new()),
            self.ids.as_ref(),
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
