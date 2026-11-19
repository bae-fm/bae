//! Turning a discovered AirPlay receiver into the sink playback pushes audio
//! to: what is refused before connecting, how bae pairs with a receiver
//! before playback moves, asking the person for the code a receiver wants and
//! keeping the pairing, and what a pairing that failed says to the person who
//! cast.

use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use bae_playback::airplay_output::AirPlaySink;
use bae_playback::renderer::CastReceiver;
use bae_util::task_failure::TaskFailed;

use crate::airplay::airplay2::TimingProtocol;
use crate::airplay::ap2_pairing::{run_code_pair_setup, start_pin, PAIR_PIN_START, PAIR_SETUP_M1};
use crate::airplay::ap2_session::{pair_once, Ap2Auth, Ap2Error, AP2_USER_AGENT};
use crate::airplay::capabilities::{Ap2Pairing, Dialect, RaopAuth, RaopEncryption, ReceiverAccess};
use crate::airplay::pairing::{HomeKitError, PairingError};
use crate::airplay::pairing_keys::{pairing_secret_name, ControllerIdentity, KeptPairing};
use crate::airplay::rtsp::RtspConnection;
use crate::airplay::sink::{Ap2Sink, RaopSink};
use crate::airplay::{AirPlayCapabilities, UnreadableSetting};
use crate::code_prompt::{
    CodeAsked, CodeKind, CodePrompt, CodeRefused, CodeRequest, AIRPLAY_PIN_DIGITS,
};
use crate::{CastController, CastError};

/// The PIN every AirPlay receiver that shows one asks for.
const AIRPLAY_PIN: CodeKind = CodeKind::Pin { digits: AIRPLAY_PIN_DIGITS };

/// An AirPlay 2 receiver bae pairs with, as each pairing step reaches it.
#[derive(Clone)]
struct Ap2Receiver {
    addr: IpAddr,
    port: u16,
    /// This device's name, which the receiver shows beside a PIN.
    client_name: String,
    receiver: CastReceiver,
}

/// Where a pairing by code keeps what it agreed on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeptFor {
    /// In this device's keychain entry for the receiver, for every later
    /// cast.
    LaterCasts,
    /// In the cast's sink alone: the receiver asks for a PIN on every cast.
    ThisCast,
}

/// How one try of a code ended, short of a refusal.
enum CodeTried {
    Paired(KeptPairing),
    /// The receiver didn't take the code; the next try waits this long, when
    /// the receiver said.
    Wrong {
        retry_after: Option<Duration>,
    },
}

impl CastController {
    /// Move playback, as cast `number`, to the AirPlay receiver `device`,
    /// pairing with it first as its advertisement says: a refusal the
    /// advertisement states fails before connecting, an AirPlay 2 receiver
    /// pairs before playback moves, asking `prompt` for a code where the
    /// receiver wants one, and a RAOP receiver that asks for nothing pairs not
    /// at all. AirPlay pushes audio, so playback keeps decoding locally and
    /// hands its output to the receiver's sink; no ephemeral server is needed.
    pub(crate) async fn play_on_airplay(
        &self,
        device: CastReceiver,
        addr: IpAddr,
        port: u16,
        capabilities: &Result<AirPlayCapabilities, UnreadableSetting>,
        prompt: Arc<dyn CodePrompt>,
        number: u64,
    ) -> Result<(), CastError> {
        let capabilities = capabilities
            .as_ref()
            .map_err(|unreadable| CastError::AirPlayUnreadableSettings(unreadable.clone()))?;
        let receiver = device.name.clone();
        match capabilities.access {
            ReceiverAccess::Anyone => {}
            ReceiverAccess::HomeMembers => return Err(CastError::AirPlayHomeOnly { receiver }),
            ReceiverAccess::Admins => return Err(CastError::AirPlayAdminsOnly { receiver }),
            ReceiverAccess::Owner => return Err(CastError::AirPlayOwnerOnly { receiver }),
        }
        let shown = device.clone();
        let sink: Box<dyn AirPlaySink> = match &capabilities.dialect {
            Dialect::AirPlay2 { pairing } => {
                // The receiver shows this name to the person beside it.
                let client_name =
                    self.services.this_device_name().await.map_err(CastError::DeviceName)?;
                let target = Ap2Receiver { addr, port, client_name, receiver: device };
                let auth = self.authenticate(&target, *pairing, prompt).await?;
                Box::new(Ap2Sink::new(
                    addr,
                    port,
                    // The receiver's features decide whether it requires PTP or accepts NTP.
                    TimingProtocol::from_features(capabilities.features),
                    target.client_name,
                    auth,
                    target.receiver,
                    self.ids.clone(),
                    self.diagnostics.clone(),
                ))
            }
            Dialect::Raop { auth: RaopAuth { device_verification: true, .. }, .. }
            | Dialect::Raop { auth: RaopAuth { password: true, .. }, .. } => {
                return Err(CastError::AirPlay1CodeUnsupported);
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
                Box::new(RaopSink::new(
                    addr,
                    port,
                    encryption,
                    // The receiver plays at its hardware volume; bae attenuates locally
                    // in the output drain, so seed the session at full.
                    bae_playback::Volume::FULL,
                    self.diagnostics.clone(),
                ))
            }
        };
        self.hand_over(number, || {
            self.services.playback_play_on_airplay(sink, shown);
        })
    }

