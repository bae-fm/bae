//! The covers a cover picker offers, as one query reads them from the
//! library's tables, filtered by source, each section with its count and the
//! whole with its total. A library release's picker offers its saved cover,
//! the covers its catalogs offered when this device last fetched them, and its
//! image files; its tables are `cover_gallery.sql`. A candidate's picker
//! offers the covers its link's catalogs offered, and the folder's images a
//! cover can be (`cover_gallery/candidate.rs`). Fetching writes the tables,
//! and a refetch replaces what they hold.

use super::*;
use crate::{CoverImageSource, CoverSelection};
use bae_catalogs::catalog::Catalog;
use bae_catalogs::remote_image::{CoverLabel, RemoteCover};

/// The cover gallery's part of the library's schema.
pub(super) const SCHEMA: bae_store::SchemaPart = bae_store::SchemaPart::new(&[
    include_str!("cover_gallery.sql"),
    include_str!("cover_gallery/candidate.sql"),
]);

mod candidate;

/// Which of the picker's sources a read shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoverFilter {
    All,
    ReleaseFiles,
    /// The covers one catalog offered.
    Catalog(Catalog),
}

impl CoverFilter {
    /// The text the read's SQL compares the sources with.
    fn column(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::ReleaseFiles => "release_files",
            Self::Catalog(catalog) => catalog.as_str(),
        }
    }
}

/// What this device knows of the catalog covers of the release or
/// candidate a picker shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoverFetch {
    /// No fetch has stored an answer yet.
    NotFetched,
    /// It names no catalog release, so there is nothing to offer.
    Unlinked,
    /// It names a catalog release, and the last fetch stored the covers its
    /// catalogs offered, which can be none.
    Linked,
}

impl CoverFetch {
    /// The fetch a stored `linked` column says was made, or none.
    fn of_column(linked: Option<bool>) -> Self {
        match linked {
            None => Self::NotFetched,
            Some(false) => Self::Unlinked,
            Some(true) => Self::Linked,
        }
    }
}

/// Which part of the picker a section is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoverSectionSource {
    Remote,
    ReleaseFiles,
}

/// What names a gallery cover to the person.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GalleryLabel {
    /// The cover the release has now.
    Current,
    /// An image file of the release or the folder, by its name there.
    File(String),
    /// A cover a catalog offers.
    Remote { label: CoverLabel, source: Catalog },
}

/// One cover the picker shows: what saving it selects (none for the saved
/// cover), where its image comes from, and its label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GalleryCover {
    pub selection: Option<CoverSelection>,
    pub image: CoverImageSource,
    pub label: GalleryLabel,
}

/// The covers of one source, and how many the filter leaves in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverSection {
    pub source: CoverSectionSource,
    pub items: Vec<GalleryCover>,
    pub count: u32,
}

/// A cover picker: a library release's saved cover, the sections the filter
/// shows, and the number of covers in all of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverGallery {
    pub current: Option<GalleryCover>,
    pub fetch: CoverFetch,
    pub sections: Vec<CoverSection>,
    pub total: u32,
}

/// The release's gallery under one filter: one row. The query returns the
/// saved cover only under all; remote covers under all or their catalog;
/// image files under all or the release's files.
const GALLERY: &str = "\
SELECT cover_version, fetched, shows_remote, remote_count, remote_covers, \
       shows_files, files_count, files, \
       (cover_version IS NOT NULL) + remote_count + files_count \
FROM (SELECT \
        CASE WHEN ?2 = 'all' THEN {cover_version} END AS cover_version, \
        (SELECT fetch.linked FROM release_cover_fetch AS fetch \
          WHERE fetch.release_id = ?1) AS fetched, \
        ?2 NOT IN ('release_files') AS shows_remote, \
        (SELECT COUNT(*) FROM release_cover_entry AS entry \
          WHERE entry.release_id = ?1 AND (?2 = 'all' OR entry.source = ?2)) AS remote_count, \
        (SELECT json_group_array(json(entry.cover) ORDER BY entry.position) \
           FROM release_cover_entry AS entry \
          WHERE entry.release_id = ?1 AND (?2 = 'all' OR entry.source = ?2)) AS remote_covers, \
        ?2 IN ('all', 'release_files') AS shows_files, \
        (SELECT COUNT(*) FROM release_files AS file \
          WHERE file.release_id = ?1 AND ?2 IN ('all', 'release_files') \
            AND file.content_type IN ({cover_types})) AS files_count, \
        (SELECT json_group_array(json_object('id', file.id, 'name', file.original_filename) \
                                 ORDER BY file.original_filename COLLATE natural_order, file.id) \
           FROM release_files AS file \
          WHERE file.release_id = ?1 AND ?2 IN ('all', 'release_files') \
            AND file.content_type IN ({cover_types})) AS files)";

#[derive(serde::Deserialize)]
struct FileJson {
    id: String,
    name: String,
}

fn gallery_sql() -> String {
    let cover_types: Vec<String> = bae_util::content_type::ContentType::SUPPORTED_COVERS
        .iter()
        .map(|content_type| format!("'{}'", content_type.as_str()))
        .collect();
    GALLERY
        .replace("{cover_version}", &bae_records::records::files::cover_version_sql("?1"))
        .replace("{cover_types}", &cover_types.join(", "))
}

fn release_image(release_id: &str, source: bae_records::GallerySource) -> CoverImageSource {
    CoverImageSource::ReleaseImage { release_id: release_id.to_string(), source }
}

/// Whose covers a picker shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoverGalleryOwner {
    /// A library release, by its id.
    Release(String),
    /// The release the scan stores at this key, before it is imported.
    Candidate(crate::CandidateKey),
}

