//! What the units of a folder that state album facts — each file's tags, or
//! each CUE sheet — say of the album together (`TaggedAlbum::of`): the value
//! they all give, else the first one's in play order, its title without a
//! trailing disc marker.

use crate::cue_flac::CueSheet;
use crate::file_tag_reader::{non_empty, years_in_date};
use crate::file_tag_snapshot::FileTags;
use bae_identify::row_facts::StatedYears;

/// What one unit that states album facts — a file's tags, or a CUE sheet —
/// says of the album.
pub(super) struct UnitAlbum {
    title: Option<String>,
    /// The album artists it names (ALBUMARTIST, a sheet's PERFORMER).
    album_artists: Vec<String>,
    /// The track artists it names (a file's ARTIST), which name the album's
    /// artists where no unit names an album artist.
    track_artists: Vec<String>,
    /// Whether it says the album is a compilation: a compilation flag, or the
    /// various-artists credit as its album artist.
    compilation: bool,
    years: StatedYears,
}

impl UnitAlbum {
    pub(super) fn of_file(tags: &FileTags) -> Self {
        let years: Vec<i32> = tags.years.iter().copied().map(i32::from).collect();
        Self {
            title: tags.album_title.clone(),
            album_artists: tags.album_artists.clone(),
            track_artists: tags.track_artists.clone(),
            compilation: tags.compilation || names_various_artists(&tags.album_artists),
            years: StatedYears::of(&years, None),
        }
    }

    pub(super) fn of_sheet(sheet: &CueSheet) -> Self {
        let album_artists: Vec<String> = non_empty(sheet.performer.clone()).into_iter().collect();
        let years: Vec<i32> = sheet
            .date
            .as_deref()
            .map(|date| years_in_date(date).map(i32::from).collect())
            .unwrap_or_default();
        Self {
            title: non_empty(sheet.title.clone()),
            compilation: names_various_artists(&album_artists),
            album_artists,
            track_artists: Vec::new(),
            years: StatedYears::of(&years, None),
        }
    }
}

/// Whether `artists` is the one credit a compilation is given.
fn names_various_artists(artists: &[String]) -> bool {
    matches!(artists, [only] if bae_types::text_match::is_various_artists(only))
}

/// What the folder's units, together, say of the album. Units that state a
/// value differently still give the album one, the first unit's: the discs
/// of one set tag `Album Title (Disc 1)` and `Album Title (Disc 2)`, and a
/// reissue's discs can carry different years.
pub(super) struct TaggedAlbum {
    pub(super) title: String,
    /// The album's artists, by name, in order; empty where no unit names
    /// one.
    pub(super) artists: Vec<String>,
    /// Every unit says the album is a compilation: it is credited to the
    /// various-artists artist, and each track keeps its own artists.
    pub(super) compilation: bool,
    pub(super) years: StatedYears,
}

impl TaggedAlbum {
    /// The album `units` describe, given in the order they play (disc, then
    /// track). Each value is the one every unit stating it gives, else the
    /// first such unit's; a title taken from the first unit drops a trailing
    /// disc marker ([`without_disc_marker`]). The title falls back to
    /// `folder_name` where no unit states one. The artists are the album
    /// artists where any unit names one, else the track artists.
    pub(super) fn of(units: impl Iterator<Item = UnitAlbum>, folder_name: Option<&str>) -> Self {
        let units: Vec<UnitAlbum> = units.collect();
        let title = match given(units.iter().map(|unit| unit.title.as_deref()), |a, b| a == b) {
            Given::Agreed(title) => title.to_string(),
            Given::Disagreed { first } => without_disc_marker(first).to_string(),
            Given::Unstated => folder_name.map(str::to_string).unwrap_or_default(),
        };
        let stated = |artists: &[String]| (!artists.is_empty()).then(|| artists.to_vec());
        let artists =
            given(units.iter().map(|unit| stated(&unit.album_artists)), |a, b| same_names(a, b))
                .value()
                .or_else(|| {
                    given(units.iter().map(|unit| stated(&unit.track_artists)), |a, b| {
                        same_names(a, b)
                    })
                    .value()
                })
                .unwrap_or_default();
        let year = |year: fn(&StatedYears) -> Option<i32>| {
            given(units.iter().map(|unit| year(&unit.years)), |a, b| a == b).value()
        };
        Self {
            title,
            artists,
            compilation: units.iter().all(|unit| unit.compilation),
            years: StatedYears {
                album: year(|years| years.album),
                pressing: year(|years| years.pressing),
            },
        }
    }
}