    /// Pair with an AirPlay 2 receiver as `pairing` says, before playback
    /// moves to it, and return how each of its sessions proves bae. A
    /// receiver that answers transient pair-setup's first step with 470 asks
    /// for a PIN, as one whose flags ask for it once.
    async fn authenticate(
        &self,
        target: &Ap2Receiver,
        pairing: Ap2Pairing,
        prompt: Arc<dyn CodePrompt>,
    ) -> Result<Ap2Auth, CastError> {
        match pairing {
            Ap2Pairing::Transient => match self.pair_with(target, Ap2Auth::Transient).await? {
                Ok(()) => Ok(Ap2Auth::Transient),
                Err(Ap2Error::Rejected { step: PAIR_SETUP_M1, status: 470 }) => {
                    self.kept_or_new_pairing(target, AIRPLAY_PIN, prompt).await
                }
                Err(error) => Err(airplay_pairing_failure(error, &target.receiver.name)),
            },
            Ap2Pairing::PinOnce => self.kept_or_new_pairing(target, AIRPLAY_PIN, prompt).await,
            Ap2Pairing::Password => {
                self.kept_or_new_pairing(target, CodeKind::Password, prompt).await
            }
            Ap2Pairing::PinEveryCast => {
                let kept = self
                    .pair_by_code(
                        target,
                        AIRPLAY_PIN,
                        CodeAsked::EveryCast,
                        KeptFor::ThisCast,
                        prompt,
                    )
                    .await?;
                Ok(Ap2Auth::Verify(kept))
            }
        }
    }

    /// Pair-verify with the pairing this device kept for the receiver, or
    /// pair anew by `code` when there is none or the receiver refuses it.
    async fn kept_or_new_pairing(
        &self,
        target: &Ap2Receiver,
        code: CodeKind,
        prompt: Arc<dyn CodePrompt>,
    ) -> Result<Ap2Auth, CastError> {
        let why = match self.kept_pairing(&target.receiver.id).await? {
            None => CodeAsked::FirstPairing,
            Some(kept) => match self.pair_with(target, Ap2Auth::Verify(kept.clone())).await? {
                Ok(()) => return Ok(Ap2Auth::Verify(kept)),
                Err(Ap2Error::Pairing(PairingError::PairingLost)) => CodeAsked::PairingLost,
                Err(error) => return Err(airplay_pairing_failure(error, &target.receiver.name)),
            },
        };
        // Pair-setup proved this device and the receiver to each other; the
        // session pair-verifies with the keys it agreed on.
        let kept = self.pair_by_code(target, code, why, KeptFor::LaterCasts, prompt).await?;
        Ok(Ap2Auth::Verify(kept))
    }

