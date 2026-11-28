//! The order of the album grid under a sort: the sort it sorts by first,
//! which picks a compiled statement, and the sorts after it, which the
//! statement takes as parameters; and the keys that say where a row is in it.

use super::*;
use bae_store::list_windows::SortValue;

/// One sort of the grid, a field one way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SortCode {
    Title,
    TitleBack,
    Artist,
    ArtistBack,
    Year,
    YearBack,
    Added,
    AddedBack,
}

impl SortCode {
    fn of(criterion: &AlbumSortCriterion) -> Self {
        use AlbumSortField::{Artist, DateAdded, Title, Year};
        use SortDirection::{Ascending, Descending};
        match (criterion.field, criterion.direction) {
            (Title, Ascending) => Self::Title,
            (Title, Descending) => Self::TitleBack,
            (Artist, Ascending) => Self::Artist,
            (Artist, Descending) => Self::ArtistBack,
            (Year, Ascending) => Self::Year,
            (Year, Descending) => Self::YearBack,
            (DateAdded, Ascending) => Self::Added,
            (DateAdded, Descending) => Self::AddedBack,
        }
    }

    /// The code the order's fragments take for the sort.
    fn code(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::TitleBack => "title_back",
            Self::Artist => "artist",
            Self::ArtistBack => "artist_back",
            Self::Year => "year",
            Self::YearBack => "year_back",
            Self::Added => "added",
            Self::AddedBack => "added_back",
        }
    }
}

/// What the grid sorts by first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AlbumLead {
    /// The flat grid, one row for each album.
    Flat(SortCode),
    /// The grid grouped by artist, one row for each album under each artist
    /// its credits show: the artists, ascending or descending.
    Grouped(SortDirection),
}

/// How many values a key of the grid holds: the first term, the two terms of
/// each place after it, and the album's id.
const KEY_VALUES: usize = 8;

/// How many sorts can come after the first.
const LATER: usize = 3;

/// The grid's order under a sort, flat or grouped by artist. With no sort the
/// newest albums come first. Rows the sorts tie go by album id, so each row
/// has one place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AlbumOrder {
    pub(crate) lead: AlbumLead,
    /// The sorts after the first, in turn.
    later: [Option<SortCode>; LATER],
}

impl AlbumOrder {
    /// The order under `sort`, which sorts by each field once: a field's
    /// second criterion could tell apart no rows its first ties.
    pub(crate) fn new(sort: &[AlbumSortCriterion], group_by_artist: bool) -> Self {
        let mut criteria: Vec<&AlbumSortCriterion> = Vec::new();
        for criterion in sort {
            if !criteria.iter().any(|taken| taken.field == criterion.field) {
                criteria.push(criterion);
            }
        }
        let mut codes = criteria.iter().map(|criterion| SortCode::of(criterion));
        let (lead, later) = if group_by_artist {
            let mut later: Vec<SortCode> = criteria
                .iter()
                .filter(|criterion| criterion.field != AlbumSortField::Artist)
                .map(|criterion| SortCode::of(criterion))
                .collect();
            if sort.is_empty() {
                later.push(SortCode::AddedBack);
            }
            (AlbumLead::Grouped(grouped_artist_direction(sort)), later)
        } else {
            let first = codes.next().unwrap_or(SortCode::AddedBack);
            (AlbumLead::Flat(first), codes.collect())
        };
        let mut places = [None; LATER];
        for (place, code) in places.iter_mut().zip(later) {
            *place = Some(code);
        }
        Self { lead, later: places }
    }

    /// The flat grid's order under `sort`, with the sort it sorts by first.
    pub(crate) fn flat(sort: &[AlbumSortCriterion]) -> (SortCode, Self) {
        let order = Self::new(sort, false);
        (order.first(), order)
    }

    /// The sort the grid sorts by first.
    fn first(&self) -> SortCode {
        match self.lead {
            AlbumLead::Flat(first) => first,
            AlbumLead::Grouped(SortDirection::Ascending) => SortCode::Artist,
            AlbumLead::Grouped(SortDirection::Descending) => SortCode::ArtistBack,
        }
    }

    /// The code of the first sort, then of each place after it, empty for a
    /// place no sort takes: the parameters the order's fragments take.
    pub(super) fn codes(&self) -> [String; LATER + 1] {
        let code = |sort: Option<SortCode>| sort.map_or("", SortCode::code).to_string();
        let [second, third, fourth] = self.later;
        [code(Some(self.first())), code(second), code(third), code(fourth)]
    }

    /// The values `key` names. Only the grid's reads make its keys.
    pub(super) fn values_of(key: &OrderKey) -> Result<[SortValue; KEY_VALUES], DbError> {
        <[SortValue; KEY_VALUES]>::try_from(key.values().to_vec())
            .map_err(|values| DbError::unreadable("an album grid key", values))
    }
}

/// The direction the grid grouped by artist orders its artists in: its
/// Artist criterion's, else ascending.
pub(super) fn grouped_artist_direction(sort: &[AlbumSortCriterion]) -> SortDirection {
    sort.iter()
        .find(|c| c.field == AlbumSortField::Artist)
        .map_or(SortDirection::Ascending, |c| c.direction)
}
