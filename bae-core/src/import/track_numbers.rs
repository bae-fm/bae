//! Where a folder's own files place each track: the disc and track numbers
//! the file's tags state, and, where its tags state none, the numbers its name
//! starts with.
//!
//! These numbers decide the order a folder's audio plays in (the scan lays its
//! files down by them) and the numbers a draft read from the files gives each
//! track. Tags come first because they are what a ripper or store wrote about
//! the track; a name is a person's spelling of it, and a name with no number
//! (`Title.m4a`) says nothing about where the track goes.

use std::cmp::Ordering;

/// A track's disc and track number, as one source states them. A stated number
/// is at least one: a tag or name reading zero states nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TrackNumbers {
    pub disc: Option<u32>,
    pub track: Option<u32>,
}

impl TrackNumbers {
    /// What `tag` states: its track number (Vorbis `TRACKNUMBER`, ID3 `TRCK`,
    /// MP4 `trkn`) and disc number (`DISCNUMBER`, `TPOS`, `disk`), without the
    /// totals some formats write beside them.
    pub(crate) fn of_tag(tag: &lofty::tag::Tag) -> Self {
        use lofty::tag::Accessor;
        Self {
            disc: tag.disk().filter(|number| *number > 0),
            track: tag.track().filter(|number| *number > 0),
        }
    }

    /// The numbers a file's name starts with: `07 Title.flac` is track 7, and
    /// `2-07 Title.flac` — how a multi-disc rip is commonly named — is disc 2,
    /// track 7. A run of more than three digits is a year or a catalog number,
    /// not a track, and states nothing.
    pub(crate) fn in_file_name(relative_path: &str) -> Self {
        let name = relative_path
            .rsplit('/')
            .next()
            .expect("rsplit yields at least one piece")
            .trim_start();
        let (first, rest) = leading_number(name);
        let Some(first) = first else {
            return Self::unstated();
        };
        match rest
            .strip_prefix('-')
            .map(leading_number)
            .and_then(|(track, _)| track)
        {
            Some(track) => Self {
                disc: Some(first),
                track: Some(track),
            },
            None => Self {
                disc: None,
                track: Some(first),
            },
        }
    }

    /// A file's numbers: its tags' `self`, each number they leave unstated
    /// taken from its name.
    pub(crate) fn or_file_name(self, relative_path: &str) -> Self {
        let named = Self::in_file_name(relative_path);
        Self {
            disc: self.disc.or(named.disc),
            track: self.track.or(named.track),
        }
    }

    /// Numbers a file whose tags could not be read, or that carries no tag,
    /// states.
    pub(crate) fn unstated() -> Self {
        Self {
            disc: None,
            track: None,
        }
    }

    /// The order two files play in by these numbers: by disc, then by track,
    /// a number one leaves unstated placing it after every file that states
    /// one. Files the numbers do not tell apart compare equal, for the caller
    /// to order by name.
    pub(crate) fn play_order(&self, other: &Self) -> Ordering {
        stated_first(self.disc, other.disc).then_with(|| stated_first(self.track, other.track))
    }
}

/// The 1–3 digit number `text` starts with, and the text after it. A number of
/// zero, or a run of four or more digits, is none.
fn leading_number(text: &str) -> (Option<u32>, &str) {
    let digits = text.bytes().take_while(u8::is_ascii_digit).count();
    let number = (1..=3)
        .contains(&digits)
        .then(|| {
            text[..digits]
                .parse::<u32>()
                .expect("1-3 ASCII digits parse")
        })
        .filter(|number| *number > 0);
    (number, &text[digits..])
}

fn stated_first(left: Option<u32>, right: Option<u32>) -> Ordering {
    match (left, right) {
        (Some(left), Some(right)) => left.cmp(&right),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

#[cfg(test)]
#[path = "track_numbers_tests.rs"]
mod tests;
