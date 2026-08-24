//! The text a folder-scan column stores for one of the scan's enums, and the
//! conversion back. Both directions live in one file so a stored spelling and
//! the reader that accepts it cannot drift apart.

use super::*;
use crate::import::folder_scanner::{
    FileRole, FolderReleaseDecision, InvalidReason, ReleaseFileScope, SheetBinding, SheetDisc,
};

/// A column holding a value no writer here produces.
pub(crate) fn unreadable(column: &str, stored: &str) -> DbError {
    DbError::Message(format!("folder scan column {column} holds {stored:?}"))
}

pub(super) fn to_i64(value: u64, what: &str) -> Result<i64, DbError> {
    i64::try_from(value)
        .map_err(|_| DbError::Message(format!("{what} exceeds SQLite's integer range")))
}

pub(crate) fn to_u64(value: i64, what: &str) -> Result<u64, DbError> {
    u64::try_from(value).map_err(|_| DbError::Message(format!("{what} is negative")))
}

pub(crate) fn to_u32(value: i64, what: &str) -> Result<u32, DbError> {
    u32::try_from(value)
        .map_err(|_| DbError::Message(format!("{what} is outside the range it counts over")))
}

pub(super) fn scope_text(scope: ReleaseFileScope) -> &'static str {
    match scope {
        ReleaseFileScope::Direct => "direct",
        ReleaseFileScope::Recursive => "recursive",
    }
}

pub(super) fn scope_of(stored: &str) -> Result<ReleaseFileScope, DbError> {
    match stored {
        "direct" => Ok(ReleaseFileScope::Direct),
        "recursive" => Ok(ReleaseFileScope::Recursive),
        other => Err(unreadable("scope", other)),
    }
}

pub(super) fn decision_text(decision: FolderReleaseDecision) -> &'static str {
    match decision {
        FolderReleaseDecision::CombineAsOneRelease => "combine_as_one_release",
        FolderReleaseDecision::KeepAsSeparateReleases => "keep_as_separate_releases",
    }
}

pub(super) fn decision_of(stored: &str) -> Result<FolderReleaseDecision, DbError> {
    match stored {
        "combine_as_one_release" => Ok(FolderReleaseDecision::CombineAsOneRelease),
        "keep_as_separate_releases" => Ok(FolderReleaseDecision::KeepAsSeparateReleases),
        other => Err(unreadable("decision", other)),
    }
}

/// The reason and the path it names, as the two columns store them.
pub(super) fn invalid_reason_columns(reason: &InvalidReason) -> (&'static str, Option<&str>) {
    match reason {
        InvalidReason::CorruptAudioFile { path } => ("corrupt_audio", Some(path.as_str())),
        InvalidReason::CorruptImage { path } => ("corrupt_image", Some(path.as_str())),
        InvalidReason::NoValidAudio => ("no_valid_audio", None),
    }
}

pub(crate) fn invalid_reason_of(
    stored: &str,
    path: Option<String>,
) -> Result<InvalidReason, DbError> {
    // The table pairs the two columns, so a reason that names a path always
    // has one; a build reading a row written before that pairing existed is
    // not a case — the schema is rebuilt, never migrated.
    let named = |path: Option<String>| {
        path.ok_or_else(|| DbError::Message(format!("folder scan reason {stored} names no path")))
    };
    match stored {
        "corrupt_audio" => Ok(InvalidReason::CorruptAudioFile { path: named(path)? }),
        "corrupt_image" => Ok(InvalidReason::CorruptImage { path: named(path)? }),
        "no_valid_audio" => Ok(InvalidReason::NoValidAudio),
        other => Err(unreadable("invalid_reason", other)),
    }
}

/// The role, and the two pairs of columns a track sheet adds to it.
pub(super) struct RoleColumns<'a> {
    pub(super) role: &'static str,
    pub(super) sheet_binding: Option<&'static str>,
    pub(super) sheet_binding_file_id: Option<&'a str>,
    pub(super) sheet_binding_codec: Option<&'a str>,
    pub(super) sheet_disc: Option<&'static str>,
    pub(super) sheet_disc_number: Option<u32>,
}

pub(super) fn role_columns(role: &FileRole) -> RoleColumns<'_> {
    let plain = |role| RoleColumns {
        role,
        sheet_binding: None,
        sheet_binding_file_id: None,
        sheet_binding_codec: None,
        sheet_disc: None,
        sheet_disc_number: None,
    };
    match role {
        FileRole::Audio => plain("audio"),
        FileRole::Artwork => plain("artwork"),
        FileRole::Document => plain("document"),
        FileRole::Other => plain("other"),
        FileRole::TrackSheet { binding, disc, .. } => {
            let (sheet_binding, sheet_binding_file_id, sheet_binding_codec) = match binding {
                SheetBinding::Describes { file_id } => ("describes", Some(file_id.as_str()), None),
                SheetBinding::Unresolved => ("unresolved", None, None),
                SheetBinding::RefusedCodec { file_id, codec } => (
                    "refused_codec",
                    Some(file_id.as_str()),
                    Some(codec.as_str()),
                ),
            };
            let (sheet_disc, sheet_disc_number) = match disc {
                SheetDisc::Disc { number } => ("disc", Some(*number)),
                SheetDisc::Ignored => ("ignored", None),
            };
            RoleColumns {
                role: "track_sheet",
                sheet_binding: Some(sheet_binding),
                sheet_binding_file_id,
                sheet_binding_codec,
                sheet_disc: Some(sheet_disc),
                sheet_disc_number,
            }
        }
    }
}

pub(super) fn sheet_binding_of(
    stored: &str,
    file_id: Option<String>,
    codec: Option<String>,
) -> Result<SheetBinding, DbError> {
    let named = |value: Option<String>, column: &str| {
        value.ok_or_else(|| {
            DbError::Message(format!("folder scan binding {stored} has no {column}"))
        })
    };
    match stored {
        "describes" => Ok(SheetBinding::Describes {
            file_id: named(file_id, "file id")?,
        }),
        "unresolved" => Ok(SheetBinding::Unresolved),
        "refused_codec" => Ok(SheetBinding::RefusedCodec {
            file_id: named(file_id, "file id")?,
            codec: named(codec, "codec")?,
        }),
        other => Err(unreadable("sheet_binding", other)),
    }
}

pub(super) fn sheet_disc_of(stored: &str, number: Option<i64>) -> Result<SheetDisc, DbError> {
    match stored {
        "disc" => {
            let number = number.ok_or_else(|| {
                DbError::Message("folder scan sheet disc names no number".to_string())
            })?;
            Ok(SheetDisc::Disc {
                number: to_u32(number, "a sheet's disc number")?,
            })
        }
        "ignored" => Ok(SheetDisc::Ignored),
        other => Err(unreadable("sheet_disc", other)),
    }
}
