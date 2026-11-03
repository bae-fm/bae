//! Whether the system lets bae make sound: the gate every start of playback
//! passes.

use std::sync::Arc;

/// How the platform shares the speaker between apps.
#[derive(Clone)]
pub enum AudioFocus {
    /// The desktop: any app plays without asking.
    Unshared,
    /// Android's audio focus and iOS's audio session: one app plays at a time,
    /// and the system refuses while something that outranks music, such as a
    /// phone call, holds the speaker.
    Shared(Arc<dyn AudioFocusSystem>),
}

/// The platform's side of [`AudioFocus::Shared`]. The playback service calls
/// both methods on its own thread.
pub trait AudioFocusSystem: Send + Sync {
    /// Ask for the speaker before playback starts. Asking while already
    /// holding it is granted again.
    fn request(&self) -> Result<(), AudioFocusRefused>;
    /// Give the speaker back once playback stops, so another app can resume.
    fn release(&self);
}

/// The system's refusal, in its own words, for the log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioFocusRefused(pub String);

impl std::fmt::Display for AudioFocusRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl AudioFocus {
    /// Ask before a start; the desktop always lets it play.
    pub(crate) fn request(&self) -> Result<(), AudioFocusRefused> {
        match self {
            Self::Unshared => Ok(()),
            Self::Shared(system) => system.request(),
        }
    }

    /// Give the speaker back after playback stops; nothing on the desktop.
    pub(crate) fn release(&self) {
        match self {
            Self::Unshared => {}
            Self::Shared(system) => system.release(),
        }
    }
}
