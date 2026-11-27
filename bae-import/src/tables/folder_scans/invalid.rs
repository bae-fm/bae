//! A folder that failed validation, as its row under Skipped and the pane
//! looking into it read it: what its scan row stores. The scan judged the
//! folder's files when it wrote the row, so no read judges them again.

use super::columns::proposed_role_of;
use super::*;
use crate::folder_scanner::{FileVersion, InvalidReason, ProposedRole, SkippedTags};
use crate::mapping::file_name;
use crate::triage::InvalidRow;
use crate::CandidateKey;
use crate::{InvalidCandidateDetail, MappingFile, MappingImage, MappingRole};

/// An image as [`super::fragments::LeadingImage`] gives it: its path in hex,
/// its size and its modification time.
pub(crate) type StoredImage = (String, i64, i64);

/// The value of the JSON a statement gave as `what`, where it gave any.
fn stored_json<T: serde::de::DeserializeOwned>(
    what: &str,
    json: Option<String>,
) -> Result<Option<T>, DbError> {
    json.map(|json| {
        serde_json::from_str(&json).map_err(|error| DbError::unreadable_json(what, json, error))
    })
    .transpose()
}

/// The image a statement's [`super::fragments::LeadingImage`] gives.
fn leading_image(
    spread: Option<&super::fragments::LeadingImage>,
) -> Result<Option<StoredImage>, DbError> {
    let json = spread.map(|spread| spread.leading_image.clone());
    Ok(stored_json::<Option<StoredImage>>("a skipped folder's image", json)?.flatten())
}

/// A folder that failed validation, as its scan row stores it.
#[derive(Clone, PartialEq)]
pub(crate) struct InvalidColumns {
    folder: PathBuf,
    separable: bool,
    reason: (String, Option<String>, Option<i64>),
    tags: Option<SkippedTags>,
    cover: Option<(PathBuf, i64, i64)>,
}

impl InvalidColumns {
    /// The columns of a folder that failed validation: its folder, whether a
    /// grouping reads it, why it failed, the album title and stored credits
    /// its tags give it, and the image it leads with.
    pub(crate) fn new(
        folder: PathBuf,
        separable: bool,
        reason: (String, Option<String>, Option<i64>),
        tags: Option<(String, Option<String>)>,
        cover: Option<StoredImage>,
    ) -> Result<Self, DbError> {
        let cover = cover
            .map(|(hex, size, modified)| {
                let bytes = hex::decode(&hex)
                    .map_err(|_error| DbError::unreadable("a stored path", hex))?;
                Ok::<_, DbError>((path_column::decode(bytes)?, size, modified))
            })
            .transpose()?;
        let tags = tags
            .map(|(album_title, credits)| {
                let artist = crate::tables::stored_credits::line_of(credits.as_deref()).map_err(
                    |error| {
                        DbError::unreadable_json(
                            "a skipped folder's credits",
                            credits.clone().unwrap_or_default(),
                            error,
                        )
                    },
                )?;
                Ok::<_, DbError>(SkippedTags { album_title, artist })
            })
            .transpose()?;
        Ok(Self { folder, separable, reason, tags, cover })
    }

    /// The columns of a row of [`super::fragments::ScanItems`]; a row of
    /// another kind holds no reason, which reads as `None`.
    pub(crate) fn of(scan: &super::fragments::ScanItems) -> Result<Option<Self>, DbError> {
        let Some(reason) = scan.invalid_reason.clone() else {
            return Ok(None);
        };
        let cover = leading_image(scan.invalid_cover.as_ref())?;
        let tags = scan
            .invalid_album_title
            .clone()
            .map(|album_title| (album_title, scan.invalid_credits.clone()));
        Self::new(
            path_column::decode(scan.invalid_folder.clone())?,
            scan.invalid_separable,
            (reason, scan.invalid_shared_name.clone(), scan.invalid_damaged_audio),
            tags,
            cover,
        )
        .map(Some)
    }

