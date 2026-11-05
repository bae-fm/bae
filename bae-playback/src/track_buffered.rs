//! The parts of the playing track whose bytes this device holds, as the
//! players' seek bar draws them under the played part.
//!
//! A track's bytes are a stretch of its file, held in that file's buffer
//! with every other track the file plays. The buffer tells the byte ranges
//! it holds; the track's byte span and its scrub points place each byte in
//! the track, so each held range becomes a stretch of the track's length.

use std::ops::Range;

use tokio::sync::watch;

use bae_audio::sparse_buffer::HeldRangesWatch;
use bae_records::{ScrubPoint, SegmentSpan};

/// A stretch of a track, as fractions of its length from INDEX 01: the unit
/// of a position's `progress`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrackSpan {
    pub start: f64,
    pub end: f64,
}

/// The parts of the playing track whose bytes this device holds, sorted and
/// none touching: none while none are here, one span from 0 to 1 once all
/// are.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackBuffered {
    pub track_id: String,
    pub ranges: Vec<TrackSpan>,
}

/// Where a track's bytes sit in its file, and how far into the track each
/// one plays: its first byte at the start, its last at the end, and its
/// scrub points between, with the bytes between two of them spread evenly
/// over the time between.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TrackBytes {
    /// Each known byte and the fraction of the track it plays at: bytes
    /// rising, fractions never falling, from the span's first byte at 0 to
    /// its end at 1. Empty for a span with no bytes.
    anchors: Vec<(u64, f64)>,
}

impl TrackBytes {
    /// The bytes of the segment that plays the track from INDEX 01 to its
    /// end: `span` in a file of `file_size` bytes, `samples` long, with its
    /// file's scrub `points`.
    pub(crate) fn new(
        span: SegmentSpan,
        file_size: u64,
        points: &[ScrubPoint],
        samples: u64,
    ) -> Self {
        let start = span.start_byte.unwrap_or(0);
        let end = span.end_byte.unwrap_or(file_size);
        if end <= start {
            return Self { anchors: Vec::new() };
        }
        let mut anchors = vec![(start, 0.0)];
        if samples > 0 {
            for point in points {
                let (last_byte, last_fraction) = *anchors.last().expect("starts with one");
                let into = point.sample.saturating_sub(span.start_sample);
                let fraction = (into as f64 / samples as f64).min(1.0);
                if point.byte > last_byte && point.byte < end && fraction >= last_fraction {
                    anchors.push((point.byte, fraction));
                }
            }
        }
        anchors.push((end, 1.0));
        Self { anchors }
    }

    /// The stretches of the track `held`, a file's byte ranges sorted and
    /// none touching, hold.
    pub(crate) fn spans(&self, held: &[Range<u64>]) -> Vec<TrackSpan> {
        let (Some(&(first, _)), Some(&(end, _))) = (self.anchors.first(), self.anchors.last())
        else {
            return Vec::new();
        };
        let mut spans: Vec<TrackSpan> = Vec::new();
        for range in held {
            let (from, to) = (range.start.max(first), range.end.min(end));
            if from >= to {
                continue;
            }
            let (start, end) = (self.fraction_at(from), self.fraction_at(to));
            if end <= start {
                continue;
            }
            match spans.last_mut() {
                Some(last) if start <= last.end => last.end = last.end.max(end),
                _ => spans.push(TrackSpan { start, end }),
            }
        }
        spans
    }

    /// The fraction of the track `byte`, inside its span, plays at.
    fn fraction_at(&self, byte: u64) -> f64 {
        let after = self.anchors.partition_point(|&(anchor, _)| anchor <= byte);
        let Some(&(next_byte, next_fraction)) = self.anchors.get(after) else {
            return 1.0;
        };
        let (byte_before, fraction_before) = self.anchors[after - 1];
        let along = (byte - byte_before) as f64 / (next_byte - byte_before) as f64;
        fraction_before + along * (next_fraction - fraction_before)
    }
}

/// The playing track's bytes and the buffer holding them.
#[derive(Clone)]
pub(crate) struct PlayingBytes {
    track_id: String,
    bytes: TrackBytes,
    /// The byte ranges its file's buffer holds, as they change.
    held: HeldRangesWatch,
}

impl PlayingBytes {
    /// `track_id`'s `bytes`, in the buffer whose ranges `held` hears.
    pub(crate) fn new(track_id: String, bytes: TrackBytes, held: HeldRangesWatch) -> Self {
        Self { track_id, bytes, held }
    }
}

/// The playing track's held stretches as they change, for a feed to write.
pub struct TrackBufferedFollow {
    playing: watch::Receiver<Option<PlayingBytes>>,
    /// The playing track's buffer while it runs.
    held: Option<HeldRangesWatch>,
    started: bool,
}

/// What [`TrackBufferedFollow::next`] woke to.
enum Woke {
    AnotherTrack,
    Held,
    BufferGone,
}

impl TrackBufferedFollow {
    pub(crate) fn new(playing: watch::Receiver<Option<PlayingBytes>>) -> Self {
        Self { playing, held: None, started: false }
    }

    /// The playing track's held stretches now, then each time they change
    /// or another track plays: `Some(None)` while no track plays, and `None`
    /// once playback has stopped running.
    pub async fn next(&mut self) -> Option<Option<TrackBuffered>> {
        if !std::mem::replace(&mut self.started, true) {
            return Some(self.follow_playing());
        }
        let woke = match &mut self.held {
            Some(held) => tokio::select! {
                changed = self.playing.changed() => {
                    changed.ok()?;
                    Woke::AnotherTrack
                }
                changed = held.changed() => match changed {
                    Ok(()) => Woke::Held,
                    Err(_) => Woke::BufferGone,
                },
            },
            None => {
                self.playing.changed().await.ok()?;
                Woke::AnotherTrack
            }
        };
        Some(match woke {
            Woke::AnotherTrack => self.follow_playing(),
            Woke::Held => self.current(),
            // A buffer drops its bytes with it.
            Woke::BufferGone => {
                self.held = None;
                self.current()
            }
        })
    }

    /// Follow the track playing now, and answer what it holds.
    fn follow_playing(&mut self) -> Option<TrackBuffered> {
        self.held = self.playing.borrow_and_update().as_ref().map(|playing| playing.held.clone());
        self.current()
    }

    fn current(&mut self) -> Option<TrackBuffered> {
        let playing = self.playing.borrow();
        let playing = playing.as_ref()?;
        let ranges = match &mut self.held {
            Some(held) => playing.bytes.spans(&held.held()),
            None => Vec::new(),
        };
        Some(TrackBuffered { track_id: playing.track_id.clone(), ranges })
    }
}

#[cfg(test)]
#[path = "track_buffered_tests.rs"]
mod tests;
