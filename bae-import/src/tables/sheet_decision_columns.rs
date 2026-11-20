//! The columns that store a track sheet's place and the person's binding of
//! one of its `FILE` references. The scan's files, the candidate's decisions
//! and the release's decisions all store them this way, so each table's
//! writer and reader go through the one pair of conversions here.
//!
//! A [`SheetPlace`] is a `place` column, `'side'` or `'ignored'`, beside a
//! medium and a side that only a side has. A [`UserSheetBinding`] is the file that the
//! reference describes, absent when the person cleared the binding.

use super::*;
use crate::folder_scanner::{SheetPlace, UserSheetBinding};
use bae_types::mediums::MediumSide;

impl SheetPlace {
    /// The `place`, medium and side columns that store this place.
    pub(super) fn columns(self) -> (&'static str, Option<u32>, Option<u32>) {
        match self {
            Self::On(side) => ("side", Some(side.medium), Some(side.side)),
            Self::Ignored => ("ignored", None, None),
        }
    }

    /// The place that `place`, `medium` and `side`, read as `what`, store.
    pub(super) fn from_columns(
        what: &str,
        place: &str,
        medium: Option<i64>,
        side: Option<i64>,
    ) -> Result<Self, DbError> {
        match (place, medium, side) {
            ("side", Some(medium), Some(side)) => Ok(Self::On(MediumSide {
                medium: to_u32(medium, what)?,
                side: to_u32(side, what)?,
            })),
            ("ignored", None, None) => Ok(Self::Ignored),
            stored => Err(DbError::unreadable(what, stored)),
        }
    }
}

impl UserSheetBinding {
    /// The file the column stores for this binding: the audio it describes,
    /// or none when the person cleared it.
    pub(super) fn stored_file(&self) -> Option<&str> {
        match self {
            Self::Describes { file_id } => Some(file_id),
            Self::Cleared => None,
        }
    }

    /// The binding whose column stores `file`.
    pub(super) fn from_stored_file(file: Option<String>) -> Self {
        match file {
            Some(file_id) => Self::Describes { file_id },
            None => Self::Cleared,
        }
    }
}
