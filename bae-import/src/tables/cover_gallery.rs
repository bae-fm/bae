//! The covers a cover picker offers, as one query reads them from the
//! library's tables, filtered by source, each section with its count and the
//! whole with its total, and the cover the picker focuses with where it sits
//! among them. A library release's picker offers its saved cover,
//! the covers its catalogs offered when this device last fetched them, and its
//! image files; its tables are `cover_gallery.sql`. A candidate's picker
//! offers the covers its link's catalogs offered, and the folder's images a
//! cover can be (`cover_gallery/candidate.rs`). Fetching writes the tables,
//! and a refetch replaces what they hold.

use super::*;
use crate::{CoverImageSource, CoverSelection};
use bae_catalogs::catalog::Catalog;
use bae_catalogs::remote_image::{CoverLabel, RemoteCover};
use bae_store::WriteStatements;

mod candidate;
mod walk;

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
    /// The release or candidate names no catalog release, so there is
    /// nothing to offer.
    Unlinked,
    /// The release or candidate names a catalog release, and the last fetch
    /// stored the covers its catalogs offered, which can be none.
    Linked,
}

impl CoverFetch {
    /// What the last fetch found, as its stored `linked` column records it,
    /// or that no fetch has stored an answer.
    fn of_column(linked: Option<bool>) -> Self {
        match linked {
            None => Self::NotFetched,
            Some(false) => Self::Unlinked,
            Some(true) => Self::Linked,
        }
    }
}

/// What names a gallery cover to the person, and what saving it selects.
/// Saving the cover the release has now selects nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GalleryLabel {
    /// The cover the release has now.
    Current,
    /// An image file of the release or the folder, by its name there.
    File { name: String, selection: CoverSelection },
    /// A cover a catalog offers.
    Remote { label: CoverLabel, source: Catalog, selection: CoverSelection },
}

/// What names one cover of a picker: the saved cover, a catalog's image by
/// its address, or an image file by its id.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CoverKey {
    Saved,
    Remote(String),
    File(String),
}

/// One cover the picker shows: its key, where its image comes from, and its
/// label, which holds what saving it selects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GalleryCover {
    pub key: CoverKey,
    pub image: CoverImageSource,
    pub label: GalleryLabel,
}

/// The covers of one source, and how many the filter leaves in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverSection {
    pub items: Vec<GalleryCover>,
    pub count: u32,
}

/// The items before and after the one a reader focuses, wrapping around the
/// ends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Neighbors<K> {
    pub previous: K,
    pub next: K,
}

/// The cover a picker focuses: the cover, where it sits among the covers in
/// the order the picker walks them (the saved cover, then each section's), and
/// the covers before and after it, which a picker of one cover lacks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GalleryFocus {
    pub cover: GalleryCover,
    pub position: u32,
    pub neighbors: Option<Neighbors<CoverKey>>,
}

/// A cover picker: a library release's saved cover, the catalogs' covers
/// and the image files, each section only while the filter shows it, the
/// number of covers in all of them, and the cover it focuses, or none while
/// it shows no cover.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverGallery {
    pub current: Option<GalleryCover>,
    pub fetch: CoverFetch,
    pub remote: Option<CoverSection>,
    pub files: Option<CoverSection>,
    pub total: u32,
    pub focus: Option<GalleryFocus>,
}

// The gallery of the library release `:release` under one filter: its saved
// cover only under all and only when the cover is made from no image file of
// the release (one made from an image file is that file's entry), catalog
// covers under all or their catalog, image files under all or the release's
// files, each file in name order. The release focuses its saved cover, or the
// file the cover is made from, unless the request names another.
sqlive_compose::read!(
    writer ReleaseGallery(
        release: String,
        filter: &'static str,
        focus_kind: Option<&'static str>,
        focus_key: Option<String>
    ) { fetched: Option<bool> },
    r#"
    SELECT (SELECT fetch.linked FROM release_cover_fetch AS fetch
            WHERE fetch.release_id = :release) AS fetched,
           walk.*
    FROM @crate::tables::cover_gallery::walk::PickerWalk(
        filter: :filter, focus_kind: :focus_kind, focus_key: :focus_key,
        items: (
            SELECT 0, 0, 'saved', NULL, 1,
                   json_object('kind', 'saved', 'version', saved.version)
            FROM @bae_records::records::files::CoverVersion AS saved
            WHERE saved.id = :release AND :filter = 'all'
              AND saved.version IS NOT NULL AND saved.source_file IS NULL
            UNION ALL
            SELECT 1, entry.position, 'remote', json_extract(entry.cover, '$.image.url'), 0,
                   json_object('kind', 'remote', 'cover', json(entry.cover))
            FROM release_cover_entry AS entry
            WHERE entry.release_id = :release AND :filter IN ('all', entry.source)
            UNION ALL
            SELECT 2,
                   ROW_NUMBER() OVER (
                       ORDER BY file.original_filename COLLATE natural_order, file.file_id),
                   'file', file.file_id,
                   COALESCE(file.file_id = (
                       SELECT made.source_file
                       FROM @bae_records::records::files::CoverVersion AS made
                       WHERE made.id = :release), 0),
                   json_object('kind', 'release_file', 'id', file.file_id,
                               'name', file.original_filename)
            FROM @bae_records::records::spread_fragments::ReleaseFileRow AS file
            WHERE file.release_id = :release AND :filter IN ('all', 'release_files')
              AND file.content_type IN (
                  SELECT shown.content_type
                  FROM @bae_records::records::file_fragments::ShowableImageTypes AS shown)
        )) AS walk
"#
);

