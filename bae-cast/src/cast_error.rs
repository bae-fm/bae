//! The ways starting a cast fails, and the class a surface words each in.

use bae_util::task_failure::TaskFailed;

use crate::airplay::UnreadableSetting;

/// A failure starting a cast session.
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
    /// The AirPlay receiver asks for a PIN, a password or device verification,
    /// which the sender doesn't do.
    #[error("this AirPlay receiver needs a PIN, which isn't supported")]
    AirPlayPinRequired,
    /// The AirPlay receiver refused to pair, for a reason it gave or a reply
    /// that didn't check out.
    #[error("couldn't pair with this AirPlay receiver: {0}")]
    AirPlayPairingFailed(String),
    /// The AirPlay receiver saw too many failed pairing attempts and refuses
    /// more until it is reset.
    #[error("this AirPlay receiver refuses more PIN attempts")]
    AirPlayLockedOut,
    /// The AirPlay receiver is pairing with another device.
    #[error("this AirPlay receiver is pairing with another device")]
    AirPlayBusy,
    /// The AirPlay receiver asks to wait before another try. bae doesn't wait
    /// or try again.
    #[error("this AirPlay receiver asks to wait {seconds} seconds before another try")]
    AirPlayBackOff { seconds: u64 },
    /// The AirPlay receiver isn't accepting new pairings.
    #[error("this AirPlay receiver isn't accepting new pairings")]
    AirPlayNotPairing,
    /// The AirPlay receiver is paired with as many devices as it allows.
    #[error("this AirPlay receiver is paired with as many devices as it allows")]
    AirPlayFull,
    /// The AirPlay receiver couldn't be reached: the connection failed, timed
    /// out, or dropped while pairing.
    #[error("couldn't reach this AirPlay receiver: {0}")]
    AirPlayUnreachable(String),
    /// The AirPlay receiver plays only from people in its home.
    #[error("this AirPlay receiver plays only from people in its home")]
    AirPlayHomeOnly,
    /// The AirPlay receiver plays only from its home's admins.
    #[error("this AirPlay receiver plays only from its home's admins")]
    AirPlayAdminsOnly,
    /// The AirPlay receiver plays only from the account it is set up with.
    #[error("this AirPlay receiver plays only from the account it is set up with")]
    AirPlayOwnerOnly,
    /// The RAOP receiver only offers audio encryption the sender can't provide.
    #[error("this AirPlay receiver requires an encryption the sender can't provide")]
    AirPlayEncryptionUnsupported,
    /// The AirPlay receiver advertises a setting bae can't read, so how to
    /// connect to it can't be decided.
    #[error("this AirPlay receiver advertises settings bae can't read: {0}")]
    AirPlayUnreadableSettings(UnreadableSetting),
    /// Starting the session failed in bae itself: work it ran panicked, a bug
    /// reported where it was caught.
    #[error("{0}")]
    Internal(TaskFailed),
    /// Whether casting is on couldn't be read.
    #[error("couldn't read whether casting is on: {0}")]
    Setting(bae_store::DbError),
    /// Casting stopped, or was turned off, or another cast started, while
    /// this one connected. Playback stayed where it was.
    #[error("the cast was called off before it started")]
    CalledOff,
}

impl CastError {
    /// The class a surface words this failure in. An AirPlay receiver the
    /// sender can't drive, that plays only from some people, or whose settings
    /// can't be read, has its own line;
    /// a setting that couldn't be read is the library's database; anything
    /// else is a fault the person can only report.
    pub fn category(&self) -> bae_diagnostics::UiErrorCategory {
        use bae_diagnostics::UiErrorCategory as C;
        match self {
            Self::AirPlayPinRequired | Self::AirPlayEncryptionUnsupported => C::AirPlayUnsupported,
            Self::AirPlayPairingFailed(_) => C::AirPlayPairingFailed,
            Self::AirPlayLockedOut => C::AirPlayLockedOut,
            Self::AirPlayBusy => C::AirPlayBusy,
            Self::AirPlayBackOff { seconds } => C::AirPlayBackOff { seconds: *seconds },
            Self::AirPlayNotPairing => C::AirPlayNotPairing,
            Self::AirPlayFull => C::AirPlayFull,
            Self::AirPlayUnreachable(_) => C::AirPlayUnreachable,
            Self::AirPlayHomeOnly => C::AirPlayHomeOnly,
            Self::AirPlayAdminsOnly => C::AirPlayAdminsOnly,
            Self::AirPlayOwnerOnly => C::AirPlayOwnerOnly,
            Self::AirPlayUnreadableSettings(_) => C::AirPlayUnreadableSettings,
            Self::Setting(error) => bae_diagnostics::database_category(error),
            Self::Disabled
            | Self::DeviceNotFound
            | Self::Connect(_)
            | Self::Serving(_)
            | Self::Internal(_)
            | Self::CalledOff => C::Internal,
        }
    }
}