/// Whose covers a picker shows, and the source it shows them from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverGalleryRequest {
    pub owner: CoverGalleryOwner,
    pub filter: CoverFilter,
}

/// What a read of a picker delivered: the request it answered, and its
/// covers. A read for no request delivers neither.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverGallerySnapshot {
    pub request: Option<CoverGalleryRequest>,
    pub gallery: Option<CoverGallery>,
}

/// The covers `request`'s picker offers, as one read of the library's tables.
fn cover_gallery_on(
    sql: &impl coven::SqlRead,
    request: &CoverGalleryRequest,
) -> Result<CoverGallery, DbError> {
    match &request.owner {
        CoverGalleryOwner::Release(release_id) => {
            release_gallery_on(sql, release_id, request.filter)
        }
        CoverGalleryOwner::Candidate(key) => {
            candidate::candidate_gallery_on(sql, key, request.filter)
        }
    }
}

/// The section of the covers the catalogs offered, each selecting itself.
fn remote_section(covers: Vec<RemoteCover>, count: u32) -> CoverSection {
    CoverSection {
        source: CoverSectionSource::Remote,
        items: covers
            .into_iter()
            .map(|cover| GalleryCover {
                selection: Some(CoverSelection::Remote(cover.image.clone(), cover.source)),
                image: CoverImageSource::Remote { image: cover.image },
                label: GalleryLabel::Remote { label: cover.label, source: cover.source },
            })
            .collect(),
        count,
    }
}

/// The covers `release_id`'s picker offers under `filter`, as one read.
fn release_gallery_on(
    sql: &impl coven::SqlRead,
    release_id: &str,
    filter: CoverFilter,
) -> Result<CoverGallery, DbError> {
    let (
        version,
        fetched,
        shows_remote,
        remote_count,
        remote_json,
        shows_files,
        files_count,
        files_json,
        total,
    ) = sql.query_row(&gallery_sql(), params![release_id, filter.column()], |row| {
        Ok((
            row.get::<_, Option<String>>(0)?,
            row.get::<_, Option<bool>>(1)?,
            row.get::<_, bool>(2)?,
            row.get::<_, u32>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, bool>(5)?,
            row.get::<_, u32>(6)?,
            row.get::<_, String>(7)?,
            row.get::<_, u32>(8)?,
        ))
    })?;
    let remote: Vec<RemoteCover> = serde_json::from_str(&remote_json)
        .map_err(|error| DbError::unreadable_json("a stored gallery cover", &remote_json, error))?;
    let files: Vec<FileJson> = serde_json::from_str(&files_json)
        .map_err(|error| DbError::unreadable_json("a release's cover files", &files_json, error))?;
    let mut sections = Vec::new();
    if shows_remote {
        sections.push(remote_section(remote, remote_count));
    }
    if shows_files {
        sections.push(CoverSection {
            source: CoverSectionSource::ReleaseFiles,
            items: files
                .into_iter()
                .map(|file| GalleryCover {
                    selection: Some(CoverSelection::Local(file.id.clone())),
                    image: release_image(
                        release_id,
                        bae_records::GallerySource::ReleaseFile { file_id: file.id },
                    ),
                    label: GalleryLabel::File(file.name),
                })
                .collect(),
            count: files_count,
        });
    }
    Ok(CoverGallery {
        current: bae_records::cover_ref(release_id, version).map(|image| GalleryCover {
            selection: None,
            image: release_image(release_id, bae_records::GallerySource::Cover(image)),
            label: GalleryLabel::Current,
        }),
        fetch: CoverFetch::of_column(fetched),
        sections,
        total,
    })
}

impl ImportTables {
    /// Follow the covers that the picker shows for the release or candidate
    /// and the source `initial` names, or nothing. A new request changes them
    /// on the same query, and coven reruns the query when a write changes
    /// the fetched covers, a release's files or saved cover, or a
    /// candidate's link, draft or folder.
    pub fn subscribe_cover_gallery(
        &self,
        initial: Option<CoverGalleryRequest>,
    ) -> coven::ReconfigurableLiveQuery<Option<CoverGalleryRequest>, Option<CoverGallery>, DbError>
    {
        self.store.subscribe_reconfigurable(initial, |request, sql| match request {
            None => Ok(None),
            Some(request) => Ok(Some(cover_gallery_on(&sql, request)?)),
        })
    }

    /// Store what the catalogs offered `release_id`'s picker, in the order
    /// offered, in place of what the last fetch stored; `None` when the
    /// release names no catalog release.
    pub async fn replace_release_covers(
        &self,
        release_id: &str,
        covers: Option<Vec<RemoteCover>>,
    ) -> Result<(), DbError> {
        let release_id = release_id.to_string();
        self.store
            .write(move |sql| {
                sql.execute("DELETE FROM release_cover_fetch WHERE release_id = ?", [&release_id])?;
                sql.execute(
                    "INSERT INTO release_cover_fetch (release_id, linked) VALUES (?, ?)",
                    params![release_id, covers.is_some()],
                )?;
                for (position, cover) in covers.iter().flatten().enumerate() {
                    sql.execute(
                        "INSERT INTO release_cover_entry (release_id, position, source, cover) \
                         VALUES (?, ?, ?, ?)",
                        params![
                            release_id,
                            position as i64,
                            cover.source.as_str(),
                            serde_json::to_string(cover)
                                .map_err(|error| DbError::encode("a gallery cover", error))?,
                        ],
                    )?;
                }
                Ok(())
            })
            .await
    }
}

#[cfg(test)]
#[path = "cover_gallery_tests.rs"]
mod tests;