    /// Pair with the receiver once as `auth` says, off the runtime. The
    /// pairing runs to its end even when this cast is called off meanwhile;
    /// its connection closes either way. The outer `Err` is a bug.
    async fn pair_with(
        &self,
        target: &Ap2Receiver,
        auth: Ap2Auth,
    ) -> Result<Result<(), Ap2Error>, CastError> {
        let (addr, port, client_name) = (target.addr, target.port, target.client_name.clone());
        self.diagnostics
            .blocking("pair with an AirPlay 2 receiver", move || {
                pair_once(addr, port, &client_name, &auth)
            })
            .await
            .map_err(CastError::Internal)
    }

    /// The pairing this device kept for the receiver `receiver_id`, if any.
    async fn kept_pairing(&self, receiver_id: &str) -> Result<Option<KeptPairing>, CastError> {
        let secrets = self.device_secrets.clone();
        let name = pairing_secret_name(receiver_id);
        let kept = self
            .diagnostics
            .blocking("read a kept AirPlay pairing", move || secrets.device_secret(&name))
            .await
            .map_err(CastError::Internal)?
            .map_err(CastError::PairingKeys)?;
        kept.map(|text| {
            KeptPairing::decode(&text)
                .map_err(|malformed| CastError::KeptPairingUnreadable(malformed.to_string()))
        })
        .transpose()
    }

    /// Ask `prompt` for the receiver's `code`, saying `why`, and pair with
    /// what the person enters until the receiver takes it or refuses. A PIN
    /// starts with `/pair-pin-start`, which makes the receiver show it, and
    /// starts again after a wrong one, since the receiver then shows a new
    /// PIN. A code of the wrong shape is asked for again without connecting.
    /// After a wrong code whose reply states a wait, the next code goes only
    /// once the wait has passed.
    async fn pair_by_code(
        &self,
        target: &Ap2Receiver,
        code: CodeKind,
        why: CodeAsked,
        kept_for: KeptFor,
        prompt: Arc<dyn CodePrompt>,
    ) -> Result<KeptPairing, CastError> {
        let mut last_try = None;
        let mut pin_shown = false;
        let mut not_before = None;
        loop {
            if matches!(code, CodeKind::Pin { .. }) && !pin_shown {
                self.show_pin(target).await?;
                pin_shown = true;
            }
            let entered = prompt
                .code(CodeRequest { code, why, last_try })
                .await
                .map_err(|bug| CastError::Internal(TaskFailed::Panicked(bug)))?;
            if !code.fits(&entered) {
                last_try = Some(CodeRefused::WrongShape);
                continue;
            }
            if let Some(not_before) = not_before.take() {
                tokio::time::sleep_until(not_before).await;
            }
            match self.try_code(target, code, entered, kept_for).await? {
                CodeTried::Paired(kept) => return Ok(kept),
                CodeTried::Wrong { retry_after } => {
                    last_try = Some(CodeRefused::WrongCode);
                    pin_shown = false;
                    not_before = retry_after.map(|wait| tokio::time::Instant::now() + wait);
                }
            }
        }
    }

    /// Ask the receiver to show a PIN, on a connection of its own. A Mac
    /// whose AirPlay Receiver plays only for its current user answers 400.
    async fn show_pin(&self, target: &Ap2Receiver) -> Result<(), CastError> {
        let (addr, port, client_name) = (target.addr, target.port, target.client_name.clone());
        let shown = self
            .diagnostics
            .blocking("ask an AirPlay receiver to show a PIN", move || {
                let mut conn = RtspConnection::connect(addr, port, AP2_USER_AGENT)?;
                start_pin(&mut conn, &client_name)
            })
            .await
            .map_err(CastError::Internal)?;
        shown.map_err(|error| match error {
            Ap2Error::Rejected { step: PAIR_PIN_START, status: 400 } => {
                CastError::AirPlayOwnerOnly { receiver: target.receiver.name.clone() }
            }
            error => airplay_pairing_failure(error, &target.receiver.name),
        })
    }

