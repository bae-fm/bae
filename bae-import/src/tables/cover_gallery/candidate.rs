//! A candidate's cover picker, read in one statement over the import's
//! tables: the covers its link's catalogs offered when this device last
//! fetched them for that link, or, before such a fetch, the covers the
//! pressing its draft was read from offered; and the folder's images a cover
//! can be. Its tables are `candidate.sql`; fetching writes them, and a
//! refetch replaces them.

use super::*;
use crate::folder_scanner::FileVersion;
use crate::tables::import_state::{current_at_scan_sql, SCANNED_HASH};
use crate::tables::source_releases::{offered_covers_of_json, offered_covers_sql};

/// The link of the candidate whose content hash the expression `hash` names,
/// as one JSON value: its kind, catalog and release, its partners and its
/// albums. NULL when the candidate is linked to nothing.
fn link_sql(hash: &str) -> String {
    format!(
        "(SELECT json_array(link.kind, link.source, link.release_id, \
             (SELECT json_group_array(json_array(p.source, p.release_id) ORDER BY p.source) \
              FROM import_candidate_release_link_partner AS p \
              WHERE p.content_hash = link.content_hash), \
             (SELECT json_group_array(json_array(a.catalog, a.album_id) ORDER BY a.catalog) \
              FROM import_candidate_release_link_album AS a \
              WHERE a.content_hash = link.content_hash)) \
          FROM import_candidate_release_link AS link WHERE link.content_hash = {hash})"
    )
}

/// The gallery of the candidate the statement binds as `:key`, under the
/// filter it binds as `:filter`: one row. The fetched covers show only while
/// the candidate's link is the one they were fetched for; before such a
/// fetch, the covers its draft's pressing offered show, while its state is
/// the one its scanned files hold. Folder images show under all or the
/// release's files.
fn candidate_gallery_sql() -> String {
    format!(
        "
WITH c AS (
    SELECT content_hash, watched_folder_path, path FROM scan_candidate WHERE path = :key LIMIT 1
),
fetch AS (
    SELECT f.content_hash, f.linked FROM candidate_cover_fetch AS f JOIN c USING (content_hash)
    WHERE f.link IS {link}
),
fetched AS (
    SELECT e.position, e.cover FROM candidate_cover_entry AS e JOIN fetch USING (content_hash)
    WHERE :filter IN ('all', e.source)
),
offered AS (
    SELECT o.key AS position, o.value AS cover
    FROM json_each(CASE WHEN EXISTS (SELECT 1 FROM fetch) THEN '[]'
                        WHEN EXISTS (SELECT 1 FROM import_candidate_state AS state WHERE {current})
                        THEN ({offered}) ELSE '[]' END) AS o
    WHERE :filter IN ('all', o.value ->> 'source')
),
files AS (
    SELECT f.position, f.relative_path, f.absolute_path, f.size, f.modified_at_ns
    FROM scan_candidate_file AS f
    JOIN c ON f.watched_folder_path = c.watched_folder_path AND f.candidate_path = c.path
    WHERE :filter IN ('all', 'release_files')
      AND f.role = 'artwork' AND bae_supported_cover(f.relative_path)
)
SELECT (SELECT linked FROM fetch),
       :filter <> 'release_files',
       (SELECT json_group_array(json(cover) ORDER BY position) FROM fetched),
       (SELECT json_group_array(json(cover) ORDER BY position) FROM offered),
       (SELECT COUNT(*) FROM fetched) + (SELECT COUNT(*) FROM offered),
       :filter IN ('all', 'release_files'),
       (SELECT COUNT(*) FROM files),
       (SELECT json_group_array(json_object('id', relative_path, 'path', hex(absolute_path),
                                            'size', size, 'modified_at_ns', modified_at_ns)
                                ORDER BY position)
        FROM files)",
        link = link_sql("c.content_hash"),
        current = current_at_scan_sql("state"),
        offered = offered_covers_sql(SCANNED_HASH),
    )
}

/// One folder image a cover can be, at the version the scan read.
#[derive(serde::Deserialize)]
struct FileJson {
    id: String,
    #[serde(deserialize_with = "path_column::from_hex")]
    path: std::path::PathBuf,
    size: u64,
    modified_at_ns: i64,
}

/// The covers the picker of the candidate at `key` offers under `filter`, as
/// one read.
pub(super) fn candidate_gallery_on(
    sql: &impl coven::SqlRead,
    key: &crate::CandidateKey,
    filter: CoverFilter,
) -> Result<CoverGallery, DbError> {
    let (
        fetched,
        shows_remote,
        fetched_json,
        offered_json,
        remote_count,
        shows_files,
        files_count,
        files_json,
    ) = sql.query_row(
        &candidate_gallery_sql(),
        named_params! { ":key": key, ":filter": filter.column() },
        |row| {
            Ok((
                row.get::<_, Option<bool>>(0)?,
                row.get::<_, bool>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, u32>(4)?,
                row.get::<_, bool>(5)?,
                row.get::<_, u32>(6)?,
                row.get::<_, String>(7)?,
            ))
        },
    )?;
    let mut remote: Vec<RemoteCover> = serde_json::from_str(&fetched_json).map_err(|error| {
        DbError::unreadable_json("a candidate's fetched cover", &fetched_json, error)
    })?;
    remote.extend(offered_covers_of_json(&offered_json)?);
    let files: Vec<FileJson> = serde_json::from_str(&files_json).map_err(|error| {
        DbError::unreadable_json("a candidate's cover files", &files_json, error)
    })?;
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
                    image: CoverImageSource::Local {
                        file: FileVersion {
                            path: file.path,
                            size: file.size,
                            modified_at_ns: file.modified_at_ns,
                        },
                    },
                    label: GalleryLabel::File(file.id),
                })
                .collect(),
            count: files_count,
        });
    }
    Ok(CoverGallery {
        current: None,
        fetch: CoverFetch::of_column(fetched),
        sections,
        total: remote_count + files_count,
    })
}

impl ImportTables {
    /// The link of the candidate `content_hash` as a fetch of its covers
    /// stores it beside them; `None` when it is linked to nothing.
    pub(crate) async fn candidate_cover_link(
        &self,
        content_hash: &str,
    ) -> Result<Option<String>, DbError> {
        let content_hash = content_hash.to_string();
        self.store
            .read(move |sql| {
                Ok(sql.query_row(&format!("SELECT {}", link_sql("?")), [&content_hash], |row| {
                    row.get(0)
                })?)
            })
            .await
    }

    /// Store what the catalogs offered the picker of the candidate
    /// `content_hash`, fetched for `link` (see [`Self::candidate_cover_link`]),
    /// in the order offered, in place of what the last fetch stored; `None`
    /// when the candidate is linked to nothing.
    pub(crate) async fn replace_candidate_covers(
        &self,
        content_hash: &str,
        link: Option<String>,
        covers: Option<Vec<RemoteCover>>,
    ) -> Result<(), DbError> {
        let content_hash = content_hash.to_string();
        self.store
            .write(move |sql| {
                sql.execute(
                    "DELETE FROM candidate_cover_fetch WHERE content_hash = ?",
                    [&content_hash],
                )?;
                sql.execute(
                    "INSERT INTO candidate_cover_fetch (content_hash, link, linked) VALUES (?, ?, ?)",
                    params![content_hash, link, covers.is_some()],
                )?;
                for (position, cover) in covers.iter().flatten().enumerate() {
                    sql.execute(
                        "INSERT INTO candidate_cover_entry (content_hash, position, source, cover) \
                         VALUES (?, ?, ?, ?)",
                        params![
                            content_hash,
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
