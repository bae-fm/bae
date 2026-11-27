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
//! surface reads the stored rows, and an import commits the stored records
//! once it has stored the releases it read in full
//! ([`ImportTables::store_import_reads`]).
//! Its tables are `claim_rows.sql`.

use super::release_link_rows::load_release_link_on;
use super::*;
use crate::ReleaseLink;
use bae_catalogs::records::{MetadataRef, ReleaseRecord};
use bae_catalogs::source_release::{claimed_records, pick_covers, SourceRelease};
use bae_store::statement_rows;
use bae_store::WriteStatements as _;

sqlive_compose::write!(
    DeleteClaimedRecords(content_hash: &'p str),
    r#"
    DELETE FROM import_candidate_claimed_record WHERE content_hash = :content_hash
"#
);

sqlive_compose::write!(
    DeleteOfferedCovers(content_hash: &'p str),
    r#"
    DELETE FROM import_candidate_offered_cover WHERE content_hash = :content_hash
"#
);

sqlive_compose::write!(
    InsertClaimedRecord(
        content_hash: &'p str,
        position: i64,
        catalog: &'p str,
        kind: &'static str,
        key: &'p str,
        album: Option<&'p str>,
    ),
    r#"
    INSERT INTO import_candidate_claimed_record (content_hash, position, catalog, kind, key, album)
    VALUES (:content_hash, :position, :catalog, :kind, :key, :album)
"#
);

sqlive_compose::write!(
    InsertOfferedCover(content_hash: &'p str, position: i64, source: &'p str, cover: String),
    r#"
    INSERT INTO import_candidate_offered_cover (content_hash, position, source, cover)
    VALUES (:content_hash, :position, :source, :cover)
"#
);

/// Work out what the link of the candidate `content_hash` claims, over the
/// claimed releases as this write has stored them, and store it in place of
/// what was stored. A candidate linked to nothing claims nothing.
pub(super) fn store_claims_on(sql: &CovenWrite<'_>, content_hash: &str) -> Result<(), DbError> {
    sql.run(&DeleteClaimedRecordsParams { content_hash })?;
    sql.run(&DeleteOfferedCoversParams { content_hash })?;
    let Some(listed) = load_release_link_on(sql, content_hash)? else {
        return Ok(());
    };
    let (link, _stored_records) = listed.into_stored();
    let (records, covers) = match link.link()? {
        ReleaseLink::Album(album) => (album.records(), Vec::new()),
        ReleaseLink::Pressing(pressing) => {
            let claimed: Vec<_> = pressing.claimed().collect();
            let releases = crate::tables::source_releases::load_source_releases_on(sql, &claimed)?
                .into_iter()
                .zip(claimed)
                .map(|(read, release)| {
                    read.ok_or_else(|| {
                        DbError::unreadable(
                            format!("the release {content_hash}'s link claims"),
                            release,
                        )
                    })
                })
                .collect::<Result<Vec<_>, DbError>>()?;
            let (primary, partners) =
                releases.split_first().expect("a pick claims at least its primary");
            let records: Vec<_> = releases.iter().map(SourceRelease::records).collect();
            (claimed_records(&records), pick_covers(primary, partners))
        }
    };
    for (position, record) in records.iter().enumerate() {
        let (kind, album) = match record {
            ReleaseRecord::Pressing { album_key, .. } => ("pressing", album_key.as_deref()),
            ReleaseRecord::Album { .. } => ("album", None),
        };
        sql.run(&InsertClaimedRecordParams {
            content_hash,
            position: to_i64(position, "a claimed record's position")?,
            catalog: record.catalog().as_str(),
            kind,
            key: record.key(),
            album,
        })?;
    }
    for (position, cover) in covers.iter().enumerate() {
        sql.run(&InsertOfferedCoverParams {
            content_hash,
            position: to_i64(position, "an offered cover's position")?,
            source: cover.source.as_str(),
            cover: serde_json::to_string(cover)
                .map_err(|error| DbError::encode("an offered cover", error))?,
        })?;
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

// The candidates whose link claims a release that has one of `albums` among
// its own albums.
sqlive_compose::read!(
    writer LinksClaimingAlbums(albums: String) { content_hash: String },
    r#"
    WITH changed (catalog, key) AS (
        SELECT changed_album.value ->> 0, changed_album.value ->> 1
        FROM json_each(:albums) AS changed_album),
    claimed (content_hash, catalog, release_id) AS (
        SELECT link.content_hash, link.source, link.release_id
        FROM import_candidate_release_link AS link
        WHERE link.kind = 'pressing'
        UNION ALL
        SELECT partner.content_hash, partner.source, partner.release_id
        FROM import_candidate_release_link_partner AS partner)
    SELECT DISTINCT claimed.content_hash AS content_hash
    FROM claimed
    JOIN @crate::tables::source_releases::row_fragments::OwnAlbums(
        listed: (SELECT 0 AS at, link.source AS catalog, link.release_id
                 FROM import_candidate_release_link AS link
                 WHERE link.kind = 'pressing'
                 UNION
                 SELECT 0 AS at, partner.source AS catalog, partner.release_id
                 FROM import_candidate_release_link_partner AS partner)) AS own
        ON own.catalog = claimed.catalog AND own.release_id = claimed.release_id
    JOIN changed
        ON changed.catalog = own.album_catalog AND changed.key = own.album_key
"#
);

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
    let request = LinksClaimingAlbumsParams { albums: albums.to_string() };
    let claiming = statement_rows(sql, &request)?;
    for row in claiming {
        store_claims_on(sql, &row.content_hash)?;
    }
    Ok(())
}

#[derive(serde::Deserialize)]
struct RecordColumns {
    catalog: String,
    kind: String,
    key: String,
    album: Option<String>,
}

/// The records [`super::listed_fragments::ListedLink`] gives as a link's
/// claimed records.
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

/// The stored records the link of the candidate `content_hash` claims; none
/// for a candidate with no link.
fn load_claimed_records_on(
    sql: &impl bae_store::WriterRead,
    content_hash: &str,
) -> Result<Vec<ReleaseRecord>, DbError> {
    match load_release_link_on(sql, content_hash)? {
        Some(listed) => claimed_records_of_json(&listed.into_stored().1),
        None => Ok(Vec::new()),
    }
}

impl ImportTables {
    /// Store `releases`, the releases the link of the candidate
    /// `content_hash` claims as an import read them in full, as every write
    /// naming a release stores it, and return the records the link claims
    /// once they are stored: a release this stores again stores what the
    /// link claims again from it. The import commits these records.
    pub(crate) async fn store_import_reads(
        &self,
        content_hash: &str,
        releases: Vec<SourceRelease>,
    ) -> Result<Vec<ReleaseRecord>, DbError> {
        let content_hash = content_hash.to_owned();
        self.store
            .call(move |sql| {
                crate::tables::source_releases::store_named_releases_on(sql, &releases)?;
                load_claimed_records_on(sql, &content_hash)
            })
            .await
    }
}

// `row_fragments::OfferedCovers` of the content `:content_hash`, in the order offered,
// which tests read.
#[cfg(test)]
sqlive_compose::read!(
    writer OfferedCoversOf(content_hash: String),
    r#"
    SELECT offered.*
    FROM @crate::tables::import_state::row_fragments::OfferedCovers AS offered
    WHERE offered.offered_hash = :content_hash
    ORDER BY offered.position
"#
);

#[cfg(test)]
#[path = "claim_rows_tests.rs"]
pub(crate) mod tests;
