use super::*;
use crate::playback::preview_player::AfterPreview;

/// Preview coordination. The preview mechanics live in `PreviewPlayer` — which
/// also records whether starting a preview paused the main player — and the
/// service owns the main player, so these handlers do the pausing and the resume
/// the preview asks for when it ends.
impl PlaybackService {
    /// Pause main player for preview. Called after a preview successfully starts
    /// so that a failed preview start doesn't leave the main player paused.
    fn pause_main_for_preview(&mut self) {
        // Pause the main player only if it is actually playing (a load still
        // filling counts as playing intent). A paused/stopped main player is left
        // alone, and the preview is told nothing, so its end doesn't spuriously
        // resume it.
        let paused = match &mut self.slot {
            PlaybackSlot::Active(cur) if cur.phase.intent() == PlayIntent::Playing => {
                cur.phase = TrackPhase::Paused(PausePhase::Manual);
                true
            }
            _ => false,
        };
        if paused {
            self.preview.main_player_paused();
            self.sync_audio_state();
            self.emit_state();
        }
    }

    /// Resume the main player the ended preview had paused.
    fn resume_main_player(&mut self) {
        let resumed = match &mut self.slot {
            PlaybackSlot::Active(cur) => {
                cur.phase = TrackPhase::Playing;
                true
            }
            _ => false,
        };
        if resumed {
            self.sync_audio_state();
            self.emit_state();
        }
    }

    /// Stop preview because main playback is taking over. Main starts itself, so
    /// the resume the preview hands back is dropped rather than acted on.
    pub(super) fn stop_preview_for_main_playback(&mut self) {
        let _ = self.preview.stop();
    }

    /// Preview a local source window. The same target toggles off (and resumes
    /// main); a different target switches and pauses the main player.
    pub(super) async fn preview_play(&mut self, target: crate::playback::PreviewTarget) {
        if self.preview.current_target() == Some(&target) {
            self.preview_stop();
            return;
        }

        // Pause the main player only once the preview has actually started, so a
        // failed preview start never leaves the main player paused. A failure
        // ships the matching operation: the file couldn't be prepared, or the
        // audio stream couldn't be built.
        use crate::playback::preview_player::PreviewPlayOutcome;
        match self.preview.play(target, self.audio_device.as_ref()).await {
            PreviewPlayOutcome::Started => self.pause_main_for_preview(),
            PreviewPlayOutcome::SetupFailed => {
                self.telemetry_playback_failed(PlaybackOperation::Preview);
            }
            PreviewPlayOutcome::StreamStartFailed => {
                self.telemetry_playback_failed(PlaybackOperation::StreamStart);
            }
        }
    }

    /// Stop any active preview and resume the main player if it was paused for it.
    pub(super) fn preview_stop(&mut self) {
        if self.preview.stop() == AfterPreview::ResumeMain {
            self.resume_main_player();
        }
    }

    /// Natural preview completion: stop the preview and resume the main player.
    pub(super) fn preview_completed(&mut self) {
        info!("Preview finished");
        self.preview_stop();
    }

    /// Toggle pause/resume on the active preview.
    pub(super) fn preview_toggle_pause(&mut self) {
        self.preview.toggle_pause();
    }
}