    /// Pair-setup with `entered` on a connection of its own, and keep what
    /// the receiver agreed on where `kept_for` says. Once the code is sent,
    /// the pairing and the keychain write run to their end even when the
    /// cast is called off meanwhile, so a receiver never holds a pairing
    /// whose keys bae dropped halfway. A write the keychain refuses fails the
    /// cast.
    async fn try_code(
        &self,
        target: &Ap2Receiver,
        code: CodeKind,
        entered: String,
        kept_for: KeptFor,
    ) -> Result<CodeTried, CastError> {
        let (addr, port, client_name) = (target.addr, target.port, target.client_name.clone());
        let receiver = target.receiver.name.clone();
        let entry = pairing_secret_name(&target.receiver.id);
        let identity = ControllerIdentity::new(self.ids.as_ref());
        let secrets = self.device_secrets.clone();
        self.diagnostics
            .blocking("pair with an AirPlay receiver by code", move || {
                let paired = RtspConnection::connect(addr, port, AP2_USER_AGENT)
                    .map_err(Ap2Error::from)
                    .and_then(|mut conn| {
                        run_code_pair_setup(&mut conn, &client_name, &entered, identity)
                    });
                let keys = match paired {
                    Ok(keys) => keys,
                    Err(Ap2Error::Pairing(PairingError::WrongCode { retry_after_seconds })) => {
                        return Ok(CodeTried::Wrong {
                            retry_after: retry_after_seconds.map(Duration::from_secs),
                        });
                    }
                    Err(error) => return Err(airplay_pairing_failure(error, &receiver)),
                };
                let password = match code {
                    CodeKind::Password => Some(entered),
                    CodeKind::Pin { .. } => None,
                };
                let kept = KeptPairing { keys, password };
                if kept_for == KeptFor::LaterCasts {
                    secrets
                        .set_device_secret(&entry, &kept.encode())
                        .map_err(CastError::PairingKeys)?;
                }
                Ok(CodeTried::Paired(kept))
            })
            .await
            .map_err(CastError::Internal)?
    }
}

/// What a failed pairing with the receiver named `receiver` says to the
/// person who cast: the receiver's own refusal, a reply that didn't check
/// out, or a connection that failed, which is never read as a refusal.
pub(crate) fn airplay_pairing_failure(error: Ap2Error, receiver: &str) -> CastError {
    let receiver = receiver.to_string();
    match error {
        // The connect failed, timed out, or dropped mid-pairing.
        Ap2Error::Io(error) if error.kind() != std::io::ErrorKind::InvalidData => {
            CastError::AirPlayUnreachable { receiver, detail: error.to_string() }
        }
        Ap2Error::Pairing(PairingError::Receiver(refusal)) => match refusal {
            HomeKitError::MaxTries => CastError::AirPlayLockedOut { receiver },
            HomeKitError::Busy => CastError::AirPlayBusy { receiver },
            HomeKitError::BackOff { seconds } => CastError::AirPlayBackOff { receiver, seconds },
            HomeKitError::Unavailable => CastError::AirPlayNotPairing { receiver },
            HomeKitError::MaxPeers => CastError::AirPlayFull { receiver },
            HomeKitError::Unknown | HomeKitError::Authentication | HomeKitError::Other(_) => {
                CastError::AirPlayPairingFailed { receiver, detail: refusal.to_string() }
            }
        },
        // The rest is a reply that failed a check, a malformed message, or a
        // status that names no HomeKit error.
        error @ (Ap2Error::Io(_)
        | Ap2Error::Rejected { .. }
        | Ap2Error::Pairing(_)
        | Ap2Error::Channel(_)
        | Ap2Error::BadBody(_)
        | Ap2Error::BodyDecode(_)) => {
            CastError::AirPlayPairingFailed { receiver, detail: error.to_string() }
        }
    }
}

#[cfg(test)]
#[path = "airplay_cast_tests.rs"]
mod tests;
