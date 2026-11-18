//! What a candidate's link claims, stored with the link: the records the
//! claimed releases describe it by, and the covers they offer its picker.
//!
//! The rules that merge them are bae-catalogs' (`claimed_records`,
//! `pick_covers`), run over the claimed releases as they are stored, so they
//! hold the albums kept for those releases' albums too. The write that stores
//! the link stores what it claims, and so does each write that changes what
//! that comes from: one that stores a claimed release again
//! ([`store_claims_of_releases_on`]), and one that changes the albums kept
//! for a claimed release's album ([`store_claims_of_albums_on`]). Every
//! surface reads the stored rows.
//! Its tables are `claim_rows.sql`.

pub(super) const SQL: &str = include_str!("claim_rows.sql");

use super::release_link_rows::{read_release_link, RELEASE_LINK_COLUMNS};
use super::*;
use crate::ReleaseLink;
use bae_catalogs::records::{MetadataRef, ReleaseRecord};
use bae_catalogs::source_release::{claimed_records, pick_covers, SourceRelease};

/// Work out what the link of the candidate `content_hash` claims, over the
/// claimed releases as this write has stored them, and store it in place of
/// what was stored. A candidate linked to nothing claims nothing.
pub(super) fn store_claims_on(sql: &CovenWrite<'_>, content_hash: &str) -> Result<(), DbError> {
    sql.execute(
        "DELETE FROM import_candidate_claimed_record WHERE content_hash = ?",
        [content_hash],
    )?;
    sql.execute(
        "DELETE FROM import_candidate_offered_cover WHERE content_hash = ?",
        [content_hash],
    )?;
    let stored = sql.query(
        &format!(
            "SELECT {RELEASE_LINK_COLUMNS} FROM import_candidate_release_link AS link \
             WHERE link.content_hash = ?"
        ),
        [content_hash],
        read_release_link,
    )?;
    let Some(link) = stored.into_iter().flatten().next() else {
        return Ok(());
    };
    let (records, covers) = match link.link()? {
        ReleaseLink::Album(album) => (album.records(), Vec::new()),
        ReleaseLink::Pressing(pressing) => {
            let load = |release: &bae_catalogs::records::LookupRef| {
                crate::tables::source_releases::load_source_release_on(sql, release)?.ok_or_else(
                    || {
                        DbError::unreadable(
                            format!("the release {content_hash}'s link claims"),
                            release,
                        )
                    },
                )
            };
            let primary = load(&pressing.record)?;
            let partners =
                pressing.partners.iter().map(load).collect::<Result<Vec<_>, DbError>>()?;
            let claimed: Vec<_> =
                std::iter::once(&primary).chain(&partners).map(SourceRelease::records).collect();
            (claimed_records(&claimed), pick_covers(&primary, &partners))
        }
    };
    for (position, record) in records.iter().enumerate() {
        let (kind, album) = match record {
            ReleaseRecord::Pressing { album_key, .. } => ("pressing", album_key.as_deref()),
            ReleaseRecord::Album { .. } => ("album", None),
        };
        sql.execute(
            "INSERT INTO import_candidate_claimed_record \
                 (content_hash, position, catalog, kind, key, album) \
             VALUES (?, ?, ?, ?, ?, ?)",
            params![
                content_hash,
                to_i64(position, "a claimed record's position")?,
                record.catalog().as_str(),
                kind,
                record.key(),
                album
            ],
        )?;
    }
    for (position, cover) in covers.iter().enumerate() {
        sql.execute(
            "INSERT INTO import_candidate_offered_cover (content_hash, position, source, cover) \
             VALUES (?, ?, ?, ?)",
            params![
                content_hash,
                to_i64(position, "an offered cover's position")?,
                cover.source.as_str(),
                serde_json::to_string(cover)
                    .map_err(|error| DbError::encode("an offered cover", error))?,
            ],
        )?;
    }
    Ok(())
}

/// Store again what every link claiming one of `releases` claims, inside the
/// write that stored those releases again.
pub(crate) fn store_claims_of_releases_on(
    sql: &CovenWrite<'_>,
    releases: &[&bae_catalogs::records::LookupRef],
) -> Result<(), DbError> {
    if releases.is_empty() {
        return Ok(());
    }
    let releases = serde_json::Value::Array(
        releases
            .iter()
            .map(|release| serde_json::json!([release.catalog.as_str(), release.key]))
            .collect(),
    );
    let claiming = sql.query(
        "WITH stored (catalog, release_id) AS ( \
             SELECT value ->> 0, value ->> 1 FROM json_each(?)) \
         SELECT link.content_hash FROM import_candidate_release_link AS link \
         JOIN stored ON (stored.catalog, stored.release_id) = (link.source, link.release_id) \
         UNION \
         SELECT partner.content_hash FROM import_candidate_release_link_partner AS partner \
         JOIN stored \
             ON (stored.catalog, stored.release_id) = (partner.source, partner.release_id)",
        [releases.to_string()],
        |row| row.get::<_, String>(0),
    )?;
    for content_hash in claiming {
        store_claims_on(sql, &content_hash)?;
    }
    Ok(())
}

