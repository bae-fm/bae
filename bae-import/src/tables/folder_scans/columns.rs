//! The text a folder-scan column stores for one of the scan's enums, and the
//! conversion back. Both directions live in one file so a stored spelling and
//! the reader that accepts it cannot drift apart.

use super::*;
use crate::cue_sheet::CdRipper;
use crate::folder_scanner::{
    AudioDamage, FileRole, ProposedRole, ReleaseFileScope, SheetAudioFile, SheetBinding,
};
use crate::VolumeKind;
use bae_records::DocumentKind;

/// The volume a scan root's row says its folder is on.
pub(super) fn volume_text(volume: VolumeKind) -> &'static str {
    match volume {
        VolumeKind::Local => "local",
        VolumeKind::Network => "network",
    }
}

pub(crate) fn volume_of(stored: &str) -> Result<VolumeKind, DbError> {
    match stored {
        "local" => Ok(VolumeKind::Local),
        "network" => Ok(VolumeKind::Network),
        other => Err(DbError::unreadable("folder_scan_roots.volume", other)),
    }
}

/// The text `folder_scan_roots.unwatched_cause` stores for `cause`.
pub(super) fn unwatched_cause_text(cause: crate::UnwatchedCause) -> &'static str {
    match cause {
        crate::UnwatchedCause::LimitReached => "limit_reached",
        crate::UnwatchedCause::Refused => "refused",
    }
}

fn unwatched_cause_of(stored: &str) -> Result<crate::UnwatchedCause, DbError> {
    match stored {
        "limit_reached" => Ok(crate::UnwatchedCause::LimitReached),
        "refused" => Ok(crate::UnwatchedCause::Refused),
        other => Err(DbError::unreadable("folder_scan_roots.unwatched_cause", other)),
    }
}

/// The reason a root is not watched, from its row's `detail`,
/// `unwatched_directory` and `unwatched_cause`.
pub(super) fn unwatched_reason_of(
    message: String,
    directory: std::path::PathBuf,
    cause: &str,
) -> Result<crate::UnwatchedReason, DbError> {
    Ok(crate::UnwatchedReason { directory, cause: unwatched_cause_of(cause)?, message })
}

/// The ripper a stored track sheet names.
pub(super) fn ripper_text(ripper: CdRipper) -> &'static str {
    match ripper {
        CdRipper::ExactAudioCopy => "exact_audio_copy",
    }
}

pub(super) fn ripper_of(stored: &str) -> Result<CdRipper, DbError> {
    match stored {
        "exact_audio_copy" => Ok(CdRipper::ExactAudioCopy),
        other => Err(DbError::unreadable("scan_cue_sheet.ripper", other)),
    }
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
        other => Err(DbError::unreadable("scan_candidate.scope", other)),
    }
}

/// A skipped file's role, the damage a damaged track names, the bits per
/// sample of a too deep one and the kind a document is, as the four columns
/// store them. The roles a release's files
/// share are spelled as [`role_columns`] spells them.
pub(super) struct ProposedRoleColumns {
    pub(super) role: &'static str,
    pub(super) audio_damage: Option<&'static str>,
    pub(super) audio_bits: Option<u32>,
    pub(super) document_kind: Option<&'static str>,
}

pub(super) fn proposed_role_columns(role: ProposedRole) -> ProposedRoleColumns {
    let plain = |role| ProposedRoleColumns {
        role,
        audio_damage: None,
        audio_bits: None,
        document_kind: None,
    };
    match role {
        ProposedRole::Audio => plain("audio"),
        ProposedRole::DamagedAudio { damage } => ProposedRoleColumns {
            audio_damage: Some(match damage {
                AudioDamage::Empty => "empty",
                AudioDamage::Unreadable => "unreadable",
            }),
            ..plain("damaged_audio")
        },
        ProposedRole::DeepAudio { bits } => {
            ProposedRoleColumns { audio_bits: Some(bits), ..plain("deep_audio") }
        }
        ProposedRole::Cue => plain("track_sheet"),
        ProposedRole::Image => plain("artwork"),
        ProposedRole::DamagedImage => plain("damaged_image"),
        ProposedRole::Document { kind } => {
            ProposedRoleColumns { document_kind: Some(kind.as_str()), ..plain("document") }
        }
        ProposedRole::Other => plain("other"),
    }
}

