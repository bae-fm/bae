//! The part every picker's statement shares: its covers in the order the
//! picker walks them, each section's covers and count, the total, and the
//! cover the picker focuses with its place among them and the covers before
//! and after it.

use super::*;
use crate::folder_scanner::FileVersion;

/// The rest of a picker's statement, after the statement's own tables end
/// with `items (rank, pos, kind, key, preferred, item)`: one row for each
/// cover it shows, its section's rank (0 the saved cover, 1 a catalog's, 2 an
/// image file), its place in the section, its key's kind and value, whether
/// it is the owner's own cover, and the cover as an [`ItemJson`]. The
/// statement binds `:filter`, and the focused cover's key as `:focus_kind`
/// and `:focus_key` (both NULL for none); `fetched` is the expression for the
/// fetch's `linked` column.
///
/// The picker focuses the cover the request names, else the owner's own
/// cover, else its first, while it shows any. The covers before and after it
/// wrap around the ends, and there are none when the picker shows one cover.
pub(super) fn walk_sql(fetched: &str) -> String {
    // The statement names `walk` twice and `items` once: SQLite expands a
    // common table at each place a statement names it, and refuses a
    // statement whose expansions open too many cursors.
    format!(
        ",
walk AS MATERIALIZED (
    SELECT *, ROW_NUMBER() OVER (ORDER BY rank, pos) - 1 AS n, COUNT(*) OVER () AS total
    FROM items
),
focused AS (
    SELECT n FROM walk
    ORDER BY (kind, key) IS (:focus_kind, :focus_key) DESC, preferred DESC, n
    LIMIT 1
)
SELECT {fetched},
       :filter <> 'release_files',
       :filter IN ('all', 'release_files'),
       MAX(CASE WHEN w.rank = 0 THEN w.item END),
       json_group_array(json(w.item) ORDER BY w.n) FILTER (WHERE w.rank = 1),
       COUNT(*) FILTER (WHERE w.rank = 1),
       json_group_array(json(w.item) ORDER BY w.n) FILTER (WHERE w.rank = 2),
       COUNT(*) FILTER (WHERE w.rank = 2),
       COUNT(w.n),
       CASE WHEN COUNT(w.n) > 0 THEN json_object(
           'item', json(MAX(CASE WHEN w.n = f.n THEN w.item END)),
           'position', MAX(f.n),
           'neighbors', CASE WHEN COUNT(w.n) > 1 THEN json_object(
               'previous', json(MAX(CASE WHEN w.n = (f.n + w.total - 1) % w.total
                                         THEN json_object('kind', w.kind, 'key', w.key) END)),
               'next', json(MAX(CASE WHEN w.n = (f.n + 1) % w.total
                                     THEN json_object('kind', w.kind, 'key', w.key) END))) END)
       END
FROM walk AS w LEFT JOIN focused AS f"
    )
}

/// One cover as a picker's statement returns it.
#[derive(serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(super) enum ItemJson {
    /// The library release's saved cover, at its version.
    Saved { version: String },
    /// A cover a fetch stored, or one the candidate's linked pressing offers
    /// while bae read the draft from it.
    Remote { cover: RemoteCover },
    /// An image file of the library release.
    ReleaseFile { id: String, name: String },
    /// An image of the candidate's folder, at the version the scan read.
    FolderFile {
        id: String,
        #[serde(deserialize_with = "path_column::from_hex")]
        path: std::path::PathBuf,
        size: u64,
        modified_at_ns: i64,
    },
}

/// The key of a cover as a picker's statement returns it.
#[derive(serde::Deserialize)]
struct KeyJson {
    kind: String,
    key: Option<String>,
}

/// The covers before and after the focused one, as a picker's statement
/// returns them.
#[derive(serde::Deserialize)]
struct NeighborsJson {
    previous: KeyJson,
    next: KeyJson,
}

/// The focused cover as a picker's statement returns it.
#[derive(serde::Deserialize)]
struct FocusJson {
    item: ItemJson,
    position: u32,
    neighbors: Option<NeighborsJson>,
}

impl CoverKey {
    /// The kind and value a picker's statement compares a cover's key with.
    pub(super) fn columns(&self) -> (&'static str, Option<&str>) {
        match self {
            Self::Saved => ("saved", None),
            Self::Remote(url) => ("remote", Some(url)),
            Self::File(id) => ("file", Some(id)),
        }
    }

    fn of_json(key: KeyJson) -> Result<Self, DbError> {
        match (key.kind.as_str(), key.key) {
            ("saved", None) => Ok(Self::Saved),
            ("remote", Some(url)) => Ok(Self::Remote(url)),
            ("file", Some(id)) => Ok(Self::File(id)),
            (kind, key) => Err(DbError::unreadable("a gallery cover's key", (kind, key))),
        }
    }
}

