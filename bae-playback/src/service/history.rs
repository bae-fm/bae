use super::*;
use bae_people::history::HistoryPlay;
use std::time::Duration;

pub(super) struct Listening {
    play: HistoryPlay,
    meter: ListeningMeter,
    saved: Duration,
}

pub(super) struct BufferedListening {
    listening: Listening,
    tail: Option<AudibleTail>,
}

struct AudibleTail {
    fmt: Arc<TrackFmt>,
    position: StreamPosition,
    finishes_at: chrono::DateTime<chrono::Utc>,
    /// When the play comes to count as played partway through the tail, and
    /// so is due a save: the tail is heard at the rate it plays. `None` when
    /// it already counted, or counts only at the tail's end or not at all.
    /// Cleared once passed.
    qualifies_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl AudibleTail {
    /// The next moment this tail makes a save due.
    fn next_due(&self) -> chrono::DateTime<chrono::Utc> {
        self.qualifies_at.map_or(self.finishes_at, |at| at.min(self.finishes_at))
    }
}

impl BufferedListening {
    fn advance(&mut self, now: chrono::DateTime<chrono::Utc>) {
        let Some(tail) = &self.tail else {
            return;
        };
        let remaining = if now < tail.finishes_at {
            (tail.finishes_at - now)
                .to_std()
                .expect("a future audio deadline has positive duration")
        } else {
            Duration::ZERO
        };
        self.listening.observe_local(tail.fmt.clone(), tail.position.saturating_sub(remaining));
        if remaining.is_zero() {
            self.tail = None;
        } else if let Some(tail) = &mut self.tail {
            tail.qualifies_at = tail.qualifies_at.filter(|at| *at > now);
        }
    }
}

/// Which plays a save writes.
#[derive(Clone, Copy)]
enum ListeningSave {
    /// Every play whose played time changed since its last save.
    Changed,
    /// Only plays that newly count as played or whose audio has finished.
    Due,
}

enum ListeningMeter {
    Local(Option<(Arc<TrackFmt>, Duration)>),
    Remote { position: StreamPosition, reported_at: chrono::DateTime<chrono::Utc> },
}

impl Listening {
    fn observe_local(&mut self, fmt: Arc<TrackFmt>, position: StreamPosition) {
        if fmt.track_id != self.play.entry().track_id {
            return;
        }
        // Count only the track body; generated and stored pregaps precede INDEX 01.
        let start = fmt.starts_at.max(fmt.timeline.track_start()).as_duration();
        let heard = position.as_duration().saturating_sub(start);
        let previous = match &self.meter {
            ListeningMeter::Local(Some((previous, amount))) if Arc::ptr_eq(previous, &fmt) => {
                *amount
            }
            _ => Duration::ZERO,
        };
        self.play.set_played(self.play.entry().played + heard.saturating_sub(previous));
        self.meter = ListeningMeter::Local(Some((fmt, heard.max(previous))));
    }
}

impl PlaybackService {
    pub(super) async fn begin_listening(&mut self) -> bool {
        if self.listening.is_some() {
            return true;
        }
        let PlaybackSlot::Active(cur) = &self.slot else {
            return true;
        };
        if !matches!(cur.phase, TrackPhase::Playing) {
            return true;
        }
        let track = cur.prepared.track_id.clone();
        let position = cur.position;
        let duration = cur.prepared.timeline.duration();
        let now = self.clock.now();
        match self.people.begin_play(self.ids.new_id(), &track, duration, now).await {
            // The person keeps no listening history: nothing of this play is metered.
            Ok(None) => true,
            Ok(Some(play)) => {
                let meter = if self.renderer.is_remote() {
                    ListeningMeter::Remote { position, reported_at: now }
                } else {
                    ListeningMeter::Local(None)
                };
                self.listening = Some(Listening { play, meter, saved: Duration::ZERO });
                true
            }
            Err(error) => {
                self.listening_failed(error.into());
                false
            }
        }
    }

    pub(super) fn capture_listening_position(&mut self) {
        let Some(listening) = &mut self.listening else {
            return;
        };
        if self.renderer.is_remote() {
            return;
        }
        if let Some(output) = &self.output {
            let (fmt, mut position) = output.source.lock().unwrap().position_event();
            if let Some(latency) = self.renderer.airplay_latency() {
                position = position.saturating_sub(latency);
            }
            listening.observe_local(fmt, position);
        }
    }

    pub(super) fn observe_listening_position(
        &mut self,
        fmt: Arc<TrackFmt>,
        position: StreamPosition,
    ) {
        if let Some(listening) = &mut self.listening {
            listening.observe_local(fmt, position);
        }
    }

    pub(super) fn finish_listening_audio(&mut self, fmt: Arc<TrackFmt>, samples: u64) {
        let PlaybackSlot::Active(cur) = &self.slot else {
            return;
        };
        let Some(listening) = &mut self.listening else {
            return;
        };
        if listening.play.entry().track_id != fmt.track_id {
            return;
        }
        let decoded = bae_util::sample_time::frame_start(
            samples / u64::from(cur.prepared.channels),
            cur.prepared.sample_rate,
        );
        let position = fmt.starts_at + decoded;
        listening.play.finish_at(
            position.as_duration().saturating_sub(fmt.timeline.track_start().as_duration()),
        );
        if let Some(latency) = self.renderer.airplay_latency() {
            listening.observe_local(fmt.clone(), position.saturating_sub(latency));
            let listening = self.listening.take().expect("the finishing play is present");
            let now = self.clock.now();
            let finishes_at = now
                + chrono::Duration::from_std(latency)
                    .expect("receiver latency fits a clock deadline");
            // A tail that starts before the track body is heard slower at
            // first, so this can come early; the save is then due at the end.
            let qualifies_at = listening
                .play
                .left_to_qualify()
                .and_then(|left| chrono::Duration::from_std(left).ok())
                .map(|left| now + left)
                .filter(|at| *at < finishes_at);
            self.buffered_listening.push(BufferedListening {
                listening,
                tail: Some(AudibleTail { fmt, position, finishes_at, qualifies_at }),
            });
        } else {
            listening.observe_local(fmt, position);
        }
    }

