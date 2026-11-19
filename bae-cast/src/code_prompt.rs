//! What a cast asks the person who cast for when an AirPlay receiver wants a
//! PIN or a password or holds the cast for its owner, and the prompt the app
//! answers it through.

use bae_util::task_failure::Bug;

/// How many digits every AirPlay PIN has (pyatv pads to four, and tvOS logs
/// `passcodeLength=4`).
pub(crate) const AIRPLAY_PIN_DIGITS: u8 = 4;

/// The code a receiver asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeKind {
    /// The PIN the receiver shows, `digits` digits long.
    Pin { digits: u8 },
    /// The password set on the receiver.
    Password,
}

impl CodeKind {
    /// Whether `code` has this kind's shape: exactly `digits` ASCII digits,
    /// leading zeros kept, or a password with a character.
    pub(crate) fn fits(self, code: &str) -> bool {
        match self {
            CodeKind::Pin { digits } => {
                code.len() == usize::from(digits) && code.bytes().all(|byte| byte.is_ascii_digit())
            }
            CodeKind::Password => !code.is_empty(),
        }
    }
}

/// Why bae asks for the code now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeAsked {
    /// bae has no pairing with the receiver.
    FirstPairing,
    /// The receiver asks for a PIN on every cast.
    EveryCast,
    /// The receiver refused the pairing bae kept for it.
    PairingLost,
}

/// What was wrong with the last code the person entered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeRefused {
    /// The receiver didn't take it. A receiver that shows a PIN shows a new
    /// one.
    WrongCode,
    /// It didn't have the code's shape, so bae sent it nowhere.
    WrongShape,
}

/// What a receiver asks the person for, and why bae asks now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodeRequest {
    pub code: CodeKind,
    pub why: CodeAsked,
    pub last_try: Option<CodeRefused>,
}

/// Asks the person who cast for the code a receiver wants.
#[async_trait::async_trait]
pub trait CodePrompt: Send + Sync {
    /// The code the person entered for `request`. A cast that ends while
    /// this waits drops the future, which closes what asks. An `Err` is a
    /// bug in the prompt, reported where it was caught.
    async fn code(&self, request: CodeRequest) -> Result<String, Bug>;

    /// Tell the person whether the receiver holds the cast while its owner
    /// decides whether to accept bae: `true` once it has held bae's request
    /// for five seconds, `false` once it answers. A cast that ends meanwhile
    /// drops the future. An `Err` is a bug in the prompt, reported where it
    /// was caught.
    async fn owner_deciding(&self, deciding: bool) -> Result<(), Bug>;
}

#[cfg(test)]
#[path = "code_prompt_tests.rs"]
mod tests;