impl ItemJson {
    /// The cover this item is in the picker of the library release
    /// `release_id`, or of a candidate (`None`).
    fn cover(self, release_id: Option<&str>) -> Result<GalleryCover, DbError> {
        let release = |what: &str| {
            release_id.ok_or_else(|| DbError::unreadable("a candidate's gallery cover", what))
        };
        let remote = |cover: RemoteCover| GalleryCover {
            key: CoverKey::Remote(cover.image.url.clone()),
            label: GalleryLabel::Remote {
                label: cover.label,
                source: cover.source,
                selection: CoverSelection::Remote(cover.image.clone(), cover.source),
            },
            image: CoverImageSource::Remote { image: cover.image },
        };
        Ok(match self {
            Self::Saved { version } => {
                let release_id = release("saved")?;
                let image = bae_records::cover_ref(release_id, Some(version))
                    .ok_or_else(|| DbError::unreadable("a release's saved cover", release_id))?;
                GalleryCover {
                    key: CoverKey::Saved,
                    image: release_image(release_id, bae_records::GallerySource::Cover(image)),
                    label: GalleryLabel::Current,
                }
            }
            Self::Remote { cover } => remote(cover),
            Self::ReleaseFile { id, name } => GalleryCover {
                key: CoverKey::File(id.clone()),
                label: GalleryLabel::File { name, selection: CoverSelection::Local(id.clone()) },
                image: release_image(
                    release("release_file")?,
                    bae_records::GallerySource::ReleaseFile { file_id: id },
                ),
            },
            Self::FolderFile { id, path, size, modified_at_ns } => GalleryCover {
                key: CoverKey::File(id.clone()),
                label: GalleryLabel::File {
                    name: id.clone(),
                    selection: CoverSelection::Local(id),
                },
                image: CoverImageSource::Local { file: FileVersion { path, size, modified_at_ns } },
            },
        })
    }
}

fn release_image(release_id: &str, source: bae_records::GallerySource) -> CoverImageSource {
    CoverImageSource::ReleaseImage { release_id: release_id.to_string(), source }
}

/// The covers in a JSON array of [`ItemJson`]s.
fn covers(json: &str, release_id: Option<&str>) -> Result<Vec<GalleryCover>, DbError> {
    let items: Vec<ItemJson> = serde_json::from_str(json)
        .map_err(|error| DbError::unreadable_json("a picker's covers", json, error))?;
    items.into_iter().map(|item| item.cover(release_id)).collect()
}

/// The row [`walk_sql`] ends a statement with, as SQLite holds it.
pub(super) struct GalleryColumns {
    fetched: Option<bool>,
    shows_remote: bool,
    shows_files: bool,
    saved: Option<String>,
    remote: String,
    remote_count: u32,
    files: String,
    files_count: u32,
    total: u32,
    focus: Option<String>,
}

impl GalleryColumns {
    pub(super) fn read(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            fetched: row.get(0)?,
            shows_remote: row.get(1)?,
            shows_files: row.get(2)?,
            saved: row.get(3)?,
            remote: row.get(4)?,
            remote_count: row.get(5)?,
            files: row.get(6)?,
            files_count: row.get(7)?,
            total: row.get(8)?,
            focus: row.get(9)?,
        })
    }

    /// The gallery these columns hold, for the picker of the library release
    /// `release_id`, or of a candidate (`None`).
    pub(super) fn gallery(self, release_id: Option<&str>) -> Result<CoverGallery, DbError> {
        let section = |shows: bool, json: &str, count: u32| {
            shows
                .then(|| Ok::<_, DbError>(CoverSection { items: covers(json, release_id)?, count }))
                .transpose()
        };
        let remote = section(self.shows_remote, &self.remote, self.remote_count)?;
        let files = section(self.shows_files, &self.files, self.files_count)?;
        let current = self
            .saved
            .map(|json| {
                serde_json::from_str::<ItemJson>(&json)
                    .map_err(|error| {
                        DbError::unreadable_json("a picker's saved cover", &json, error)
                    })?
                    .cover(release_id)
            })
            .transpose()?;
        let focus = self
            .focus
            .map(|json| {
                let focus: FocusJson = serde_json::from_str(&json).map_err(|error| {
                    DbError::unreadable_json("a picker's focused cover", &json, error)
                })?;
                Ok::<_, DbError>(GalleryFocus {
                    cover: focus.item.cover(release_id)?,
                    position: focus.position,
                    neighbors: focus
                        .neighbors
                        .map(|neighbors| {
                            Ok::<_, DbError>(Neighbors {
                                previous: CoverKey::of_json(neighbors.previous)?,
                                next: CoverKey::of_json(neighbors.next)?,
                            })
                        })
                        .transpose()?,
                })
            })
            .transpose()?;
        Ok(CoverGallery {
            current,
            fetch: CoverFetch::of_column(self.fetched),
            remote,
            files,
            total: self.total,
            focus,
        })
    }
}
