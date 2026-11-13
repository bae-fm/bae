//! The instrument or voice a performer credit names.
//!
//! MusicBrainz lists its instruments by id, and a relation names each one by
//! that id (`attribute-ids`); its voices are the values of its `Vocal`
//! attribute, each with an id the same way. bae holds both lists as
//! MusicBrainz states them, read at compile time, so the apps show them in the
//! viewer's language. An instrument or voice neither list holds is shown as the
//! catalog wrote it.

use super::table;

const INSTRUMENT_LIST: &str = include_str!("musicbrainz-instruments.tsv");
const INSTRUMENT_COUNT: usize = table::count(INSTRUMENT_LIST);
static INSTRUMENTS: table::Entries<INSTRUMENT_COUNT> =
    table::parse::<INSTRUMENT_COUNT>(INSTRUMENT_LIST);

const VOICE_LIST: &str = include_str!("musicbrainz-voices.tsv");
const VOICE_COUNT: usize = table::count(VOICE_LIST);
static VOICES: table::Entries<VOICE_COUNT> = table::parse::<VOICE_COUNT>(VOICE_LIST);

/// The id of MusicBrainz's `Vocal` attribute itself: the voice of a credit
/// that names no kind of voice.
const GENERAL_VOICE_ID: &str = "d92884b7-ee0c-46d5-96f3-918196ba8c5b";

/// The entry of `list` with `id`, as the list holds it.
fn by_id(list: &'static [(&'static str, &'static str)], id: &str) -> Option<&'static str> {
    list.iter().find(|(listed, _)| *listed == id).map(|(listed, _)| *listed)
}

/// The id of the first entry of `list` named `name`.
fn by_name(list: &'static [(&'static str, &'static str)], name: &str) -> Option<&'static str> {
    list.iter().find(|(_, listed)| *listed == name).map(|(id, _)| *id)
}

fn name_of(list: &'static [(&'static str, &'static str)], id: &str) -> &'static str {
    list.iter()
        .find(|(listed, _)| *listed == id)
        .map(|(_, name)| *name)
        .expect("an entry of the list")
}

/// An instrument on MusicBrainz's list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Instrument {
    id: &'static str,
}

impl Instrument {
    /// The instrument MusicBrainz's list gives `id`, `None` for an id it does
    /// not hold.
    pub fn from_id(id: &str) -> Option<Self> {
        by_id(&INSTRUMENTS, id).map(|id| Self { id })
    }

    /// The first instrument MusicBrainz lists under `name`. Two instruments
    /// share the name `tar`; every other name is one instrument's.
    pub fn from_name(name: &str) -> Option<Self> {
        by_name(&INSTRUMENTS, name).map(|id| Self { id })
    }

    /// Every instrument, in the list's order.
    pub fn all() -> impl Iterator<Item = Self> {
        INSTRUMENTS.iter().map(|(id, _)| Self { id })
    }

    /// The id MusicBrainz and bae's stored credits name the instrument by.
    pub fn id(self) -> &'static str {
        self.id
    }

    /// The name MusicBrainz lists the instrument under.
    pub fn name(self) -> &'static str {
        name_of(&INSTRUMENTS, self.id)
    }
}

/// A voice on MusicBrainz's list: its `Vocal` attribute and the kinds of
/// vocals it distinguishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Voice {
    id: &'static str,
}

impl Voice {
    /// The voice of a performer of vocals no kind of which is named.
    pub fn general() -> Self {
        Self { id: by_id(&VOICES, GENERAL_VOICE_ID).expect("the Vocal attribute is listed") }
    }

    /// The voice MusicBrainz's list gives `id`, `None` for an id it does not
    /// hold.
    pub fn from_id(id: &str) -> Option<Self> {
        by_id(&VOICES, id).map(|id| Self { id })
    }

    /// The voice MusicBrainz lists under `name`.
    pub fn from_name(name: &str) -> Option<Self> {
        by_name(&VOICES, name).map(|id| Self { id })
    }

    /// Every voice, in the list's order.
    pub fn all() -> impl Iterator<Item = Self> {
        VOICES.iter().map(|(id, _)| Self { id })
    }

    /// The id MusicBrainz and bae's stored credits name the voice by.
    pub fn id(self) -> &'static str {
        self.id
    }

    /// The name MusicBrainz lists the voice under.
    pub fn name(self) -> &'static str {
        name_of(&VOICES, self.id)
    }
}

/// What a performer performs.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Part {
    Instrument(Instrument),
    Voice(Voice),
    /// An instrument or voice bae's lists do not hold, as the catalog wrote it.
    Words(String),
}

#[cfg(test)]
#[path = "part_tests.rs"]
mod tests;
