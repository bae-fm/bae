//! The columns that store a track sheet's disc and the person's binding of
//! one of its `FILE` references. The scan's files, the candidate's decisions
//! and the release's decisions all store them this way, so each table's
//! writer and reader go through the one pair of conversions here.
//!
//! A [`SheetDisc`] is a `disc` column, `'disc'` or `'ignored'`, beside a disc
//! number that only a disc has. A [`UserSheetBinding`] is the file that the
//! reference describes, absent when the person cleared the binding.

use super::*;
use crate::folder_scanner::{SheetDisc, UserSheetBinding};

impl SheetDisc {
    /// The `disc` and disc number columns that store this disc.
    pub(super) fn columns(self) -> (&'static str, Option<u32>) {
        match self {
            Self::Disc { number } => ("disc", Some(number)),
            Self::Ignored => ("ignored", None),
        }
    }

    /// The disc that `disc` and `number`, read as `what`, store.
    pub(super) fn from_columns(
        what: &str,
        disc: &str,
        number: Option<i64>,
    ) -> Result<Self, DbError> {
        match (disc, number) {
            ("disc", Some(number)) => Ok(Self::Disc { number: to_u32(number, what)? }),
            ("ignored", None) => Ok(Self::Ignored),
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
