//! The covers a library release's picker offers, as the tables read them: the
//! release's saved cover, the covers its catalogs offered when this device
//! last fetched them, and its image files, filtered by source, each section
//! with its count and the whole with its total. Its tables are
//! `cover_gallery.sql`; fetching writes them, and a refetch replaces them.

use super::*;
use crate::{CoverImageSource, CoverSelection};
use bae_catalogs::catalog::Catalog;
use bae_catalogs::remote_image::{CoverLabel, RemoteCover};

/// The cover gallery's part of the library's schema.
pub(super) const SCHEMA: bae_store::SchemaPart =
    bae_store::SchemaPart::new(&[include_str!("cover_gallery.sql")]);

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

/// What this device knows of the release's catalog covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoverFetch {
    /// No fetch has stored an answer yet.
    NotFetched,
    /// The release names no catalog release, so there is nothing to offer.
    Unlinked,
    /// The release is linked; the covers stored are what its catalogs offered,
    /// possibly none.
    Linked,
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
    /// An image file of the release, by its name in the release.
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

/// A library release's cover picker: its saved cover, the sections the filter
/// shows, and the number of covers in all of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverGallery {
    pub current: Option<GalleryCover>,
    pub fetch: CoverFetch,
    pub sections: Vec<CoverSection>,
    pub total: u32,
}

/// The release's gallery under one filter: one row. The saved cover is
/// offered with every source; remote covers when the filter is all or their
/// catalog; image files when it is all or the release's files.
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

/// The release whose covers a picker shows, and the source it shows them from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverGalleryRequest {
    pub release_id: String,
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
    let release_id = request.release_id.as_str();
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
    ) = sql.query_row(&gallery_sql(), params![release_id, request.filter.column()], |row| {
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
        sections.push(CoverSection {
            source: CoverSectionSource::Remote,
            items: remote
                .into_iter()
                .map(|cover| GalleryCover {
                    selection: Some(CoverSelection::Remote(cover.image.clone(), cover.source)),
                    image: CoverImageSource::Remote { image: cover.image },
                    label: GalleryLabel::Remote { label: cover.label, source: cover.source },
                })
                .collect(),
            count: remote_count,
        });
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
        fetch: match fetched {
            None => CoverFetch::NotFetched,
            Some(false) => CoverFetch::Unlinked,
            Some(true) => CoverFetch::Linked,
        },
        sections,
        total,
    })
}

impl ImportTables {
    /// Follow the covers the picker of the release `initial` names offers
    /// under its source, or none. Another release or source is a new request
    /// on the same query, and a write to what the picker reads (a fetch's
    /// covers, the release's files or saved cover) reaches it by itself.
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
