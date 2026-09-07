//! Provider documents and their references are stored atomically by entity key.
//! Reads retrieve owned bytes using those references without decoding metadata.

use super::query::{QueryOne, QueryRows};
use super::*;
use crate::import::payloads::ReleasePayloads;
use crate::import::{ImportError, MetadataRef, MetadataSource, PayloadSource, SourcePayload};

/// Read exactly the supported traversal in one database transaction: a
/// MusicBrainz anchor's group and Discogs release, then that release's master;
/// a Discogs anchor's master and MusicBrainz cross-reference. Optional targets
/// become visible when fetched, without rewriting the referring document.
pub(super) fn load_release_payloads_on(
    sql: &(impl QueryOne + QueryRows),
    release: &MetadataRef,
) -> Result<Option<ReleasePayloads>, ImportError> {
    let read = || -> Result<Option<ReleasePayloads>, DbError> {
        let source = PayloadSource::release_of(release.source);
        let anchor: Option<(String, Option<String>, String)> = sql.query_row(
            "SELECT json, source_group_id, document_release_id FROM source_release_payloads WHERE source = ? AND source_release_id = ?",
            params![source.as_str(), release.id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ).optional()?;
        let Some((anchor, group_id, document_id)) = anchor else {
            return Ok(None);
        };
        let mut supporting = sql.query(
            "SELECT payload.source, payload.source_release_id, payload.json \
             FROM source_document_reference AS reference \
             JOIN source_release_payloads AS payload \
               ON payload.source = reference.target_source AND payload.source_release_id = reference.target_id \
             WHERE reference.source = ? AND reference.source_release_id = ? \
             ORDER BY payload.source, payload.source_release_id",
            params![source.as_str(), release.id],
            |row| {
                let source: String = row.get(0)?;
                let source = source.parse::<PayloadSource>().map_err(|error| {
                    coven::rusqlite::Error::FromSqlConversionFailure(0, coven::rusqlite::types::Type::Text, error.into())
                })?;
                Ok(SourcePayload::new(source, row.get::<_, String>(1)?, row.get(2)?))
            },
        )?;
        if release.source == MetadataSource::MusicBrainz {
            if let Some(discogs) = supporting
                .iter()
                .find(|payload| payload.source == PayloadSource::Discogs)
            {
                let master: Option<(String, String)> = sql.query_row(
                    "SELECT payload.source_release_id, payload.json FROM source_document_reference AS reference \
                     JOIN source_release_payloads AS payload \
                       ON payload.source = reference.target_source AND payload.source_release_id = reference.target_id \
                     WHERE reference.source = 'discogs' AND reference.source_release_id = ? \
                       AND reference.target_source = 'discogs_master'",
                    params![discogs.source_release_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                ).optional()?;
                if let Some((id, json)) = master {
                    supporting.push(SourcePayload::new(PayloadSource::DiscogsMaster, id, json));
                }
            }
        }
        Ok(Some(ReleasePayloads::from_stored(
            release.clone(),
            anchor,
            supporting,
            group_id,
            document_id,
        )))
    };
    read().map_err(|error| ImportError::Db(crate::library::LibraryError::Database(error)))
}

impl Database {
    /// Fetch owned provider documents in one read. Parsing belongs to the
    /// consuming operation, after the connection has been released.
    pub(crate) async fn load_release_payloads(
        &self,
        release: &MetadataRef,
    ) -> Result<Option<ReleasePayloads>, ImportError> {
        let release = release.clone();
        self.read(move |sql| Ok(load_release_payloads_on(&sql, &release)))
            .await
            .map_err(|error| ImportError::Db(crate::library::LibraryError::Database(error)))?
    }

    /// Decode relationship facts before dispatching the write. Payload bytes,
    /// group identity and replacement references commit as one transaction.
    pub async fn save_source_release_payloads(
        &self,
        payloads: &[DbSourceReleasePayload],
    ) -> Result<(), DbError> {
        let payloads = payloads
            .iter()
            .map(|payload| {
                crate::provider_document::decode(
                    payload.source.as_str(),
                    &payload.source_release_id,
                    &payload.json,
                )
                .map(|facts| (payload.clone(), facts))
                .map_err(DbError::Message)
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.call(move |sql| {
            for (payload, facts) in payloads {
                let (document_id, group_id) = match facts.release {
                    Some(release) => (Some(release.id), release.group_id),
                    None => (None, None),
                };
                sql.execute(
                    "INSERT INTO source_release_payloads \
                         (source, source_release_id, json, fetched_at, source_group_id, document_release_id) VALUES (?, ?, ?, ?, ?, ?) \
                     ON CONFLICT(source, source_release_id) DO UPDATE SET \
                         json = excluded.json, fetched_at = excluded.fetched_at, source_group_id = excluded.source_group_id, document_release_id = excluded.document_release_id",
                    params![payload.source.as_str(), payload.source_release_id, payload.json, payload.fetched_at.to_rfc3339(), group_id, document_id],
                )?;
                sql.execute(
                    "DELETE FROM source_document_reference WHERE source = ? AND source_release_id = ?",
                    params![payload.source.as_str(), payload.source_release_id],
                )?;
                for (target_source, target_id) in facts.references {
                    sql.execute(
                        "INSERT INTO source_document_reference (source, source_release_id, target_source, target_id) VALUES (?, ?, ?, ?)",
                        params![payload.source.as_str(), payload.source_release_id, target_source, target_id],
                    )?;
                }
            }
            Ok(())
        }).await
    }

    /// The documents stored under each key; absent rows are absent from the map.
    pub async fn load_source_release_payloads(
        &self,
        keys: &[(crate::import::PayloadSource, String)],
    ) -> Result<HashMap<(crate::import::PayloadSource, String), String>, DbError> {
        let keys = keys.to_vec();
        self.read(move |sql| {
            let mut found = HashMap::with_capacity(keys.len());
            for (source, source_release_id) in keys {
                let json: Option<String> = sql.query_row(
                    "SELECT json FROM source_release_payloads WHERE source = ? AND source_release_id = ?",
                    params![source.as_str(), source_release_id],
                    |row| row.get(0),
                ).optional()?;
                if let Some(json) = json { found.insert((source, source_release_id), json); }
            }
            Ok(found)
        }).await
    }
}