    /// What the folder's tags call it, when they name an album.
    pub(crate) fn tags(self) -> Option<SkippedTags> {
        self.tags
    }

    /// The row of the folder stored at `key` under `watched_folder_path`,
    /// named `folder_name`.
    pub(crate) fn row(
        self,
        key: CandidateKey,
        folder_name: String,
        watched_folder_path: PathBuf,
    ) -> Result<InvalidRow, DbError> {
        let reason = match self.reason {
            (kind, Some(name), None) if kind == "same_name" => InvalidReason::SameName { name },
            (kind, None, Some(count)) if kind == "damaged_audio" => InvalidReason::DamagedAudio {
                count: u32::try_from(count).map_err(|_too_large| {
                    DbError::unreadable("scan_candidate.invalid_damaged_audio", count)
                })?,
            },
            (kind, None, None) if kind == "no_valid_audio" => InvalidReason::NoValidAudio,
            other => return Err(DbError::unreadable("scan_candidate.invalid_reason", other)),
        };
        let cover = match self.cover {
            Some((path, size, modified_at_ns)) => {
                Some(crate::CoverImageSource::Import(crate::ImportPicture::Folder {
                    file: FileVersion {
                        path,
                        size: to_u64(size, "a skipped image's size")?,
                        modified_at_ns,
                    },
                }))
            }
            None => None,
        };
        Ok(InvalidRow {
            candidate_key: key,
            folder: self.folder,
            folder_name,
            watched_folder_path,
            separable: self.separable,
            reason,
            tags: self.tags,
            cover,
        })
    }
}

// The scan row of the folder that failed validation stored at `key`.
sqlive_compose::read!(
    InvalidScan(key: CandidateKey),
    r#"
    SELECT listed.*
    FROM @crate::tables::folder_scans::listed_fragments::ListedScan AS listed
    WHERE listed.listed_key = :key AND listed.listed_invalid_reason IS NOT NULL
"#
);

/// The folder that failed validation stored at `key`, as its row shows it;
/// `None` when no folder that failed validation is stored there. Watched
/// roots never overlap and keys are absolute paths, so at most one root
/// holds it.
pub(crate) fn load_invalid_row(
    sql: &impl coven::SqlRead,
    key: &CandidateKey,
) -> Result<Option<InvalidRow>, DbError> {
    let request = InvalidScanParams { key: key.clone() };
    let Some(row) = bae_store::statement_optional(sql, &request)? else {
        return Ok(None);
    };
    let scan = row.listed;
    // The statement keeps the rows that store a reason.
    let reason = scan.listed_invalid_reason.ok_or_else(|| {
        DbError::unreadable(format!("the reason folder {key} failed validation"), None::<String>)
    })?;
    let tags = stored_json("a skipped folder's tags", scan.listed_invalid_tags)?;
    let cover = leading_image(scan.listed_invalid_cover.as_ref())?;
    let columns = InvalidColumns::new(
        path_column::decode(scan.listed_folder)?,
        scan.listed_grouping_key.is_some(),
        (reason, scan.listed_shared_name, scan.listed_damaged_audio),
        tags,
        cover,
    )?;
    columns.row(key.clone(), scan.listed_name, path_column::decode(scan.listed_root)?).map(Some)
}