/// What the units stating a value give of it.
enum Given<T> {
    /// Every unit stating it states this.
    Agreed(T),
    /// Units state it differently; `first` is the first one's.
    Disagreed { first: T },
    /// No unit states it.
    Unstated,
}

impl<T> Given<T> {
    /// The value the album takes: the one agreed on, else the first unit's.
    fn value(self) -> Option<T> {
        match self {
            Self::Agreed(value) | Self::Disagreed { first: value } => Some(value),
            Self::Unstated => None,
        }
    }
}

/// What the units stating a value give of it, by `same`; a unit stating
/// nothing (`None`) never disagrees.
fn given<T>(values: impl Iterator<Item = Option<T>>, same: impl Fn(&T, &T) -> bool) -> Given<T> {
    let mut first: Option<T> = None;
    let mut agreed = true;
    for value in values.flatten() {
        match &first {
            None => first = Some(value),
            Some(known) => agreed &= same(known, &value),
        }
    }
    match first {
        Some(value) if agreed => Given::Agreed(value),
        Some(first) => Given::Disagreed { first },
        None => Given::Unstated,
    }
}

/// `title` without the disc marker it ends with, and the separator before
/// it: `Album Title (cd 1)`, `Album Title [disc  09]`, `Album Title CD1`,
/// `Album Title - Disc 1` and `Album Title_D2` are all `Album Title`. A
/// marker is `disc`, `disk`, `cd` or `d` standing as its own word, then a
/// number, the two in brackets or not. `Album Title Volume 1` names no disc
/// and keeps its number, and a title that is nothing but a marker is kept
/// whole. Trailing spaces go either way.
fn without_disc_marker(title: &str) -> &str {
    const MARKERS: &[&str] = &["disc", "disk", "cd", "d"];
    const SEPARATORS: &[char] = &['-', '–', '—', ':', ',', '_', '/', '|'];
    let title = title.trim_end();
    let (inside, open) = match title.chars().last() {
        Some(')') => (&title[..title.len() - 1], Some('(')),
        Some(']') => (&title[..title.len() - 1], Some('[')),
        Some('}') => (&title[..title.len() - 1], Some('{')),
        _ => (title, None),
    };
    let inside = inside.trim_end();
    let number_start = inside.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    if number_start == inside.len() {
        return title;
    }
    let before_number = inside[..number_start].trim_end();
    let lowered = before_number.to_ascii_lowercase();
    let Some(marker) = MARKERS.iter().find(|marker| lowered.ends_with(*marker)) else {
        return title;
    };
    let before_marker = &before_number[..before_number.len() - marker.len()];
    if before_marker.chars().next_back().is_some_and(char::is_alphanumeric) {
        return title;
    }
    let before_marker = match open {
        Some(open) => match before_marker.strip_suffix(open) {
            Some(rest) => rest,
            None => return title,
        },
        None if before_marker.ends_with(['(', '[', '{']) => return title,
        None => before_marker,
    };
    let kept =
        before_marker.trim_end_matches(|c: char| c.is_whitespace() || SEPARATORS.contains(&c));
    if kept.is_empty() {
        title
    } else {
        kept
    }
}

/// Whether two lists of artist names name the same artists in the same
/// order.
pub(super) fn same_names(a: &[String], b: &[String]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(a, b)| bae_types::text_match::same_artist_name(a, b))
}

#[cfg(test)]
#[path = "album_tests.rs"]
mod tests;
