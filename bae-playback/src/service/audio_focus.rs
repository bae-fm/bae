use super::*;

impl PlaybackService {
    /// The gate every start of playback passes, before it changes anything:
    /// whether the system lets bae make sound now. A refusal — a call holds
    /// the speaker — is logged with the system's reason.
    pub(super) fn audio_focus_granted(&self) -> bool {
        match self.audio_focus.request() {
            Ok(()) => true,
            Err(refused) => {
                warn!("the system refused audio focus; staying paused: {refused}");
                false
            }
        }
    }

    /// Where a start lands once the gate has answered: `target` itself, or
    /// paused when a start the system refuses would have played.
    pub(super) fn admit(&self, target: PlayTarget) -> PlayTarget {
        match target {
            PlayTarget::Playing if !self.audio_focus_granted() => {
                PlayTarget::Paused(PausePhase::Manual)
            }
            target => target,
        }
    }
}