// The files but the images of the folder that failed validation stored at
// `key`, each with whether the preview player holds the whole of it, which is
// the stretch an audio file of this pane plays.
sqlive_compose::read!(
    SkippedPaneFiles(key: CandidateKey) {
        relative_path: String,
        absolute_path: Vec<u8>,
        size: i64,
        role: String,
        audio_damage: Option<String>,
        document_kind: Option<String>,
        previewing: bool
    },
    r#"
    SELECT f.relative_path, f.absolute_path, f.size, f.role, f.audio_damage, f.document_kind,
           previewed.previewed_state IS NOT NULL AS previewing
    FROM scan_candidate AS c
    JOIN scan_skipped_file AS f
        ON f.watched_folder_path = c.watched_folder_path AND f.candidate_path = c.path
    LEFT JOIN @bae_playback::tables::now::PreviewedWindow AS previewed
        ON previewed.previewed_path = f.absolute_path
       AND previewed.previewed_start_sample = 0
       AND previewed.previewed_end_sample IS NULL
    WHERE c.path = :key AND c.kind = 'invalid' AND f.role <> 'artwork'
    ORDER BY f.position
"#
);

/// The files of the folder that failed validation stored at the key bound
/// as `?1`, joined as `f`, for a statement that adds which roles it keeps.
const INVALID_FILES: &str = "FROM scan_candidate AS c \
     JOIN scan_skipped_file AS f \
       ON f.watched_folder_path = c.watched_folder_path AND f.candidate_path = c.path \
     WHERE c.path = ?1 AND c.kind = 'invalid'";

/// The folder that failed validation stored at `key`, as the pane looking
/// into it reads it: its row, the images that decode, and every other file,
/// each in name order. Each statement reads by the key the request names, and
/// SQL keeps each part's files by their role; `None` when no folder that
/// failed validation is stored there.
pub(crate) fn load_invalid_detail(
    sql: &impl coven::SqlRead,
    key: &CandidateKey,
) -> Result<Option<InvalidCandidateDetail>, DbError> {
    let row = load_invalid_row(sql, key)?;
    let images = sql.query(
        &format!(
            "SELECT f.relative_path, f.absolute_path, f.size, f.modified_at_ns {INVALID_FILES} \
               AND f.role = 'artwork' \
             ORDER BY f.position"
        ),
        [key],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                path_column::get(row, 1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
            ))
        },
    )?;
    let files = bae_store::statement_rows(sql, &SkippedPaneFilesParams { key: key.clone() })?;
    let Some(candidate) = row else {
        return Ok(None);
    };
    let images = images
        .into_iter()
        .map(|(relative_path, path, size, modified_at_ns)| {
            Ok(MappingImage {
                name: file_name(&relative_path).to_string(),
                file_id: relative_path,
                file: FileVersion {
                    path,
                    size: to_u64(size, "a skipped image's size")?,
                    modified_at_ns,
                },
            })
        })
        .collect::<Result<Vec<_>, DbError>>()?;
    let files = files
        .into_iter()
        .map(|file| {
            let role = mapping_role(proposed_role_of(
                &file.role,
                file.audio_damage.as_deref(),
                file.document_kind.as_deref(),
            )?);
            let path = path_column::decode(file.absolute_path)?;
            Ok(MappingFile {
                file_id: file.relative_path.clone(),
                name: file.relative_path,
                size: to_u64(file.size, "a skipped file's size")?,
                audition: (role == MappingRole::Audio).then(|| crate::FileAudition {
                    target: bae_track_audio::PreviewTarget::whole_file(path),
                    previewing: file.previewing,
                }),
                role,
            })
        })
        .collect::<Result<Vec<_>, DbError>>()?;
    Ok(Some(InvalidCandidateDetail { candidate, images, files }))
}

/// What the pane lists a skipped file as. bae never imports a skipped folder,
/// so it never binds a cue sheet in one to audio, and the pane lists the
/// sheet as a document.
fn mapping_role(role: ProposedRole) -> MappingRole {
    match role {
        ProposedRole::Audio => MappingRole::Audio,
        ProposedRole::DamagedAudio { damage } => MappingRole::DamagedAudio { damage },
        ProposedRole::Cue | ProposedRole::Document { .. } => MappingRole::Document,
        ProposedRole::Image => MappingRole::Image,
        ProposedRole::DamagedImage => MappingRole::DamagedImage,
        ProposedRole::Other => MappingRole::Other,
    }
}
