//! The text a folder-scan column stores for one of the scan's enums, and the
//! conversion back. Both directions live in one file so a stored spelling and
//! the reader that accepts it cannot drift apart.

use super::*;
use crate::import::folder_scanner::{
    AudioDamage, FileRole, ProposedRole, ReleaseFileScope, SheetAudioFile, SheetBinding, SheetDisc,
};

/// A column holding a value no writer here produces.
pub(crate) fn unreadable(column: &str, stored: &str) -> DbError {
    DbError::Message(format!("folder scan column {column} holds {stored:?}"))
}

pub(crate) fn to_u64(value: i64, what: &str) -> Result<u64, DbError> {
    u64::try_from(value).map_err(|error| DbError::Message(format!("{what} is negative: {error}")))
}

pub(crate) fn to_u32(value: i64, what: &str) -> Result<u32, DbError> {
    u32::try_from(value).map_err(|error| {
        DbError::Message(format!(
            "{what} is outside the range it counts over: {error}"
        ))
    })
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

/// A skipped file's role, and the damage a damaged track names, as the two
/// columns store them. The roles a release's files share are spelled as
/// [`role_columns`] spells them.
pub(super) fn proposed_role_columns(role: ProposedRole) -> (&'static str, Option<&'static str>) {
    match role {
        ProposedRole::Audio => ("audio", None),
        ProposedRole::DamagedAudio { damage } => (
            "damaged_audio",
            Some(match damage {
                AudioDamage::Empty => "empty",
                AudioDamage::Unreadable => "unreadable",
            }),
        ),
        ProposedRole::Cue => ("track_sheet", None),
        ProposedRole::Image => ("artwork", None),
        ProposedRole::DamagedImage => ("damaged_image", None),
        ProposedRole::Document => ("document", None),
        ProposedRole::Other => ("other", None),
    }
}

pub(super) fn proposed_role_of(
    stored: &str,
    damage: Option<&str>,
) -> Result<ProposedRole, DbError> {
    // The table pairs the two columns, so only a damaged track names damage.
    match (stored, damage) {
        ("audio", None) => Ok(ProposedRole::Audio),
        ("damaged_audio", Some("empty")) => Ok(ProposedRole::DamagedAudio {
            damage: AudioDamage::Empty,
        }),
        ("damaged_audio", Some("unreadable")) => Ok(ProposedRole::DamagedAudio {
            damage: AudioDamage::Unreadable,
        }),
        ("track_sheet", None) => Ok(ProposedRole::Cue),
        ("artwork", None) => Ok(ProposedRole::Image),
        ("damaged_image", None) => Ok(ProposedRole::DamagedImage),
        ("document", None) => Ok(ProposedRole::Document),
        ("other", None) => Ok(ProposedRole::Other),
        (stored, damage) => Err(DbError::Message(format!(
            "skipped file role {stored:?} stores damage {damage:?}"
        ))),
    }
}

/// The role, and the columns a track sheet adds to it. The audio a bound
/// sheet describes is not a column: it is the sheet's `scan_sheet_audio_file`
/// rows, written after every file of the candidate is in place.
pub(super) struct RoleColumns<'a> {
    pub(super) role: &'static str,
    pub(super) sheet_binding: Option<&'static str>,
    pub(super) sheet_binding_codec: Option<&'a str>,
    pub(super) sheet_disc: Option<&'static str>,
    pub(super) sheet_disc_number: Option<u32>,
}

pub(super) fn role_columns(role: &FileRole) -> RoleColumns<'_> {
    let plain = |role| RoleColumns {
        role,
        sheet_binding: None,
        sheet_binding_codec: None,
        sheet_disc: None,
        sheet_disc_number: None,
    };
    match role {
        FileRole::Audio => plain("audio"),
        FileRole::Artwork => plain("artwork"),
        FileRole::DamagedImage => plain("damaged_image"),
        FileRole::Document => plain("document"),
        FileRole::Other => plain("other"),
        FileRole::TrackSheet { binding, disc, .. } => {
            let (sheet_binding, sheet_binding_codec) = match binding {
                SheetBinding::Resolved { .. } => ("resolved", None),
                SheetBinding::Unresolved { .. } => ("unresolved", None),
                SheetBinding::RefusedTiming { .. } => ("refused_timing", None),
                SheetBinding::RefusedCodec { codec } => ("refused_codec", Some(codec.as_str())),
            };
            let (sheet_disc, sheet_disc_number) = match disc {
                SheetDisc::Disc { number } => ("disc", Some(*number)),
                SheetDisc::Ignored => ("ignored", None),
            };
            RoleColumns {
                role: "track_sheet",
                sheet_binding: Some(sheet_binding),
                sheet_binding_codec,
                sheet_disc: Some(sheet_disc),
                sheet_disc_number,
            }
        }
    }
}

/// A sheet's binding from its column and its stored audio pairing. A bound or
/// timing-refused spelling with no pairing rows is a store nothing here
/// wrote.
pub(super) fn sheet_binding_of(
    stored: &str,
    audio_files: Vec<SheetAudioFile>,
    codec: Option<String>,
) -> Result<SheetBinding, DbError> {
    let malformed =
        |detail: &str| DbError::Message(format!("folder scan binding {stored} {detail}"));
    match stored {
        "resolved" => {
            if audio_files.is_empty() {
                return Err(malformed("names no audio"));
            }
            Ok(SheetBinding::Resolved { files: audio_files })
        }
        "unresolved" => Ok(SheetBinding::Unresolved { files: audio_files }),
        "refused_timing" => {
            if audio_files.is_empty() {
                return Err(malformed("names no audio"));
            }
            Ok(SheetBinding::RefusedTiming { files: audio_files })
        }
        "refused_codec" => Ok(SheetBinding::RefusedCodec {
            codec: codec.ok_or_else(|| malformed("has no codec"))?,
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
