//! A candidate's cover picker, read in one statement over the import's
//! tables: the covers its link's catalogs offered when this device last
//! fetched them for that link, or, before such a fetch, the covers offered by
//! the pressing that bae read the draft from; and the folder's images a cover
//! can be. Its tables are `candidate.sql`; fetching writes them, and a
//! refetch replaces them.

use super::*;
use crate::tables::import_state::{current_at_scan_sql, SCANNED_HASH};
use crate::tables::source_releases::offered_covers_sql;

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
/// filter it binds as `:filter`. The fetched covers show only while the
/// candidate's link is the link this device fetched them for; before such a
/// fetch, the covers its draft's pressing offered show, while its state is
/// the one its scanned files hold. Folder images show under all or the release's
/// files. The candidate's own cover is the one its draft chose.
pub(super) fn candidate_gallery_sql() -> String {
    format!(
        "
WITH c AS (
    SELECT content_hash, watched_folder_path, path FROM scan_candidate WHERE path = :key LIMIT 1
),
fetch AS (
    SELECT f.content_hash, f.linked FROM candidate_cover_fetch AS f JOIN c USING (content_hash)
    WHERE f.link IS {link}
),
chosen (kind, key) AS (
    SELECT CASE cover.kind WHEN 'local' THEN 'file' ELSE cover.kind END,
           COALESCE(cover.file_id, cover.url)
    FROM import_candidate_cover AS cover JOIN c USING (content_hash)
),
listed (rank, pos, kind, key, item) AS (
    SELECT 1, e.position, 'remote', json_extract(e.cover, '$.image.url'),
           json_object('kind', 'remote', 'cover', json(e.cover))
    FROM candidate_cover_entry AS e JOIN fetch USING (content_hash)
    WHERE :filter IN ('all', e.source)
    UNION ALL
    SELECT 1, o.key, 'remote', o.value ->> 'url',
           json_object('kind', 'offered', 'cover', json(o.value))
    FROM json_each(CASE WHEN EXISTS (SELECT 1 FROM fetch) THEN '[]'
                        WHEN EXISTS (SELECT 1 FROM import_candidate_state AS state WHERE {current})
                        THEN ({offered}) ELSE '[]' END) AS o
    WHERE :filter IN ('all', o.value ->> 'source')
    UNION ALL
    SELECT 2, f.position, 'file', f.relative_path,
           json_object('kind', 'folder_file', 'id', f.relative_path, 'path', hex(f.absolute_path),
                       'size', f.size, 'modified_at_ns', f.modified_at_ns)
    FROM scan_candidate_file AS f
    JOIN c ON f.watched_folder_path = c.watched_folder_path AND f.candidate_path = c.path
    WHERE :filter IN ('all', 'release_files')
      AND f.role = 'artwork' AND bae_supported_cover(f.relative_path)
),
items (rank, pos, kind, key, preferred, item) AS (
    SELECT rank, pos, kind, key, EXISTS (SELECT 1 FROM chosen WHERE (chosen.kind, chosen.key) = (listed.kind, listed.key)), item
    FROM listed
){walk}",
        link = link_sql("c.content_hash"),
        current = current_at_scan_sql("state"),
        offered = offered_covers_sql(SCANNED_HASH),
        walk = super::walk::walk_sql("(SELECT linked FROM fetch)"),
    )
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
