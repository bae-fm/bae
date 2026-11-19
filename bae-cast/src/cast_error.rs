//! The ways starting a cast fails, and the class a surface words each in.

use bae_util::task_failure::TaskFailed;

use crate::airplay::UnreadableSetting;

/// A failure starting a cast session. Each AirPlay receiver's refusal names
/// the receiver, as discovery found it, for the line that tells the person.
#[derive(Debug, thiserror::Error)]
pub enum CastError {
    /// Casting is turned off in settings, so no session may be started.
    #[error("casting is turned off in settings")]
    Disabled,
    /// No discovered device matches the requested id.
    #[error("no such Cast device")]
    DeviceNotFound,
    /// The control channel to the device couldn't be opened.
    #[error("couldn't connect to the Cast device: {0}")]
    Connect(String),
    /// The ephemeral serving couldn't be started (bind / no LAN address).
    #[error("couldn't start serving audio to the Cast device: {0}")]
    Serving(String),
    /// The AirPlay receiver refused to pair, for a reason it gave or a reply
    /// that didn't check out.
    #[error("couldn't pair with {receiver}: {detail}")]
    AirPlayPairingFailed { receiver: String, detail: String },
    /// The AirPlay receiver saw too many failed pairing attempts and refuses
    /// more until it is reset.
    #[error("{receiver} refuses more PIN attempts")]
    AirPlayLockedOut { receiver: String },
    /// The AirPlay receiver is pairing with another device.
    #[error("{receiver} is pairing with another device")]
    AirPlayBusy { receiver: String },
    /// The AirPlay receiver asks to wait before another try.
    #[error("{receiver} asks to wait {seconds} seconds before another try")]
    AirPlayBackOff { receiver: String, seconds: u64 },
    /// The AirPlay receiver isn't accepting new pairings.
    #[error("{receiver} isn't accepting new pairings")]
    AirPlayNotPairing { receiver: String },
    /// The AirPlay receiver is paired with as many devices as it allows.
    #[error("{receiver} is paired with as many devices as it allows")]
    AirPlayFull { receiver: String },
    /// The AirPlay receiver couldn't be reached: the connection failed, timed
    /// out, or dropped while pairing.
    #[error("couldn't reach {receiver}: {detail}")]
    AirPlayUnreachable { receiver: String, detail: String },
    /// The AirPlay receiver plays only from people in its home.
    #[error("{receiver} plays only from people in its home")]
    AirPlayHomeOnly { receiver: String },
    /// The AirPlay receiver plays only from its home's admins.
    #[error("{receiver} plays only from its home's admins")]
    AirPlayAdminsOnly { receiver: String },
    /// The AirPlay receiver plays only from the account it is set up with, or
    /// a Mac's AirPlay Receiver only for its current user.
    #[error("{receiver} plays only from the account it is set up with")]
    AirPlayOwnerOnly { receiver: String },
    /// The RAOP receiver only offers audio encryption the sender can't provide.
    #[error("this AirPlay receiver requires an encryption the sender can't provide")]
    AirPlayEncryptionUnsupported,
    /// The AirPlay receiver advertises a setting bae can't read, so how to
    /// connect to it can't be decided.
    #[error("this AirPlay receiver advertises settings bae can't read: {0}")]
    AirPlayUnreadableSettings(UnreadableSetting),
    /// The keychain refused to read or write this device's pairing with an
    /// AirPlay receiver.
    #[error("couldn't read or keep the AirPlay pairing: {0}")]
    PairingKeys(coven::KeyError),
    /// This device's kept pairing with an AirPlay receiver is text bae
    /// didn't write.
    #[error("{0}")]
    KeptPairingUnreadable(String),
    /// Starting the session failed in bae itself: work it ran panicked, a bug
    /// reported where it was caught.
    #[error("{0}")]
    Internal(TaskFailed),
    /// Whether casting is on couldn't be read.
    #[error("couldn't read whether casting is on: {0}")]
    Setting(bae_store::DbError),
    /// This device's name, which an AirPlay receiver shows while it pairs,
    /// couldn't be read.
    #[error("couldn't read this device's name: {0}")]
    DeviceName(bae_store::DbError),
    /// Casting stopped, or was turned off, or another cast started, while
    /// this one connected. Playback stayed where it was.
    #[error("the cast was called off before it started")]
    CalledOff,
}

impl CastError {
    /// The class a surface words this failure in. An AirPlay receiver the
    /// sender can't drive, that plays only from some people, that refuses to
    /// pair, or whose settings can't be read, has its own line, which names
    /// the receiver where it refused; a keychain that refused the pairing is
    /// the keychain's; a setting or name that couldn't be read is the
    /// library's database; anything else is a fault the person can only
    /// report.
    pub fn category(&self) -> bae_diagnostics::UiErrorCategory {
        use bae_diagnostics::UiErrorCategory as C;
        match self {
            Self::AirPlayEncryptionUnsupported => C::AirPlayUnsupported,
            Self::AirPlayPairingFailed { receiver, .. } => {
                C::AirPlayPairingFailed { receiver: receiver.clone() }
            }
            Self::AirPlayLockedOut { receiver } => {
                C::AirPlayLockedOut { receiver: receiver.clone() }
            }
            Self::AirPlayBusy { receiver } => C::AirPlayBusy { receiver: receiver.clone() },
            Self::AirPlayBackOff { receiver, seconds } => {
                C::AirPlayBackOff { receiver: receiver.clone(), seconds: *seconds }
            }
            Self::AirPlayNotPairing { receiver } => {
                C::AirPlayNotPairing { receiver: receiver.clone() }
            }
            Self::AirPlayFull { receiver } => C::AirPlayFull { receiver: receiver.clone() },
            Self::AirPlayUnreachable { receiver, .. } => {
                C::AirPlayUnreachable { receiver: receiver.clone() }
            }
            Self::AirPlayHomeOnly { receiver } => C::AirPlayHomeOnly { receiver: receiver.clone() },
            Self::AirPlayAdminsOnly { receiver } => {
                C::AirPlayAdminsOnly { receiver: receiver.clone() }
            }
            Self::AirPlayOwnerOnly { receiver } => {
                C::AirPlayOwnerOnly { receiver: receiver.clone() }
            }
            Self::AirPlayUnreadableSettings(_) => C::AirPlayUnreadableSettings,
            Self::PairingKeys(error) => bae_diagnostics::key_category(error),
            Self::Setting(error) | Self::DeviceName(error) => {
                bae_diagnostics::database_category(error)
            }
            Self::Disabled
            | Self::DeviceNotFound
            | Self::Connect(_)
            | Self::Serving(_)
            | Self::KeptPairingUnreadable(_)
            | Self::Internal(_)
            | Self::CalledOff => C::Internal,
        }
    }
}