pub(super) fn proposed_role_of(
    stored: &str,
    damage: Option<&str>,
    bits: Option<i64>,
    document_kind: Option<&str>,
) -> Result<ProposedRole, DbError> {
    // The table pairs the columns, so only a damaged track names damage, only
    // a too deep one its bits, and only a document a kind.
    match (stored, damage, bits, document_kind) {
        ("audio", None, None, None) => Ok(ProposedRole::Audio),
        ("damaged_audio", Some("empty"), None, None) => {
            Ok(ProposedRole::DamagedAudio { damage: AudioDamage::Empty })
        }
        ("damaged_audio", Some("unreadable"), None, None) => {
            Ok(ProposedRole::DamagedAudio { damage: AudioDamage::Unreadable })
        }
        ("deep_audio", None, Some(bits), None) => Ok(ProposedRole::DeepAudio {
            bits: u32::try_from(bits)
                .map_err(|error| DbError::unreadable("a skipped file's bit depth", error))?,
        }),
        ("track_sheet", None, None, None) => Ok(ProposedRole::Cue),
        ("artwork", None, None, None) => Ok(ProposedRole::Image),
        ("damaged_image", None, None, None) => Ok(ProposedRole::DamagedImage),
        ("document", None, None, Some(kind)) => {
            Ok(ProposedRole::Document { kind: document_kind_of(kind)? })
        }
        ("other", None, None, None) => Ok(ProposedRole::Other),
        stored => Err(DbError::unreadable(
            "a skipped file's role, damage, bit depth and document kind",
            stored,
        )),
    }
}

/// The document kind a scanned file's `document_kind` column names.
pub(super) fn document_kind_of(stored: &str) -> Result<DocumentKind, DbError> {
    DocumentKind::from_db_value(stored)
        .ok_or_else(|| DbError::unreadable("a scanned file's document_kind", stored))
}

/// The role, and the columns a track sheet or a document adds to it. The
/// audio a bound sheet describes is not a column: it is the sheet's
/// `scan_sheet_audio_file` rows, written after every file of the candidate is
/// in place.
pub(super) struct RoleColumns<'a> {
    pub(super) role: &'static str,
    pub(super) document_kind: Option<&'static str>,
    pub(super) sheet_binding: Option<&'static str>,
    pub(super) sheet_binding_codec: Option<&'a str>,
    pub(super) sheet_place: Option<&'static str>,
    pub(super) sheet_medium: Option<u32>,
    pub(super) sheet_side: Option<u32>,
}

pub(super) fn role_columns(role: &FileRole) -> RoleColumns<'_> {
    let plain = |role| RoleColumns {
        role,
        document_kind: None,
        sheet_binding: None,
        sheet_binding_codec: None,
        sheet_place: None,
        sheet_medium: None,
        sheet_side: None,
    };
    match role {
        FileRole::Audio => plain("audio"),
        FileRole::Artwork => plain("artwork"),
        FileRole::DamagedImage => plain("damaged_image"),
        FileRole::Document { kind } => {
            RoleColumns { document_kind: Some(kind.as_str()), ..plain("document") }
        }
        FileRole::Other => plain("other"),
        FileRole::TrackSheet { binding, place, .. } => {
            let (sheet_binding, sheet_binding_codec) = match binding {
                SheetBinding::Resolved { .. } => ("resolved", None),
                SheetBinding::Unresolved { .. } => ("unresolved", None),
                SheetBinding::RefusedTiming { .. } => ("refused_timing", None),
                SheetBinding::RefusedCodec { codec } => ("refused_codec", Some(codec.as_str())),
            };
            let (sheet_place, sheet_medium, sheet_side) = place.columns();
            RoleColumns {
                role: "track_sheet",
                document_kind: None,
                sheet_binding: Some(sheet_binding),
                sheet_binding_codec,
                sheet_place: Some(sheet_place),
                sheet_medium,
                sheet_side,
            }
        }
    }
}

/// A sheet's binding from its column and its stored audio pairing. A bound or
/// timing-refused spelling with no pairing rows is a store nothing here
/// wrote.
#[expect(
    clippy::read_length_branch,
    reason = "refuses a stored sheet binding that pairs no audio file: checks what the statement returned for damage, and no screen shows it"
)]
pub(super) fn sheet_binding_of(
    stored: &str,
    audio_files: Vec<SheetAudioFile>,
    codec: Option<String>,
) -> Result<SheetBinding, DbError> {
    let malformed = |column: &str| {
        DbError::unreadable(
            format!("the {column} of a {stored} scan_candidate_file.sheet_binding"),
            "none",
        )
    };
    match stored {
        "resolved" => {
            if audio_files.is_empty() {
                return Err(malformed("audio pairing"));
            }
            Ok(SheetBinding::Resolved { files: audio_files })
        }
        "unresolved" => Ok(SheetBinding::Unresolved { files: audio_files }),
        "refused_timing" => {
            if audio_files.is_empty() {
                return Err(malformed("audio pairing"));
            }
            Ok(SheetBinding::RefusedTiming { files: audio_files })
        }
        "refused_codec" => {
            Ok(SheetBinding::RefusedCodec { codec: codec.ok_or_else(|| malformed("codec"))? })
        }
        other => Err(DbError::unreadable("scan_candidate_file.sheet_binding", other)),
    }
}
