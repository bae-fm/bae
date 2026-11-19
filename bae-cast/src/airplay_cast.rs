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
use crate::airplay::capabilities::{
    Ap2Pairing, DeviceVerification, Dialect, RaopAuth, RaopEncryption, ReceiverAccess,
};
use crate::airplay::device_verification::{self, SessionVerification, VerificationError};
use crate::airplay::digest::DigestStyle;
use crate::airplay::pairing::{HomeKitError, PairingError};
use crate::airplay::pairing_keys::{
    pairing_secret_name, password_secret_name, verification_secret_name, ControllerIdentity,
    KeptPairing, MalformedPairingKeys, VerificationKey,
};
use crate::airplay::rtsp::RtspConnection;
use crate::airplay::session::{
    asks_for_verification, password_accepted, RaopError, RaopPassword, RAOP_USER_AGENT,
};
use crate::airplay::sink::{Ap2Sink, RaopSink};
use crate::airplay::{AirPlayCapabilities, UnreadableSetting};
use crate::code_prompt::{
    CodeAsked, CodeKind, CodePrompt, CodeRefused, CodeRequest, AIRPLAY_PIN_DIGITS,
};
use crate::{CastController, CastError};

/// The PIN every AirPlay receiver that shows one asks for.
const AIRPLAY_PIN: CodeKind = CodeKind::Pin { digits: AIRPLAY_PIN_DIGITS };

/// An AirPlay receiver bae pairs with, as each pairing step reaches it: an
/// AirPlay 2 receiver at its AirPlay port, or an AirPlay 1 receiver that
/// verifies the device at its RAOP port.
#[derive(Clone)]
struct PairingTarget {
    addr: IpAddr,
    port: u16,
    /// This device's name, which the receiver shows beside a PIN.
    client_name: String,
    receiver: CastReceiver,
}

/// An AirPlay 2 receiver bae pairs with by HomeKit pairing.
struct Ap2Receiver(PairingTarget);

/// An AirPlay 1 receiver bae pairs with by device verification.
struct RaopReceiver(PairingTarget);

/// An AirPlay 1 receiver that asks for a password as HTTP Digest, which
/// bae keeps as the receiver's pairing, with what each request to it proves
/// bae first where it verifies the device too.
struct RaopPasswordReceiver {
    target: PairingTarget,
    style: DigestStyle,
    verification: Option<SessionVerification>,
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
enum CodeTried<Kept> {
    Paired(Kept),
    /// The receiver didn't take the code; the next try waits this long, when
    /// the receiver said.
    Wrong {
        retry_after: Option<Duration>,
    },
}

/// How a pair-verify with a kept pairing ended, short of a refusal.
enum KeptVerified {
    /// The receiver knows this device.
    Accepted,
    /// The receiver no longer knows this device.
    Lost,
}

/// A receiver bae pairs with by a code the person enters: each AirPlay
/// generation's own pairing, under the one way bae asks for the code and
/// keeps what the pairing agreed on.
trait PairsByCode: Sync {
    /// What a pairing agreed on, which each later session proves bae with.
    type Kept: Clone + Send + 'static;

    /// The name of this device's keychain entry for the receiver's pairing.
    fn kept_entry(&self) -> String;

    /// Read the text a pairing was kept as.
    fn decode_kept(text: &str) -> Result<Self::Kept, MalformedPairingKeys>;

    /// Pair-verify with `kept` once, off the runtime.
    async fn verify_kept(
        &self,
        cast: &CastController,
        kept: Self::Kept,
    ) -> Result<KeptVerified, CastError>;

    /// Ask the receiver to show a PIN, on a connection of its own.
    async fn show_pin(&self, cast: &CastController) -> Result<(), CastError>;

