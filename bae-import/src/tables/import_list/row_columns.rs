//! The JSON documents the placement's row fragments (`row_fragments`) give,
//! as the columns the rows are shaped from.

use super::*;
use crate::tables::folder_scans::{CoverVersionColumns, InvalidColumns, StoredImage};
use crate::tables::import_state::{
    RuntimeFactColumns, StoredCover, StoredProvenance, StoredReleaseLink,
};
use bae_catalogs::records::ReleaseRecord;
use serde::de::DeserializeOwned;
use std::path::PathBuf;

/// `json`, a document a row fragment gave as `what`.
fn document<T: DeserializeOwned>(what: &str, json: &str) -> Result<T, DbError> {
    serde_json::from_str(json).map_err(|error| DbError::unreadable_json(what, json, error))
}

/// A `ListedRow` document.
#[derive(serde::Deserialize)]
struct RowDocument {
    name: String,
    block: (Option<String>, Option<String>, Option<String>),
    failure: Option<String>,
    facts: (Option<String>, Option<i64>, Option<String>, Option<String>),
    invalid: Option<InvalidDocument>,
    imported: Option<ImportedDocument>,
    provenance: Option<(String, Option<String>, Option<String>)>,
    link: Option<LinkDocument>,
    cover: Option<CoverDocument>,
}

#[derive(serde::Deserialize)]
struct InvalidDocument {
    #[serde(deserialize_with = "path_column::from_hex")]
    folder: PathBuf,
    separable: i64,
    reason: String,
    shared_name: Option<String>,
    damaged_audio: Option<i64>,
    /// The album title and the stored credits.
    tags: Option<(String, Option<String>)>,
    cover: Option<StoredImage>,
}

#[derive(serde::Deserialize)]
struct ImportedDocument {
    release_id: String,
    album_id: String,
    cover: Option<String>,
    records: serde_json::Value,
    records_kind: Option<String>,
    /// How many of the release's files wait and are being sent, while any is
    /// queued.
    upload: Option<(i64, i64)>,
}

#[derive(serde::Deserialize)]
struct LinkDocument {
    content_hash: String,
    kind: String,
    source: Option<String>,
    release_id: Option<String>,
    partners: String,
    albums: String,
    records: String,
}

#[derive(serde::Deserialize)]
struct CoverDocument {
    kind: String,
    file_id: Option<String>,
    url: Option<String>,
    source: Option<String>,
    copies: String,
    /// The chosen folder image's path in hex, size and modification time.
    local: (String, Option<i64>, Option<i64>),
    embedded_source: Option<String>,
    /// The size, modification time and path in hex of the file the snapshot
    /// read the chosen artwork from.
    embedded: (Option<i64>, Option<i64>, String),
}

/// What one release's row holds past its placement.
pub(super) struct ReleaseColumns {
    pub(super) name: String,
    pub(super) block: (Option<String>, Option<String>, Option<String>),
    pub(super) failure: Option<crate::ImportFailureReason>,
    pub(super) facts: RuntimeFactColumns,
    pub(super) invalid: Option<InvalidColumns>,
    pub(super) shown: ShownColumns,
}

/// The columns a release's row shows past its placement, by whether this
/// device imported the release.
pub(super) enum ShownColumns {
    /// A release this device imported, which shows the library release.
    Imported {
        release: crate::ImportedRelease,
        /// The version of the library release's cover, if it has one.
        cover: Option<String>,
        /// The catalogs the library release's records name.
        records: Vec<ReleaseRecord>,
        /// What the library release's records identify it as: 'pressing',
        /// 'album', or nothing.
        records_kind: Option<String>,
        /// How the library release's upload stands.
        upload: Option<bae_records::UploadProgress>,
    },
    /// A release this device has not imported, which shows the candidate's
    /// draft source, link and cover. They are the candidate's own only while
    /// its state describes its current files.
    Candidate {
        /// The records of what the candidate's link names, as JSON.
        link_records: Option<String>,
        provenance: Option<StoredProvenance>,
        link: Option<StoredReleaseLink>,
        /// The chosen cover, with the versions of the files it reads.
        cover: Option<(StoredCover, CoverVersionColumns)>,
    },
}

impl ShownColumns {
    /// The library release a release this device imported shows.
    pub(super) fn imported(&self) -> Option<&crate::ImportedRelease> {
        match self {
            Self::Imported { release, .. } => Some(release),
            Self::Candidate { .. } => None,
        }
    }
}

