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
    AirPlayControlError, AirPlayFailed, AirPlaySink, AirPlayStreamControl, PcmSource, PcmWake,
};
use bae_playback::renderer::CastReceiver;
use bae_playback::AudioError;

use super::ap2_session::{Ap2Auth, Ap2Error, Ap2Session};
use super::capabilities::RaopEncryption;
use super::crypto::{apple_public_key, RaopCipher};
use super::device_verification::{SessionVerification, VerificationError};
use super::digest::DigestPassword;
use super::pairing::PairingError;
use super::session::{RaopError, RaopSession};
use super::stream::{PcmFeed, SystemClock};

/// The parameters an [`AirPlaySink`] needs to open a RAOP session to a receiver.
pub(crate) struct RaopSink {
    receiver: IpAddr,
    rtsp_port: u16,
    /// What each session proves bae with, for a receiver that verifies the
    /// device.
    verification: Option<SessionVerification>,
    /// The password the receiver asks for as HTTP Digest, if it has one.
    password: Option<DigestPassword>,
    /// The encryption the receiver requires — `None` for `et=0`, `RsaAes` for
    /// `et=1`.
    encryption: RaopEncryption,
    initial_volume: bae_playback::Volume,
    /// The receiver as playback knows it, which a failure names when the
    /// receiver no longer accepts this device.
    device: CastReceiver,
    /// What the session's stream threads run under, so one that panics fails
    /// the stream.
    diagnostics: Diagnostics,
}

impl RaopSink {
    pub(crate) fn new(
        receiver: IpAddr,
        rtsp_port: u16,
        verification: Option<SessionVerification>,
        password: Option<DigestPassword>,
        encryption: RaopEncryption,
        initial_volume: bae_playback::Volume,
        device: CastReceiver,
        diagnostics: Diagnostics,
    ) -> Self {
        Self {
            receiver,
            rtsp_port,
            verification,
            password,
            encryption,
            initial_volume,
            device,
            diagnostics,
        }
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
            self.verification.as_ref(),
            self.password.as_ref(),
            PcmFeed::new(source, wake),
            failed,
            cipher,
            self.initial_volume,
            Arc::new(SystemClock::new()),
        )
        .map_err(|error| match error {
            // The receiver no longer accepts the key or the password bae
            // kept: casting to it again asks anew.
            RaopError::Verification(VerificationError::NoLongerKnown) => {
                AudioError::AirPlayPairingLost(self.device.clone())
            }
            RaopError::PasswordRefused if self.password.is_some() => {
                AudioError::AirPlayPairingLost(self.device.clone())
            }
            error => AudioError::AirPlayRefused(error.to_string()),
        })?;

        Ok(Arc::new(session))
    }
}

impl AirPlayStreamControl for RaopSession {
    fn flush(&self) -> Result<(), AirPlayControlError> {
        self.flush().map_err(|e| AirPlayControlError::new(format!("FLUSH failed: {e}")))
    }

    fn reanchor(&self) -> Result<(), AirPlayControlError> {
        self.reanchor();
        Ok(())
    }

    fn failure(&self) -> Option<String> {
        self.failure()
    }

    fn latency(&self) -> Duration {
        self.latency()
    }
}

/// The parameters an [`AirPlaySink`] needs to open an AirPlay 2 session: pair
/// (transient pair-setup, or pair-verify with a kept pairing) then push
/// ChaCha-encrypted audio to a HomePod-class receiver.
pub(crate) struct Ap2Sink {
    receiver: IpAddr,
    /// The receiver's AirPlay control port (`_airplay._tcp`, usually 7000).
    airplay_port: u16,
    /// The name each session's pairing gives this device.
    client_name: String,
    /// How each session proves bae to the receiver.
    auth: Ap2Auth,
    /// The receiver as playback knows it, which a failure names when the
    /// receiver no longer accepts this device.
    device: CastReceiver,
    /// Where the session's ids come from.
    ids: coven::IdSourceRef,
    /// What the session's stream threads run under, so one that panics fails
    /// the stream.
    diagnostics: Diagnostics,
}

impl Ap2Sink {
    pub(crate) fn new(
        receiver: IpAddr,
        airplay_port: u16,
        client_name: String,
        auth: Ap2Auth,
        device: CastReceiver,
        ids: coven::IdSourceRef,
        diagnostics: Diagnostics,
    ) -> Self {
        Self { receiver, airplay_port, client_name, auth, device, ids, diagnostics }
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
            &self.client_name,
            &self.auth,
            PcmFeed::new(source, wake),
            failed,
            Arc::new(SystemClock::new()),
            self.ids.as_ref(),
        )
        .map_err(|error| match error {
            // The receiver no longer accepts the pairing bae kept: casting to
            // it again pairs anew.
            Ap2Error::Pairing(PairingError::PairingLost) => {
                AudioError::AirPlayPairingLost(self.device.clone())
            }
            error => AudioError::AirPlayRefused(error.to_string()),
        })?;

        Ok(Arc::new(session))
    }
}

impl AirPlayStreamControl for Ap2Session {
    fn flush(&self) -> Result<(), AirPlayControlError> {
        self.flush().map_err(|e| AirPlayControlError::new(format!("FLUSH failed: {e}")))
    }

    fn reanchor(&self) -> Result<(), AirPlayControlError> {
        self.reanchor();
        Ok(())
    }

    fn failure(&self) -> Option<String> {
        self.failure()
    }

    fn latency(&self) -> Duration {
        self.latency()
    }
}
