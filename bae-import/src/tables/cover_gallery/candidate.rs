//! A candidate's cover picker, read in one statement over the import's
//! tables: the covers its link's catalogs offered when this device last
//! fetched them for that link, or, before such a fetch, the covers offered by
//! the pressing that bae read the draft from; and the folder's images a cover
//! can be. Its tables are `candidate.sql`; fetching writes them, and a
//! refetch replaces them.

use super::*;
use bae_store::statement_optional;

// Each candidate's link as one JSON value, by the content hash it is the link
// of: its kind, catalog and release, its partners and its albums. A fetch of a
// candidate's covers stores it beside them, and the picker shows them while
// the candidate's link is still that value.
sqlive_compose::fragment!(
    CandidateCoverLink { cover_link: String },
    r#"
    WITH listed AS (
        SELECT link.*
        FROM @crate::tables::import_state::listed_fragments::ListedLink AS link
    )
    SELECT listed.link_hash AS linked_hash,
           json_array(listed.link_kind, listed.link_source, listed.link_release_id,
                      json(listed.link_partners), json(listed.link_albums)) AS cover_link
    FROM listed
"#
);

// [`CandidateCoverLink`] of the content `:content_hash`.
sqlive_compose::read!(
    writer CoverLinkOf(content_hash: String),
    r#"
    SELECT link.*
    FROM @crate::tables::cover_gallery::candidate::CandidateCoverLink AS link
    WHERE link.linked_hash = :content_hash
"#
);

// The gallery of the candidate `:key` under the filter `:filter`. The fetched
// covers show only while the candidate's link is the link this device fetched
// them for; before such a fetch, the covers its draft's pressing offered show,
// while its state is the one its scanned files hold. Folder images show under
// all or the release's files. The candidate's own cover is the one its draft
// chose.
sqlive_compose::read!(
    writer CandidateGallery(
        key: crate::CandidateKey,
        filter: &'static str,
        focus_kind: Option<&'static str>,
        focus_key: Option<String>
    ) { fetched: Option<bool> },
    r#"
    WITH scanned AS (
        SELECT scan.*
        FROM @crate::tables::folder_scans::row_fragments::ScanContent(key: :key) AS scan
        LIMIT 1
    ),
    linked AS (
        SELECT link.*
        FROM @crate::tables::cover_gallery::candidate::CandidateCoverLink AS link
    ),
    fetched_covers AS (
        SELECT fetch.content_hash, fetch.linked
        FROM candidate_cover_fetch AS fetch
        JOIN scanned ON scanned.content_hash = fetch.content_hash
        WHERE fetch.link IS (SELECT linked.cover_link FROM linked
                             WHERE linked.linked_hash = fetch.content_hash)
    ),
    own AS (
        SELECT cover.*
        FROM @crate::tables::import_state::listed_fragments::ListedCover AS cover
    ),
    chosen (kind, key) AS (
        SELECT CASE own.cover_kind WHEN 'local' THEN 'file' ELSE own.cover_kind END,
               COALESCE(own.cover_file_id, own.cover_url)
        FROM own
        JOIN scanned ON scanned.content_hash = own.cover_hash
    ),
    offers AS (
        SELECT offered.*
        FROM @crate::tables::import_state::row_fragments::OfferedCovers AS offered
    ),
    shown (rank, pos, kind, key, item) AS (
        SELECT 1, entry.position, 'remote', json_extract(entry.cover, '$.image.url'),
               json_object('kind', 'remote', 'cover', json(entry.cover))
        FROM candidate_cover_entry AS entry
        JOIN fetched_covers ON fetched_covers.content_hash = entry.content_hash
        WHERE :filter IN ('all', entry.source)
        UNION ALL
        SELECT 1, offers.position, 'remote', json_extract(offers.cover, '$.image.url'),
               json_object('kind', 'remote', 'cover', json(offers.cover))
        FROM offers
        JOIN scanned ON scanned.content_hash = offers.offered_hash
        WHERE NOT EXISTS (SELECT 1 FROM fetched_covers)
          AND EXISTS (SELECT 1
                      FROM @crate::tables::import_state::current_owner::CurrentOwner(
                          key: :key) AS owner)
          AND :filter IN ('all', offers.source)
        UNION ALL
        SELECT 2, file.position, 'file', file.relative_path,
               json_object('kind', 'folder_file', 'id', file.relative_path,
                           'path', hex(file.absolute_path), 'size', file.size,
                           'modified_at_ns', file.modified_at_ns)
        FROM @crate::tables::folder_scans::row_fragments::ShowableArtwork(key: :key) AS file
        WHERE :filter IN ('all', 'release_files')
    )
    SELECT (SELECT fetched_covers.linked FROM fetched_covers) AS fetched, walk.*
    FROM @crate::tables::cover_gallery::walk::PickerWalk(
        filter: :filter, focus_kind: :focus_kind, focus_key: :focus_key,
        items: (
            SELECT shown.rank, shown.pos, shown.kind, shown.key,
                   EXISTS (SELECT 1 FROM chosen
                           WHERE (chosen.kind, chosen.key) = (shown.kind, shown.key)),
                   shown.item
            FROM shown
        )) AS walk
"#
);

impl ImportTables {
    /// The link of the candidate `content_hash` as a fetch of its covers
    /// stores it beside them ([`CandidateCoverLink`]); `None` when it is
    /// linked to nothing.
    pub(crate) async fn candidate_cover_link(
        &self,
        content_hash: &str,
    ) -> Result<Option<String>, DbError> {
        let content_hash = content_hash.to_string();
        self.store
            .read(move |sql| {
                let row = statement_optional(&sql, &CoverLinkOfParams { content_hash })?;
                Ok(row.map(|row| row.link.cover_link))
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
