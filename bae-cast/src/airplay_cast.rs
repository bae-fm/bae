//! Turning a discovered AirPlay receiver into the sink playback pushes audio
//! to: what is refused before connecting, and what a pairing that failed says
//! to the person who cast.

use bae_diagnostics::Diagnostics;
use bae_playback::airplay_output::AirPlaySink;

use crate::airplay::airplay2::TimingProtocol;
use crate::airplay::ap2_session::{Ap2Error, PAIR_SETUP_M1};
use crate::airplay::capabilities::{Ap2Pairing, Dialect, RaopAuth, RaopEncryption, ReceiverAccess};
use crate::airplay::pairing::{HomeKitError, PairingError};
use crate::airplay::sink::{Ap2Sink, RaopSink};
use crate::airplay::{AirPlayCapabilities, UnreadableSetting};
use crate::CastError;

/// What the cast does with a receiver before playback moves to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CastPairing {
    /// An AirPlay 2 receiver that pairs with the password-less transient pairing:
    /// the cast pairs once first, so a receiver that refuses says why where the
    /// cast was asked for.
    Transient,
    /// A RAOP receiver that asks for nothing: there is nothing to settle before
    /// its first session.
    Nothing,
}

/// Build the push-audio sink for an AirPlay receiver, or a kind-specific error
/// when it can't be driven: one advertising a setting bae can't read, one that
/// plays only from people in its home, its home's admins or its owner, one that
/// asks for a PIN, a password or device verification, or one offering only
/// encryption the sender can't provide. An AirPlay 2 receiver that asks for
/// nothing pairs transiently, whatever its feature bits. RAOP receivers pick
/// unencrypted when offered, else RSA-AES.
pub(crate) fn build_airplay_sink(
    ids: &coven::IdSourceRef,
    diagnostics: &Diagnostics,
    addr: std::net::IpAddr,
    port: u16,
    capabilities: &Result<AirPlayCapabilities, UnreadableSetting>,
) -> Result<(Box<dyn AirPlaySink>, CastPairing), CastError> {
    let capabilities = capabilities
        .as_ref()
        .map_err(|unreadable| CastError::AirPlayUnreadableSettings(unreadable.clone()))?;
    match capabilities.access {
        ReceiverAccess::Anyone => {}
        ReceiverAccess::HomeMembers => return Err(CastError::AirPlayHomeOnly),
        ReceiverAccess::Admins => return Err(CastError::AirPlayAdminsOnly),
        ReceiverAccess::Owner => return Err(CastError::AirPlayOwnerOnly),
    }
    match &capabilities.dialect {
        Dialect::AirPlay2 { pairing: Ap2Pairing::Transient } => {
            Ok((
                Box::new(Ap2Sink::new(
                    addr,
                    port,
                    // The receiver's features decide whether it requires PTP or accepts NTP.
                    TimingProtocol::from_features(capabilities.features),
                    ids.clone(),
                    diagnostics.clone(),
                )),
                CastPairing::Transient,
            ))
        }
        Dialect::AirPlay2 { pairing: Ap2Pairing::PinOnce }
        | Dialect::AirPlay2 { pairing: Ap2Pairing::PinEveryCast }
        | Dialect::AirPlay2 { pairing: Ap2Pairing::Password }
        | Dialect::Raop { auth: RaopAuth { device_verification: true, .. }, .. }
        | Dialect::Raop { auth: RaopAuth { password: true, .. }, .. } => {
            Err(CastError::AirPlayPinRequired)
        }
        Dialect::Raop {
            encryption: offered,
            auth: RaopAuth { device_verification: false, password: false },
        } => {
            let encryption = if offered.contains(&RaopEncryption::None) {
                RaopEncryption::None
            } else if offered.contains(&RaopEncryption::RsaAes) {
                RaopEncryption::RsaAes
            } else {
                return Err(CastError::AirPlayEncryptionUnsupported);
            };
            Ok((
                Box::new(RaopSink::new(
                    addr,
                    port,
                    encryption,
                    // The receiver plays at its hardware volume; bae attenuates locally
                    // in the output drain, so seed the session at full.
                    bae_playback::Volume::FULL,
                    diagnostics.clone(),
                )),
                CastPairing::Nothing,
            ))
        }
    }
}

/// What a failed transient pair-setup says to the person who cast: the
/// receiver's own refusal, a reply that didn't check out, or a connection that
/// failed, which is never read as a refusal.
pub(crate) fn airplay_pairing_failure(error: Ap2Error) -> CastError {
    match error {
        // The connect failed, timed out, or dropped mid-pairing.
        Ap2Error::Io(error) if error.kind() != std::io::ErrorKind::InvalidData => {
            CastError::AirPlayUnreachable(error.to_string())
        }
        // A 470 to the first step is a receiver that wants a PIN.
        Ap2Error::Rejected { step, status: 470 } if step == PAIR_SETUP_M1 => {
            CastError::AirPlayPinRequired
        }
        Ap2Error::Pairing(PairingError::Receiver(refusal)) => match refusal {
            HomeKitError::MaxTries => CastError::AirPlayLockedOut,
            HomeKitError::Busy => CastError::AirPlayBusy,
            HomeKitError::BackOff { seconds } => CastError::AirPlayBackOff { seconds },
            HomeKitError::Unavailable => CastError::AirPlayNotPairing,
            HomeKitError::MaxPeers => CastError::AirPlayFull,
            HomeKitError::Unknown | HomeKitError::Authentication | HomeKitError::Other(_) => {
                CastError::AirPlayPairingFailed(refusal.to_string())
            }
        },
        // The rest is a reply that failed a check, a malformed message, or a
        // status that names no HomeKit error.
        error @ (Ap2Error::Io(_)
        | Ap2Error::Rejected { .. }
        | Ap2Error::Pairing(_)
        | Ap2Error::Channel(_)
        | Ap2Error::BadBody(_)
        | Ap2Error::BodyDecode(_)) => CastError::AirPlayPairingFailed(error.to_string()),
    }
}
