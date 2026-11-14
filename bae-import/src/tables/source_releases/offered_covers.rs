//! The covers a candidate's picker offers, read in one statement.
//!
//! While the draft was read from the pressing the candidate is linked to, the
//! picker offers every claimed release's own images first (in claim order,
//! then position), then the albums' images, each image once however many
//! releases offer it, and only the images a catalog stated when any is stated.
//! A MusicBrainz release group a kept statement joins to a claimed release
//! offers its front image after that release's own album images.
//!
//! [`bae_catalogs::source_release::pick_covers`] states the same rule over
//! releases a write has fetched and not stored yet, and a test runs both over
//! the same stored releases.

use super::claimed_records::claimed_tables;
use super::*;

/// The offered covers as a JSON array of `{url, label, source, standing,
/// copies}` objects in the order the picker lists them, for the candidate
/// bound to `:hash`; `[]` unless its draft was read from its linked pressing.
pub(crate) fn offered_covers_sql() -> String {
    let copies = super::super::downscaled_copies::copies_json_sql(
        "source_release_cover_copy",
        "copy.catalog = s.catalog AND copy.release_id = s.release_id \
         AND copy.scope = s.scope AND copy.position = s.position",
    );
    format!(
        "WITH {claimed}
-- Every image a claimed release offers, and the front image of each release
-- group joined to one.
images (claim_pos, scope, joined, position, url, label, source, standing, copies) AS (
    SELECT c.claim_pos, s.scope, 0, s.position, s.url, json(s.label), s.source, s.standing,
           json({copies})
    FROM claimed AS c JOIN source_release_cover AS s USING (catalog, release_id)
    UNION ALL
    SELECT claim_pos, 'album', 1, 0, cover ->> 'url', cover -> 'label', cover ->> 'source',
           cover ->> 'standing', cover -> 'copies'
    FROM (SELECT claim_pos, bae_release_group_cover(key) AS cover
          FROM joined WHERE record_catalog = 'musicbrainz')
),
offered AS (
    SELECT *,
           ROW_NUMBER() OVER (PARTITION BY url
                              ORDER BY scope = 'album', claim_pos, joined, position) AS nth,
           MAX(standing = 'stated') OVER (PARTITION BY url) AS stated
    FROM images
),
firsts AS (SELECT * FROM offered WHERE nth = 1)
SELECT json_group_array(json_object(
           'url', url, 'label', json(label), 'source', source,
           'standing', CASE WHEN stated THEN 'stated' ELSE 'unstated' END,
           'copies', json(copies))
       ORDER BY scope = 'album', claim_pos, joined, position)
FROM firsts
WHERE (stated OR NOT EXISTS (SELECT 1 FROM firsts WHERE stated))
  AND EXISTS (SELECT 1
              FROM import_candidate_draft_provenance AS pr
              JOIN claimed AS c ON c.claim_pos = 0
              WHERE pr.content_hash = :hash AND pr.kind = 'external_release'
                AND (pr.source, pr.release_id) = (c.catalog, c.release_id))",
        claimed = claimed_tables!(),
    )
}

#[derive(serde::Deserialize)]
struct CoverColumns {
    url: String,
    label: bae_catalogs::remote_image::CoverLabel,
    source: String,
    standing: String,
    copies: Vec<bae_catalogs::remote_image::DownscaledCopy>,
}

/// The covers the picker offers the candidate `content_hash`.
pub(crate) fn offered_covers_on<S: coven::SqlRead>(
    sql: &S,
    content_hash: &str,
) -> Result<Vec<RemoteCover>, DbError> {
    let json: String = sql.query_row(
        &offered_covers_sql(),
        rusqlite::named_params! { ":hash": content_hash },
        |row| row.get(0),
    )?;
    let rows: Vec<CoverColumns> = serde_json::from_str(&json)
        .map_err(|error| DbError::unreadable("the offered covers statement", (&json, error)))?;
    rows.into_iter()
        .map(|row| {
            Ok(RemoteCover {
                image: RemoteImageSet::with_copies(row.url, row.copies),
                label: row.label,
                source: catalog_column(&row.source)?,
                standing: CoverStanding::from_column(&row.standing)
                    .ok_or_else(|| DbError::unreadable("a cover's standing", &row.standing))?,
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "offered_covers_tests.rs"]
mod tests;
