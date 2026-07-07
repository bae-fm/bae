use super::*;

/// Preview coordination. The preview mechanics live in `PreviewPlayer`; the
/// service owns the one piece of cross-player state — pausing the main player
/// while a preview runs and resuming it after — so these handlers wrap the
/// `PreviewPlayer` calls with that coordination.
impl PlaybackService {
    /// Pause main player for preview. Called after a preview successfully starts
    /// so that a failed preview start doesn't leave the main player paused.
    pub(super) fn pause_main_for_preview(&mut self) {
        // Pause the main player only if it is actually playing (a load still
        // filling counts as playing intent). A paused/stopped main player is left
        // alone, and the resume marker stays clear so preview stop doesn't
        // spuriously resume it.
        let paused = match &mut self.slot {
            PlaybackSlot::Active(cur) if cur.phase.intent() == PlayIntent::Playing => {
                cur.phase = TrackPhase::Paused(PausePhase::Manual);
                true
            }
            _ => false,
        };
        if paused {
            self.main_was_playing_before_preview = true;
            self.sync_audio_state();
            self.emit_state();
        }
    }

    /// Resume main player if it was paused for preview.
    pub(super) fn maybe_resume_main_player(&mut self) {
        if !self.main_was_playing_before_preview {
            return;
        }
        self.main_was_playing_before_preview = false;
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

    /// Stop preview because main playback is taking over, without resuming main
    /// from the preview pause marker.
    pub(super) fn stop_preview_for_main_playback(&mut self) {
        self.main_was_playing_before_preview = false;
        self.preview.stop();
    }

    /// Preview a local file. Same path toggles off (and resumes main); a
    /// different path switches; a fresh path starts and pauses the main player.
    pub(super) async fn preview_play(&mut self, path: String) {
        // Same path: dismiss if playing/paused (and resume main); if finished,
        // clear it and replay from the top.
        if self.preview.current_path() == Some(path.as_str()) {
            if self.preview.is_finished() {
                self.preview.clear_finished();
            } else {
                self.preview_stop();
                return;
            }
        }

        // Pause the main player only once the preview has actually started, so a
        // failed preview start never leaves the main player paused.
        if self.preview.play(path).await {
            self.pause_main_for_preview();
        }
    }

    /// Stop any active preview and resume the main player if it was paused for it.
    pub(super) fn preview_stop(&mut self) {
        self.preview.stop();
        self.maybe_resume_main_player();
    }

    /// Natural preview completion: stop the preview and resume the main player.
    pub(super) fn preview_completed(&mut self) {
        info!("Preview finished");
        self.preview.stop();
        self.maybe_resume_main_player();
    }

    /// Toggle pause/resume on the active preview. A finished preview restarts
    /// from the top (which re-pauses the main player).
    pub(super) async fn preview_toggle_pause(&mut self) {
        let Some(path) = self.preview.current_path().map(str::to_string) else {
            return;
        };
        if self.preview.is_finished() {
            self.preview_play(path).await;
            return;
        }
        self.preview.toggle_pause();
    }
}