sqlive_compose::write!(
    DeleteReleaseCoverFetch(release: &'p str),
    r#"DELETE FROM release_cover_fetch WHERE release_id = :release"#
);

sqlive_compose::write!(
    InsertReleaseCoverFetch(release: &'p str, linked: bool),
    r#"INSERT INTO release_cover_fetch (release_id, linked) VALUES (:release, :linked)"#
);

sqlive_compose::write!(
    InsertReleaseCoverEntry(release: &'p str, position: i64, source: &'static str, cover: &'p str),
    r#"
    INSERT INTO release_cover_entry (release_id, position, source, cover)
    VALUES (:release, :position, :source, :cover)
"#
);

/// Whose covers a picker shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoverGalleryOwner {
    /// A library release, by its id.
    Release(String),
    /// The release the scan stores at this key, before bae imports it.
    Candidate(crate::CandidateKey),
}

/// Whose covers a picker shows, the source it shows them from, and the cover
/// it focuses: the one `focus` names when it shows it, else its own (a
/// release's saved cover, a candidate's chosen cover), else its first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverGalleryRequest {
    pub owner: CoverGalleryOwner,
    pub filter: CoverFilter,
    pub focus: Option<CoverKey>,
}

/// What a read of a picker delivered: the request it answered, and its
/// covers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverGallerySnapshot {
    pub request: CoverGalleryRequest,
    pub gallery: CoverGallery,
}

/// What a gallery read returns: the picker's walk of its covers, and the
/// `linked` column of the owner's last fetch.
type GalleryRow = (walk::PickerWalk, Option<bool>);

/// The read of the covers `request`'s picker offers, one statement of the
/// library's tables for a release's picker and one for a candidate's.
fn cover_gallery_read(request: &CoverGalleryRequest) -> coven::ReadAs<GalleryRow> {
    let (focus_kind, focus_key) = request.focus.as_ref().map(CoverKey::columns).unzip();
    let focus_key = focus_key.flatten().map(str::to_string);
    let filter = request.filter.column();
    match &request.owner {
        CoverGalleryOwner::Release(release_id) => {
            let statement =
                ReleaseGalleryParams { release: release_id.clone(), filter, focus_kind, focus_key };
            coven::ReadAs::new(statement, |row| (row.walk, row.fetched))
        }
        CoverGalleryOwner::Candidate(key) => {
            let statement = candidate::CandidateGalleryParams {
                key: key.clone(),
                filter,
                focus_kind,
                focus_key,
            };
            coven::ReadAs::new(statement, |row| (row.walk, row.fetched))
        }
    }
}

impl ImportTables {
    /// Follow the covers that the picker shows for the release or candidate
    /// and the source `initial` names, each with the request it answers, or
    /// nothing. A new request changes them on the same query, and coven
    /// reruns the query when a write changes the fetched covers, a release's
    /// files or saved cover, or a candidate's link, draft or folder.
    pub fn subscribe_cover_gallery(
        &self,
        initial: Option<CoverGalleryRequest>,
    ) -> coven::ReconfigurableLiveQuery<
        Option<CoverGalleryRequest>,
        Option<CoverGallerySnapshot>,
        DbError,
    > {
        self.store
            .subscribe_reconfigurable(initial, |request| {
                Ok(coven::Maybe(request.as_ref().map(cover_gallery_read)))
            })
            .process(|request, galleries| {
                let Some((request, galleries)) = bae_store::requested(request, galleries) else {
                    return Ok(None);
                };
                let (walk, fetched) =
                    bae_store::exactly_one::<coven::ReadAs<GalleryRow>, _>(galleries)?;
                let release_id = match &request.owner {
                    CoverGalleryOwner::Release(release_id) => Some(release_id.as_str()),
                    CoverGalleryOwner::Candidate(_) => None,
                };
                let gallery = walk.gallery(fetched, release_id)?;
                Ok(Some(CoverGallerySnapshot { request: request.clone(), gallery }))
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
                sql.run(&DeleteReleaseCoverFetchParams { release: &release_id })?;
                sql.run(&InsertReleaseCoverFetchParams {
                    release: &release_id,
                    linked: covers.is_some(),
                })?;
                for (position, cover) in covers.iter().flatten().enumerate() {
                    sql.run(&InsertReleaseCoverEntryParams {
                        release: &release_id,
                        position: position as i64,
                        source: cover.source.as_str(),
                        cover: &serde_json::to_string(cover)
                            .map_err(|error| DbError::encode("a gallery cover", error))?,
                    })?;
                }
                Ok(())
            })
            .await
    }
}

#[cfg(test)]
#[path = "cover_gallery_tests.rs"]
mod tests;