    /// Pair with `entered`, a code of the kind `code`, on a connection of its
    /// own, and keep what the receiver agreed on where `kept_for` says. Once
    /// the code is sent, the pairing and the keychain write run to their end
    /// even when the cast is called off meanwhile, so a receiver never holds
    /// a pairing whose keys bae dropped halfway. A write the keychain refuses
    /// fails the cast.
    async fn try_code(
        &self,
        cast: &CastController,
        code: CodeKind,
        entered: String,
        kept_for: KeptFor,
    ) -> Result<CodeTried<Self::Kept>, CastError>;
}

impl CastController {
    /// Move playback, as cast `number`, to the AirPlay receiver `device`,
    /// pairing with it first as its advertisement says: a refusal the
    /// advertisement states fails before connecting, an AirPlay 2 receiver
    /// pairs before playback moves, an AirPlay 1 receiver that verifies the
    /// device pairs or verifies before playback moves, each asking `prompt`
    /// for a code where the receiver wants one, and an AirPlay 1 receiver
    /// that asks for nothing pairs not at all. AirPlay pushes audio, so
    /// playback keeps decoding locally and hands its output to the receiver's
    /// sink; no ephemeral server is needed.
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
                let target = self.pairing_target(addr, port, device).await?;
                let auth = self.authenticate(Ap2Receiver(target.clone()), *pairing, prompt).await?;
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
            Dialect::Raop {
                encryption: offered,
                auth: RaopAuth { device_verification, password },
            } => {
                let encryption = if offered.contains(&RaopEncryption::None) {
                    RaopEncryption::None
                } else if offered.contains(&RaopEncryption::RsaAes) {
                    RaopEncryption::RsaAes
                } else {
                    return Err(CastError::AirPlayEncryptionUnsupported);
                };
                let verification = self
                    .verify_device(addr, port, &device, *device_verification, prompt.clone())
                    .await?;
                let password = match password {
                    Some(style) => {
                        let target = RaopPasswordReceiver {
                            target: self.pairing_target(addr, port, device.clone()).await?,
                            style: *style,
                            verification: verification.clone(),
                        };
                        let kept =
                            self.kept_or_new_pairing(&target, CodeKind::Password, prompt).await?;
                        Some(RaopPassword { password: kept, style: *style })
                    }
                    None => None,
                };
                Box::new(RaopSink::new(
                    addr,
                    port,
                    verification,
                    password,
                    encryption,
                    // The receiver plays at its hardware volume; bae attenuates locally
                    // in the output drain, so seed the session at full.
                    bae_playback::Volume::FULL,
                    device,
                    self.diagnostics.clone(),
                ))
            }
        };
        self.hand_over(number, || {
            self.services.playback_play_on_airplay(sink, shown);
        })
    }

    /// The receiver `device` at `addr:port`, with this device's name, which
    /// the receiver shows to the person beside it.
    async fn pairing_target(
        &self,
        addr: IpAddr,
        port: u16,
        device: CastReceiver,
    ) -> Result<PairingTarget, CastError> {
        let client_name = self.services.this_device_name().await.map_err(CastError::DeviceName)?;
        Ok(PairingTarget { addr, port, client_name, receiver: device })
    }

    /// Pair with an AirPlay 2 receiver as `pairing` says, before playback
    /// moves to it, and return how each of its sessions proves bae. A
    /// receiver that answers transient pair-setup's first step with 470 asks
    /// for a PIN, as one whose flags ask for it once.
    async fn authenticate(
        &self,
        target: Ap2Receiver,
        pairing: Ap2Pairing,
        prompt: Arc<dyn CodePrompt>,
    ) -> Result<Ap2Auth, CastError> {
        let kept = match pairing {
            Ap2Pairing::Transient => match self.pair_with(&target.0, Ap2Auth::Transient).await? {
                Ok(()) => return Ok(Ap2Auth::Transient),
                Err(Ap2Error::Rejected { step: PAIR_SETUP_M1, status: 470 }) => {
                    self.kept_or_new_pairing(&target, AIRPLAY_PIN, prompt).await?
                }
                Err(error) => return Err(airplay_pairing_failure(error, &target.0.receiver.name)),
            },
            Ap2Pairing::PinOnce => self.kept_or_new_pairing(&target, AIRPLAY_PIN, prompt).await?,
            Ap2Pairing::Password => {
                self.kept_or_new_pairing(&target, CodeKind::Password, prompt).await?
            }
            Ap2Pairing::PinEveryCast => {
                self.pair_by_code(
                    &target,
                    AIRPLAY_PIN,
                    CodeAsked::EveryCast,
                    KeptFor::ThisCast,
                    prompt,
                )
                .await?
            }
        };
        // Pair-setup proved this device and the receiver to each other; each
        // session pair-verifies with the keys it agreed on.
        Ok(Ap2Auth::Verify(kept))
    }

    /// Verify this device to an AirPlay 1 receiver before playback moves, as
    /// its advertisement says, and return what each of its sessions proves
    /// bae with: nothing for a receiver that verifies no device. A receiver
    /// whose advertisement states no verification but that answers OPTIONS
    /// with 403 asks for it, as one whose flags ask for it once.
    async fn verify_device(
        &self,
        addr: IpAddr,
        port: u16,
        device: &CastReceiver,
        advertised: Option<DeviceVerification>,
        prompt: Arc<dyn CodePrompt>,
    ) -> Result<Option<SessionVerification>, CastError> {
        let asked = match advertised {
            Some(asked) => asked,
            None => {
                let asks = self
                    .diagnostics
                    .blocking("ask an AirPlay 1 receiver for its options", move || {
                        asks_for_verification(addr, port)
                    })
                    .await
                    .map_err(CastError::Internal)?
                    .map_err(|error| raop_pairing_failure(error.into(), &device.name))?;
                if !asks {
                    return Ok(None);
                }
                DeviceVerification::Once
            }
        };
        let target = RaopReceiver(self.pairing_target(addr, port, device.clone()).await?);
        let key = match asked {
            DeviceVerification::Once => {
                self.kept_or_new_pairing(&target, AIRPLAY_PIN, prompt).await?
            }
            DeviceVerification::EveryCast => {
                self.pair_by_code(
                    &target,
                    AIRPLAY_PIN,
                    CodeAsked::EveryCast,
                    KeptFor::ThisCast,
                    prompt,
                )
                .await?
            }
        };
        Ok(Some(SessionVerification { key, client_name: target.0.client_name }))
    }

    /// Pair-verify with the pairing this device kept for the receiver, or
    /// pair anew by `code` when there is none or the receiver refuses it.
    async fn kept_or_new_pairing<T: PairsByCode>(
        &self,
        target: &T,
        code: CodeKind,
        prompt: Arc<dyn CodePrompt>,
    ) -> Result<T::Kept, CastError> {
        let why = match self.kept_pairing(target).await? {
            None => CodeAsked::FirstPairing,
            Some(kept) => match target.verify_kept(self, kept.clone()).await? {
                KeptVerified::Accepted => return Ok(kept),
                KeptVerified::Lost => CodeAsked::PairingLost,
            },
        };
        self.pair_by_code(target, code, why, KeptFor::LaterCasts, prompt).await
    }

    /// Pair with the AirPlay 2 receiver once as `auth` says, off the runtime.
    /// The pairing runs to its end even when this cast is called off
    /// meanwhile; its connection closes either way. The outer `Err` is a bug.
    async fn pair_with(
        &self,
        target: &PairingTarget,
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

    /// The pairing this device kept for the receiver, if any.
    async fn kept_pairing<T: PairsByCode>(&self, target: &T) -> Result<Option<T::Kept>, CastError> {
        let secrets = self.device_secrets.clone();
        let name = target.kept_entry();
        let kept = self
            .diagnostics
            .blocking("read a kept AirPlay pairing", move || secrets.device_secret(&name))
            .await
            .map_err(CastError::Internal)?
            .map_err(CastError::PairingKeys)?;
        kept.map(|text| {
            T::decode_kept(&text)
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
    async fn pair_by_code<T: PairsByCode>(
        &self,
        target: &T,
        code: CodeKind,
        why: CodeAsked,
        kept_for: KeptFor,
        prompt: Arc<dyn CodePrompt>,
    ) -> Result<T::Kept, CastError> {
        let mut last_try = None;
        let mut pin_shown = false;
        let mut not_before = None;
        loop {
            if matches!(code, CodeKind::Pin { .. }) && !pin_shown {
                target.show_pin(self).await?;
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
            match target.try_code(self, code, entered, kept_for).await? {
                CodeTried::Paired(kept) => return Ok(kept),
                CodeTried::Wrong { retry_after } => {
                    last_try = Some(CodeRefused::WrongCode);
                    pin_shown = false;
                    not_before = retry_after.map(|wait| tokio::time::Instant::now() + wait);
                }
            }
        }
    }
}

impl PairsByCode for Ap2Receiver {
    type Kept = KeptPairing;

    fn kept_entry(&self) -> String {
        pairing_secret_name(&self.0.receiver.id)
    }

    fn decode_kept(text: &str) -> Result<KeptPairing, MalformedPairingKeys> {
        KeptPairing::decode(text)
    }

    async fn verify_kept(
        &self,
        cast: &CastController,
        kept: KeptPairing,
    ) -> Result<KeptVerified, CastError> {
        match cast.pair_with(&self.0, Ap2Auth::Verify(kept)).await? {
            Ok(()) => Ok(KeptVerified::Accepted),
            Err(Ap2Error::Pairing(PairingError::PairingLost)) => Ok(KeptVerified::Lost),
            Err(error) => Err(airplay_pairing_failure(error, &self.0.receiver.name)),
        }
    }

    /// A Mac whose AirPlay Receiver plays only for its current user answers
    /// the PIN start with 400.
    async fn show_pin(&self, cast: &CastController) -> Result<(), CastError> {
        let PairingTarget { addr, port, client_name, receiver } = self.0.clone();
        let shown = cast
            .diagnostics
            .blocking("ask an AirPlay receiver to show a PIN", move || {
                let mut conn = RtspConnection::connect(addr, port, AP2_USER_AGENT)?;
                start_pin(&mut conn, &client_name)
            })
            .await
            .map_err(CastError::Internal)?;
        shown.map_err(|error| match error {
            Ap2Error::Rejected { step: PAIR_PIN_START, status: 400 } => {
                CastError::AirPlayOwnerOnly { receiver: receiver.name }
            }
            error => airplay_pairing_failure(error, &receiver.name),
        })
    }

    async fn try_code(
        &self,
        cast: &CastController,
        code: CodeKind,
        entered: String,
        kept_for: KeptFor,
    ) -> Result<CodeTried<KeptPairing>, CastError> {
        let PairingTarget { addr, port, client_name, receiver } = self.0.clone();
        let entry = self.kept_entry();
        let identity = ControllerIdentity::new(cast.ids.as_ref());
        let secrets = cast.device_secrets.clone();
        cast.diagnostics
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
                    Err(error) => return Err(airplay_pairing_failure(error, &receiver.name)),
                };
                let password = match code {
                    CodeKind::Password => Some(entered),
                    CodeKind::Pin { .. } => None,
                };
                let kept = KeptPairing { keys, password };
                keep(&secrets, &entry, &kept.encode(), kept_for)?;
                Ok(CodeTried::Paired(kept))
            })
            .await
            .map_err(CastError::Internal)?
    }
}

impl PairsByCode for RaopReceiver {
    type Kept = VerificationKey;

    fn kept_entry(&self) -> String {
        verification_secret_name(&self.0.receiver.id)
    }

    fn decode_kept(text: &str) -> Result<VerificationKey, MalformedPairingKeys> {
        VerificationKey::decode(text)
    }

    async fn verify_kept(
        &self,
        cast: &CastController,
        kept: VerificationKey,
    ) -> Result<KeptVerified, CastError> {
        let PairingTarget { addr, port, client_name, receiver } = self.0.clone();
        let verified = cast
            .diagnostics
            .blocking("verify this device to an AirPlay 1 receiver", move || {
                let mut conn = RtspConnection::connect(addr, port, RAOP_USER_AGENT)?;
                device_verification::verify(&mut conn, &client_name, &kept)
            })
            .await
            .map_err(CastError::Internal)?;
        match verified {
            Ok(()) => Ok(KeptVerified::Accepted),
            Err(VerificationError::NoLongerKnown) => Ok(KeptVerified::Lost),
            Err(error) => Err(raop_pairing_failure(error, &receiver.name)),
        }
    }

    async fn show_pin(&self, cast: &CastController) -> Result<(), CastError> {
        let PairingTarget { addr, port, client_name, receiver } = self.0.clone();
        cast.diagnostics
            .blocking("ask an AirPlay 1 receiver to show a PIN", move || {
                let mut conn = RtspConnection::connect(addr, port, RAOP_USER_AGENT)?;
                device_verification::show_pin(&mut conn, &client_name)
            })
            .await
            .map_err(CastError::Internal)?
            .map_err(|error| raop_pairing_failure(error, &receiver.name))
    }

    /// Device verification asks only for a PIN, and a wrong one states no
    /// wait.
    async fn try_code(
        &self,
        cast: &CastController,
        _code: CodeKind,
        entered: String,
        kept_for: KeptFor,
    ) -> Result<CodeTried<VerificationKey>, CastError> {
        let PairingTarget { addr, port, client_name, receiver } = self.0.clone();
        let entry = self.kept_entry();
        let secrets = cast.device_secrets.clone();
        cast.diagnostics
            .blocking("pair with an AirPlay 1 receiver by PIN", move || {
                let key = VerificationKey::new();
                let paired = RtspConnection::connect(addr, port, RAOP_USER_AGENT)
                    .map_err(VerificationError::from)
                    .and_then(|mut conn| {
                        device_verification::pair_with_pin(&mut conn, &client_name, &entered, &key)
                    });
                match paired {
                    Ok(()) => {}
                    Err(VerificationError::WrongPin) => {
                        return Ok(CodeTried::Wrong { retry_after: None });
                    }
                    Err(error) => return Err(raop_pairing_failure(error, &receiver.name)),
                }
                keep(&secrets, &entry, &key.encode(), kept_for)?;
                Ok(CodeTried::Paired(key))
            })
            .await
            .map_err(CastError::Internal)?
    }
}

impl PairsByCode for RaopPasswordReceiver {
    /// The password, which is the keychain entry's whole text.
    type Kept = String;

    fn kept_entry(&self) -> String {
        password_secret_name(&self.target.receiver.id)
    }

    fn decode_kept(text: &str) -> Result<String, MalformedPairingKeys> {
        Ok(text.to_string())
    }

    async fn verify_kept(
        &self,
        cast: &CastController,
        kept: String,
    ) -> Result<KeptVerified, CastError> {
        let accepted = self.accepts(cast, kept).await?;
        Ok(if accepted { KeptVerified::Accepted } else { KeptVerified::Lost })
    }

    /// A password is no PIN, so the receiver shows nothing.
    async fn show_pin(&self, _cast: &CastController) -> Result<(), CastError> {
        Ok(())
    }

    /// The receiver takes the password when no request of the check gets a
    /// 401 after the answer, and a wrong one states no wait.
    async fn try_code(
        &self,
        cast: &CastController,
        _code: CodeKind,
        entered: String,
        kept_for: KeptFor,
    ) -> Result<CodeTried<String>, CastError> {
        let entry = self.kept_entry();
        let secrets = cast.device_secrets.clone();
        if !self.accepts(cast, entered.clone()).await? {
            return Ok(CodeTried::Wrong { retry_after: None });
        }
        // Once the receiver took the password, the keychain write runs to its
        // end even when the cast is called off meanwhile.
        cast.diagnostics
            .blocking("keep an AirPlay 1 receiver's password", move || {
                keep(&secrets, &entry, &entered, kept_for)?;
                Ok(CodeTried::Paired(entered))
            })
            .await
            .map_err(CastError::Internal)?
    }
}

impl RaopPasswordReceiver {
    /// Whether the receiver takes `password`, asked on a connection of its
    /// own, off the runtime.
    async fn accepts(&self, cast: &CastController, password: String) -> Result<bool, CastError> {
        let PairingTarget { addr, port, receiver, .. } = self.target.clone();
        let password = RaopPassword { password, style: self.style };
        let verification = self.verification.clone();
        cast.diagnostics
            .blocking("check an AirPlay 1 receiver's password", move || {
                password_accepted(addr, port, verification.as_ref(), &password)
                    .map_err(|error| raop_session_failure(error, &receiver.name))
            })
            .await
            .map_err(CastError::Internal)?
    }
}

/// Write `text` to this device's keychain entry `entry` when the pairing is
/// kept for later casts. A write the keychain refuses fails the cast.
fn keep(
    secrets: &coven::Coven,
    entry: &str,
    text: &str,
    kept_for: KeptFor,
) -> Result<(), CastError> {
    match kept_for {
        KeptFor::LaterCasts => {
            secrets.set_device_secret(entry, text).map_err(CastError::PairingKeys)
        }
        KeptFor::ThisCast => Ok(()),
    }
}

/// What a failed device verification with the AirPlay 1 receiver named
/// `receiver` says to the person who cast: a connection that failed is
/// unreachable, and anything else is a pairing that didn't complete, since
/// device verification names no reason.
pub(crate) fn raop_pairing_failure(error: VerificationError, receiver: &str) -> CastError {
    let receiver = receiver.to_string();
    match error {
        VerificationError::Io(error) if error.kind() != std::io::ErrorKind::InvalidData => {
            CastError::AirPlayUnreachable { receiver, detail: error.to_string() }
        }
        error @ (VerificationError::Io(_)
        | VerificationError::Rejected { .. }
        | VerificationError::WrongPin
        | VerificationError::NoLongerKnown
        | VerificationError::Plist { .. }
        | VerificationError::BadReply { .. }
        | VerificationError::Srp(_)) => {
            CastError::AirPlayPairingFailed { receiver, detail: error.to_string() }
        }
    }
}

/// What a failed connection to the AirPlay 1 receiver named `receiver` says
/// to the person who cast: a failed device verification as it says, a
/// connection that failed is unreachable, and any other reply is a pairing
/// that didn't complete.
fn raop_session_failure(error: RaopError, receiver: &str) -> CastError {
    match error {
        RaopError::Verification(error) => raop_pairing_failure(error, receiver),
        RaopError::Io(error) if error.kind() != std::io::ErrorKind::InvalidData => {
            CastError::AirPlayUnreachable {
                receiver: receiver.to_string(),
                detail: error.to_string(),
            }
        }
        error @ (RaopError::Io(_)
        | RaopError::Rejected { .. }
        | RaopError::PasswordRefused
        | RaopError::MissingTransport) => CastError::AirPlayPairingFailed {
            receiver: receiver.to_string(),
            detail: error.to_string(),
        },
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