/// The columns of the `ListedRow` document `json`.
pub(super) fn release_columns(json: &str) -> Result<ReleaseColumns, DbError> {
    let row: RowDocument = document("an import list row", json)?;
    let (identification, import_asked, save_failure_error, import_step) = row.facts;
    let shown = match row.imported {
        Some(imported) => ShownColumns::Imported {
            release: crate::ImportedRelease {
                release_id: imported.release_id,
                album_id: imported.album_id,
            },
            cover: imported.cover,
            records: bae_records::records::read::records_of_json(&imported.records.to_string())?,
            records_kind: imported.records_kind,
            upload: imported
                .upload
                .map(|(waiting, sending)| {
                    bae_records::records::queued_uploads::upload_progress(waiting, sending)
                })
                .transpose()?,
        },
        None => candidate_columns(row.provenance, row.link, row.cover)?,
    };
    Ok(ReleaseColumns {
        name: row.name,
        block: row.block,
        failure: row.failure.map(|detail| crate::ImportFailureReason { detail }),
        facts: RuntimeFactColumns::new(
            identification,
            import_asked.map(|asked| asked != 0),
            save_failure_error,
            import_step,
        ),
        invalid: row.invalid.map(invalid_columns).transpose()?,
        shown,
    })
}

fn invalid_columns(invalid: InvalidDocument) -> Result<InvalidColumns, DbError> {
    InvalidColumns::new(
        invalid.folder,
        invalid.separable != 0,
        (invalid.reason, invalid.shared_name, invalid.damaged_audio),
        invalid.tags,
        invalid.cover,
    )
}

fn candidate_columns(
    provenance: Option<(String, Option<String>, Option<String>)>,
    link: Option<LinkDocument>,
    cover: Option<CoverDocument>,
) -> Result<ShownColumns, DbError> {
    let (link, link_records) = match link {
        Some(link) => (
            Some(StoredReleaseLink::new(
                link.content_hash,
                link.kind,
                link.source,
                link.release_id,
                link.partners,
                link.albums,
            )),
            Some(link.records),
        ),
        None => (None, None),
    };
    // The hex of no path is empty text.
    let stored_path = |hex: &str| match hex {
        "" => Ok::<_, DbError>(None),
        hex => {
            hex::decode(hex).map(Some).map_err(|_error| DbError::unreadable("a stored path", hex))
        }
    };
    let cover = cover
        .map(|cover| {
            let (local_path, local_size, local_modified) = cover.local;
            let (embedded_size, embedded_modified, embedded_path) = cover.embedded;
            let versions = CoverVersionColumns::new(
                (stored_path(&local_path)?, local_size, local_modified),
                cover.embedded_source,
                (embedded_size, embedded_modified, stored_path(&embedded_path)?),
            );
            let stored =
                StoredCover::new(cover.kind, cover.file_id, cover.url, cover.source, cover.copies);
            Ok::<_, DbError>((stored, versions))
        })
        .transpose()?;
    Ok(ShownColumns::Candidate {
        link_records,
        provenance: provenance
            .map(|(kind, source, release_id)| StoredProvenance::new(kind, source, release_id)),
        link,
        cover,
    })
}

/// A `ListedHeader` document.
#[derive(serde::Deserialize)]
pub(super) struct HeaderDocument {
    #[serde(deserialize_with = "path_column::from_hex")]
    root: PathBuf,
    folder: String,
    pub(super) name: String,
    folded: i64,
    entries: i64,
    combinable: i64,
}

/// A group's header as its window shows it, from the `ListedHeader` document
/// `json`; `None` for a row that shows no header.
pub(super) fn header(json: &str) -> Result<Option<crate::list::ImportGroupHeader>, DbError> {
    let Some(header) = document::<Option<HeaderDocument>>("an import list header", json)? else {
        return Ok(None);
    };
    let root = header.root;
    let folder = hex::decode(&header.folder)
        .map_err(|_error| DbError::unreadable("a group's folder", &header.folder))?;
    let folder = crate::RelativeFolder::from_bytes(folder)
        .map_err(|error| DbError::unreadable("a group's folder", error.to_string()))?;
    Ok(Some(crate::list::ImportGroupHeader {
        group: crate::triage::TriageGroup {
            key: crate::FolderReleaseDecisionKey {
                watched_folder_path: root.clone(),
                relative_folder: folder,
            },
            name: header.name,
            combinable: header.combinable != 0,
        },
        watched_folder_path: root,
        expanded: header.folded == 0,
        entry_count: to_u32(header.entries, "a group's entry count")?,
    }))
}
