//! The records the releases a candidate's link claims describe together,
//! merged by one statement over the stored releases.
//!
//! A pressing link claims its primary release and at most one partner per
//! other catalog. Each claimed release names its own record, what its rows say
//! about other catalogs, and the albums a kept statement joins to its own
//! albums ([`super::super::album_link_rows`]). Per catalog, a claimed
//! release's own record wins, else the first claimed release's says. The
//! applied catalog, the primary's, comes first, then catalog rank. An album
//! link claims no release: its records are its albums, in catalog rank.
//!
//! [`bae_catalogs::source_release::claimed_records`] states the same rule over
//! releases a write has fetched and not stored yet, and a test runs both over
//! the same stored releases.

use super::*;

/// The merged records as a JSON array of `{catalog, kind, key, album}`
/// objects, in the order surfaces list them, for the candidate bound to
/// `:hash`. Usable as a column of a larger statement.
pub(crate) const CLAIMED_RECORDS_SQL: &str = "
WITH claimed (claim_pos, catalog, release_id) AS (
    SELECT 0, l.source, l.release_id
    FROM import_candidate_release_link AS l
    WHERE l.content_hash = :hash AND l.kind = 'pressing'
    UNION ALL
    SELECT ROW_NUMBER() OVER (ORDER BY p.source), p.source, p.release_id
    FROM import_candidate_release_link_partner AS p
    WHERE p.content_hash = :hash
),
-- What each claimed release says about other catalogs, beside its own record.
stated (claim_pos, catalog, release_id, record_catalog, kind, key, album_key) AS (
    SELECT c.claim_pos, c.catalog, c.release_id, c.catalog, 'pressing', c.release_id,
           r.source_group_id
    FROM claimed AS c JOIN source_release AS r USING (catalog, release_id)
    UNION ALL
    SELECT c.claim_pos, c.catalog, c.release_id, o.record_catalog, o.kind, o.key, o.album_key
    FROM claimed AS c JOIN source_release_record AS o USING (catalog, release_id)
),
-- The albums of each release: its group first, then the albums its records name.
albums (claim_pos, catalog, release_id, pos, album_catalog, album_key) AS (
    SELECT claim_pos, catalog, release_id, 0, catalog, album_key
    FROM stated
    WHERE record_catalog = catalog AND album_key IS NOT NULL
    UNION ALL
    SELECT claim_pos, catalog, release_id,
           ROW_NUMBER() OVER (PARTITION BY claim_pos ORDER BY record_catalog),
           record_catalog, CASE kind WHEN 'album' THEN key ELSE album_key END
    FROM stated
    WHERE record_catalog <> catalog AND (kind = 'album' OR album_key IS NOT NULL)
),
-- The album each kept statement says one of those is, on a catalog the
-- release's own records do not reach; one per catalog, the first.
joined_candidates (claim_pos, catalog, joined_catalog, joined_key, pos, k_catalog, k_key, k_group) AS (
    SELECT a.claim_pos, a.catalog, k.catalog, k.key, a.pos, k.catalog, k.key, k.release_group
    FROM albums AS a
    JOIN release_group_album_link AS k
        ON a.album_catalog = 'musicbrainz' AND k.release_group = a.album_key
    UNION ALL
    SELECT a.claim_pos, a.catalog, 'musicbrainz', k.release_group, a.pos, k.catalog, k.key,
           k.release_group
    FROM albums AS a
    JOIN release_group_album_link AS k
        ON a.album_catalog <> 'musicbrainz'
       AND k.catalog = a.album_catalog AND k.key = a.album_key
),
joined (claim_pos, record_catalog, key) AS (
    SELECT claim_pos, joined_catalog, joined_key
    FROM (SELECT j.*,
                 ROW_NUMBER() OVER (PARTITION BY j.claim_pos, j.joined_catalog
                                    ORDER BY j.pos, j.k_catalog, j.k_key, j.k_group) AS nth
          FROM joined_candidates AS j
          WHERE j.joined_catalog <> j.catalog
            AND NOT EXISTS (SELECT 1 FROM stated AS s
                            WHERE s.claim_pos = j.claim_pos
                              AND s.record_catalog = j.joined_catalog))
    WHERE nth = 1
),
records (claim_pos, own, record_catalog, kind, key, album_key) AS (
    SELECT claim_pos, record_catalog = catalog, record_catalog, kind, key, album_key
    FROM stated
    UNION ALL
    SELECT claim_pos, 0, record_catalog, 'album', key, NULL FROM joined
    UNION ALL
    SELECT 0, 1, a.catalog, 'album', a.album_id, NULL
    FROM import_candidate_release_link_album AS a
    WHERE a.content_hash = :hash
),
ranked AS (
    SELECT *,
           ROW_NUMBER() OVER (PARTITION BY record_catalog
                              ORDER BY own DESC,
                                       CASE WHEN own THEN -claim_pos ELSE claim_pos END) AS nth
    FROM records
)
SELECT json_group_array(json_object('catalog', record_catalog, 'kind', kind,
                                    'key', key, 'album', album_key)
       ORDER BY record_catalog IS NOT (SELECT catalog FROM claimed WHERE claim_pos = 0),
                bae_catalog_rank(record_catalog))
FROM ranked WHERE nth = 1";

#[derive(serde::Deserialize)]
struct RecordColumns {
    catalog: String,
    kind: String,
    key: String,
    album: Option<String>,
}

/// The records of the candidate `content_hash`'s link, merged; none for a
/// candidate linked to nothing.
pub(crate) fn claimed_records_on<S: coven::SqlRead>(
    sql: &S,
    content_hash: &str,
) -> Result<Vec<ReleaseRecord>, DbError> {
    let json: String = sql.query_row(
        CLAIMED_RECORDS_SQL,
        rusqlite::named_params! { ":hash": content_hash },
        |row| row.get(0),
    )?;
    let rows: Vec<RecordColumns> = serde_json::from_str(&json)
        .map_err(|error| DbError::unreadable("the claimed records statement", (&json, error)))?;
    rows.into_iter()
        .map(|row| {
            let reference = MetadataRef::new(catalog_column(&row.catalog)?, row.key);
            match row.kind.as_str() {
                "pressing" => Ok(ReleaseRecord::new(&reference, row.album)),
                "album" => Ok(ReleaseRecord::album(&reference)),
                other => Err(DbError::unreadable("a claimed record's kind", other)),
            }
        })
        .collect()
}

#[cfg(test)]
#[path = "claimed_records_tests.rs"]
mod tests;