/// Store again what every link claims whose claimed release has one of
/// `albums` among its own albums (its group, or an album one of its records
/// names), inside the write that changed the albums kept for them.
pub(crate) fn store_claims_of_albums_on(
    sql: &CovenWrite<'_>,
    albums: &[MetadataRef],
) -> Result<(), DbError> {
    if albums.is_empty() {
        return Ok(());
    }
    let albums = serde_json::Value::Array(
        albums.iter().map(|album| serde_json::json!([album.catalog.as_str(), album.key])).collect(),
    );
    let claiming = sql.query(
        "WITH changed (catalog, key) AS ( \
             SELECT value ->> 0, value ->> 1 FROM json_each(?)), \
         claimed (content_hash, catalog, release_id) AS ( \
             SELECT content_hash, source, release_id FROM import_candidate_release_link \
             WHERE kind = 'pressing' \
             UNION ALL \
             SELECT content_hash, source, release_id FROM import_candidate_release_link_partner), \
         albums (catalog, release_id, album_catalog, album_key) AS ( \
             SELECT catalog, release_id, catalog, source_group_id FROM source_release \
             WHERE source_group_id IS NOT NULL \
             UNION ALL \
             SELECT catalog, release_id, record_catalog, \
                    CASE kind WHEN 'album' THEN key ELSE album_key END \
             FROM source_release_record) \
         SELECT DISTINCT claimed.content_hash FROM claimed \
         JOIN albums USING (catalog, release_id) \
         JOIN changed \
             ON (changed.catalog, changed.key) = (albums.album_catalog, albums.album_key)",
        [albums.to_string()],
        |row| row.get::<_, String>(0),
    )?;
    for content_hash in claiming {
        store_claims_on(sql, &content_hash)?;
    }
    Ok(())
}

/// The stored records the link of the candidate whose content hash the
/// expression `hash` names claims, as a JSON array of `{catalog, kind, key,
/// album}` objects in the order surfaces list them. A larger statement can
/// use this SQL as one of its columns.
pub(crate) fn claimed_records_sql(hash: &str) -> String {
    format!(
        "SELECT json_group_array(json_object('catalog', claimed.catalog, 'kind', claimed.kind, \
                                             'key', claimed.key, 'album', claimed.album) \
                                 ORDER BY claimed.position) \
         FROM import_candidate_claimed_record AS claimed WHERE claimed.content_hash = {hash}"
    )
}

#[derive(serde::Deserialize)]
struct RecordColumns {
    catalog: String,
    kind: String,
    key: String,
    album: Option<String>,
}

/// The records a [`claimed_records_sql`] column holds.
pub(crate) fn claimed_records_of_json(json: &str) -> Result<Vec<ReleaseRecord>, DbError> {
    let rows: Vec<RecordColumns> = serde_json::from_str(json)
        .map_err(|error| DbError::unreadable_json("the claimed records", json, error))?;
    rows.into_iter()
        .map(|row| {
            let reference = MetadataRef::new(super::match_rows::catalog_of(&row.catalog)?, row.key);
            match row.kind.as_str() {
                "pressing" => Ok(ReleaseRecord::new(&reference, row.album)),
                "album" => Ok(ReleaseRecord::album(&reference)),
                other => Err(DbError::unreadable("a claimed record's kind", other)),
            }
        })
        .collect()
}

/// The SQL of the rows the picker of each candidate the table `candidates`
/// holds (by its `content_hash`) lists as the covers its linked pressing
/// offers: `(position, source, cover)`, the cover as `RemoteCover` JSON. It
/// lists them only while the candidate's draft is the one bae read from that
/// pressing.
pub(crate) fn offered_covers_sql(candidates: &str) -> String {
    format!(
        "SELECT offered.position, offered.source, offered.cover \
         FROM import_candidate_offered_cover AS offered JOIN {candidates} USING (content_hash) \
         WHERE EXISTS (SELECT 1 FROM import_candidate_draft_provenance AS provenance \
                       JOIN import_candidate_release_link AS link \
                           ON link.content_hash = provenance.content_hash \
                          AND link.kind = 'pressing' \
                          AND (link.source, link.release_id) \
                              = (provenance.source, provenance.release_id) \
                       WHERE provenance.content_hash = offered.content_hash \
                         AND provenance.kind = 'external_release')"
    )
}

#[cfg(test)]
#[path = "claim_rows_tests.rs"]
pub(crate) mod tests;