    /// The next moment audio still in an AirPlay receiver makes something due:
    /// a finished play whose audio has been heard, or comes to count as played,
    /// and saves; a drained track whose last audio has been heard, and
    /// completes. The run loop waits for it, then checkpoints and publishes.
    pub(super) fn audible_deadline(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        let completing = matches!(
            &self.slot,
            PlaybackSlot::Active(cur) if matches!(cur.phase, TrackPhase::Completed)
        );
        let completion =
            self.audible_completion.as_ref().filter(|_| completing).map(|c| c.audible_at);
        self.buffered_listening
            .iter()
            .filter_map(|buffered| buffered.tail.as_ref().map(AudibleTail::next_due))
            .chain(completion)
            .min()
    }

    /// Save plays at a boundary: pause, seek, finish. Each save is a synced
    /// library write, so audio progress between boundaries goes through
    /// `checkpoint_listening` instead.
    pub(super) async fn persist_listening(&mut self) -> bool {
        self.save_listening(ListeningSave::Changed).await
    }

    /// Save what audio progress alone makes due: a play that has just come
    /// to count as played, and a finished play whose audio has now been
    /// heard. Saving every change would write continuously while audio plays.
    pub(super) async fn checkpoint_listening(&mut self) -> bool {
        self.save_listening(ListeningSave::Due).await
    }

    async fn save_listening(&mut self, save: ListeningSave) -> bool {
        let now = self.clock.now();
        let mut failures = Vec::new();
        if let Some(listening) = &mut self.listening {
            let due = match save {
                ListeningSave::Changed => true,
                ListeningSave::Due => listening.saved.is_zero(),
            };
            if due {
                if let Err(error) = persist(&self.people, listening).await {
                    failures.push(error);
                }
            }
        }
        for buffered in &mut self.buffered_listening {
            buffered.advance(now);
            let due = match save {
                ListeningSave::Changed => true,
                ListeningSave::Due => buffered.tail.is_none() || buffered.listening.saved.is_zero(),
            };
            if due {
                if let Err(error) = persist(&self.people, &mut buffered.listening).await {
                    failures.push(error);
                }
            }
        }
        self.buffered_listening.retain(|buffered| buffered.tail.is_some());
        let succeeded = failures.is_empty();
        for error in failures {
            self.listening_failed(error.into());
        }
        succeeded
    }

    /// A receiver flush discards queued sound. Freeze each completed play at
    /// what was audible and skip the current decoder's unplayed send-ahead.
    pub(super) fn discard_buffered_listening(&mut self) {
        let now = self.clock.now();
        for buffered in &mut self.buffered_listening {
            buffered.advance(now);
            buffered.tail = None;
        }
        if !self.renderer.is_airplay() {
            return;
        }
        self.capture_listening_position();
        if let (Some(listening), Some(output)) = (&mut self.listening, &self.output) {
            let (fmt, position) = output.source.lock().unwrap().position_event();
            if fmt.track_id == listening.play.entry().track_id {
                let start = fmt.starts_at.max(fmt.timeline.track_start()).as_duration();
                let sent = position.as_duration().saturating_sub(start);
                listening.meter = ListeningMeter::Local(Some((fmt, sent)));
            }
        }
    }

    pub(super) async fn finish_listening(&mut self) {
        self.persist_listening().await;
        self.listening = None;
    }

    pub(super) fn reset_remote_listening_position(&mut self, position: StreamPosition) {
        if let Some(listening) = &mut self.listening {
            listening.meter = ListeningMeter::Remote { position, reported_at: self.clock.now() };
        }
    }

    pub(super) fn observe_remote_listening(&mut self, position: StreamPosition, playing: bool) {
        let track_start = match &self.slot {
            PlaybackSlot::Active(cur) => cur.prepared.timeline.track_start().as_duration(),
            _ => return,
        };
        let Some(listening) = &mut self.listening else {
            return;
        };
        let now = self.clock.now();
        if let ListeningMeter::Remote { position: previous, reported_at } = listening.meter {
            if playing {
                // A device-side seek must not turn skipped positions into listening.
                let elapsed = match (now - reported_at).to_std() {
                    Ok(elapsed) => elapsed,
                    Err(_) => {
                        warn!("playback clock moved backwards while reading remote listening");
                        Duration::ZERO
                    }
                };
                let moved = position
                    .as_duration()
                    .saturating_sub(track_start)
                    .saturating_sub(previous.as_duration().saturating_sub(track_start));
                listening.play.set_played(listening.play.entry().played + moved.min(elapsed));
            }
        }
        listening.meter = ListeningMeter::Remote { position, reported_at: now };
    }

    fn listening_failed(&self, error: PlaybackError) {
        error!("could not record listening history: {error}");
        self.fail_playback(PlaybackOperation::History, error.ui_reason());
    }
}

async fn persist(
    people: &bae_people::People,
    listening: &mut Listening,
) -> Result<(), bae_people::history::PlayRecordError> {
    if listening.play.qualifies()
        && listening.play.entry().played.as_millis() != listening.saved.as_millis()
    {
        people.record_play(&listening.play).await?;
        listening.saved = listening.play.entry().played;
    }
    Ok(())
}

#[cfg(test)]
#[path = "history_tests.rs"]
mod tests;
