//! A release's mediums and where each of its tracks sits on them.
//!
//! A release holds the mediums the person has — the discs, records or tapes
//! its audio was read from — in order, each with its carrier where the source
//! states one and its number of sides (a CD is one side). Every track sits on
//! one side of one of them, with a number on that side. These are stored
//! values, written when a source is applied; a track's label ("A1",
//! "Disc 2 · 3", "5") is a display of them.

use crate::pressing::Medium;
use std::num::{NonZeroU16, NonZeroU32, NonZeroU8};

/// One medium a release holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReleaseMedium {
    /// The carrier, where the source states one.
    pub carrier: Option<Medium>,
    pub sides: NonZeroU8,
}

impl ReleaseMedium {
    /// A medium of one side and `carrier`.
    pub fn one_sided(carrier: Option<Medium>) -> Self {
        Self { carrier, sides: NonZeroU8::MIN }
    }

    /// Whether the medium is played side by side, so its sides carry letters.
    pub fn is_sided(&self) -> bool {
        self.carrier.is_some_and(Medium::is_sided)
    }
}

/// Where a track sits: on which of its release's mediums (counted from one),
/// on which of that medium's sides (from one), and its number on that side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TrackPlace {
    pub medium: NonZeroU16,
    pub side: NonZeroU8,
    pub number: NonZeroU32,
}

impl TrackPlace {
    /// The medium's index in its release's list of mediums.
    pub fn medium_index(&self) -> usize {
        usize::from(self.medium.get()) - 1
    }
}

/// Places tracks in play order: a track whose source states a number keeps
/// it, and one whose source states none is one more than the tracks before it
/// on its own side.
#[derive(Debug, Default)]
pub struct Placer {
    on_side: std::collections::HashMap<(NonZeroU16, NonZeroU8), u32>,
}

impl Placer {
    /// The place of the next track, on `side` of medium `medium`.
    pub fn place(
        &mut self,
        medium: NonZeroU16,
        side: NonZeroU8,
        stated: Option<NonZeroU32>,
    ) -> TrackPlace {
        let before = self.on_side.entry((medium, side)).or_default();
        *before += 1;
        let number = stated.unwrap_or_else(|| {
            NonZeroU32::new(*before).expect("a side's count is at least one once a track is on it")
        });
        TrackPlace { medium, side, number }
    }
}

/// How a track's place reads ([`track_position`]): on a
/// lettered side of a record or cassette, on a numbered disc of a release of
/// several mediums, or plain on a release of one. Core renders `position_text`
/// from this; the UI only resolves the header word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrackPosition {
    /// Header "Side A", position "A1". `side_letter` names the side among the
    /// release's sided mediums (A, B, … Z, AA, …); `number` is the track's
    /// number on that side.
    Sided { side_letter: String, number: u32 },
    /// Header "Disc 2", position "3" (the header carries the disc).
    Disc { disc: u32, number: u32 },
    /// Position "5", no header.
    Flat { number: u32 },
}

/// What the UI renders as the "Side A" / "Disc 2" header; `Flat` means no header.
/// Separate from [`TrackPosition`] because a header carries no per-track number —
/// every track on a side shares one `TrackSide`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrackSide {
    Sided { side_letter: String },
    Disc { disc: u32 },
    Flat,
}

impl TrackPosition {
    /// The per-track position label: "A1", or the track's number where the
    /// disc is the group header or there is none.
    pub fn text(&self) -> String {
        match self {
            Self::Sided { side_letter, number } => format!("{side_letter}{number}"),
            // The disc is the group header; repeating it on every row is noise.
            Self::Disc { number, .. } | Self::Flat { number } => number.to_string(),
        }
    }

    /// The side the track belongs to — the grouping discriminant. Every track
    /// on a side yields the same value, so consecutive-equal grouping lands
    /// one group per side. Carries no per-track number (a header has none).
    pub fn side(&self) -> TrackSide {
        match self {
            Self::Sided { side_letter, .. } => {
                TrackSide::Sided { side_letter: side_letter.clone() }
            }
            Self::Disc { disc, .. } => TrackSide::Disc { disc: *disc },
            Self::Flat { .. } => TrackSide::Flat,
        }
    }
}

/// The mediums and places of tracks read from files: one medium per distinct
/// disc the files state, in disc order (files stating none first), each of
/// unstated carrier and one side; each track on its disc's medium, numbered as
/// its file states, else by its place there.
pub fn by_disc(
    tracks: &[(Option<u32>, Option<NonZeroU32>)],
) -> (Vec<ReleaseMedium>, Vec<TrackPlace>) {
    let discs: std::collections::BTreeSet<Option<u32>> =
        tracks.iter().map(|(disc, _)| *disc).collect();
    let discs: Vec<Option<u32>> = discs.into_iter().collect();
    let mediums = discs.iter().map(|_| ReleaseMedium::one_sided(None)).collect();
    let mut placer = Placer::default();
    let places = tracks
        .iter()
        .map(|(disc, number)| {
            let index = discs
                .iter()
                .position(|known| known == disc)
                .expect("every track's disc is one of the discs");
            placer.place(medium_position(index), NonZeroU8::MIN, *number)
        })
        .collect();
    (mediums, places)
}

/// The medium position `index` (counted from zero) names.
pub fn medium_position(index: usize) -> NonZeroU16 {
    u16::try_from(index + 1)
        .ok()
        .and_then(NonZeroU16::new)
        .expect("a release holds fewer than 65536 mediums")
}

/// The side number `index` (counted from zero) names.
pub fn side_number(index: usize) -> NonZeroU8 {
    u8::try_from(index + 1)
        .ok()
        .and_then(NonZeroU8::new)
        .expect("a medium has fewer than 256 sides")
}

/// The letters of the `n`th side of a release's sided mediums, counted from
/// one: A … Z, then AA, AB, ….
pub fn side_letters(n: u32) -> String {
    assert!(n >= 1, "sides are counted from one");
    let mut n = n;
    let mut letters = Vec::new();
    while n > 0 {
        let index = (n - 1) % 26;
        letters.push(char::from(b'A' + u8::try_from(index).expect("below 26")));
        n = (n - 1) / 26;
    }
    letters.iter().rev().collect()
}

/// How a track at `place` reads on a release holding `mediums`. A track on a
/// sided medium is on a lettered side, the letters counting the sides of the
/// release's sided mediums in order; a track on any other medium is on a
/// numbered disc when the release holds more than one medium (its position
/// among them), and plain otherwise.
pub fn track_position(place: &TrackPlace, mediums: &[ReleaseMedium]) -> TrackPosition {
    let number = place.number.get();
    let medium =
        mediums.get(place.medium_index()).expect("a track's medium is one of its release's");
    if medium.is_sided() {
        let sides_before: u32 = mediums[..place.medium_index()]
            .iter()
            .filter(|medium| medium.is_sided())
            .map(|medium| u32::from(medium.sides.get()))
            .sum();
        return TrackPosition::Sided {
            side_letter: side_letters(sides_before + u32::from(place.side.get())),
            number,
        };
    }
    if mediums.len() > 1 {
        return TrackPosition::Disc { disc: u32::from(place.medium.get()), number };
    }
    TrackPosition::Flat { number }
}

/// The side or disc `place` is on, on a release holding `mediums`.
pub fn track_side(place: &TrackPlace, mediums: &[ReleaseMedium]) -> TrackSide {
    track_position(place, mediums).side()
}

#[cfg(test)]
#[path = "mediums_tests.rs"]
mod tests;
