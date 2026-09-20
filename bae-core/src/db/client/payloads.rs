//! The `source_release_payloads` store: provider documents keyed by the source
//! entity they describe.
//!
//! One table serves both halves of a release's life. Identification writes its
//! rows before it stores the verdict that names the release, so a candidate the
//! user opens replays what identification fetched with no network; the release
//! that candidate becomes reads the same rows back through its external
//! metadata provenance.

use super::query::QueryOne;
use super::*;
use crate::import::payloads::ArchivedDocuments;
use crate::import::{ImportError, MetadataRef, PayloadSource};

/// One document by its key, on whichever connection the caller holds. The
/// import module drives the rounds; this answers each key.
pub(super) struct StoredDocuments<'a, S>(pub(super) &'a S);

impl<S: QueryOne> ArchivedDocuments for StoredDocuments<'_, S> {
    fn document(&self, source: PayloadSource, id: &str) -> Result<Option<String>, ImportError> {
        self.0
            .query_row(
                "SELECT json FROM source_release_payloads \
                 WHERE source = ? AND source_release_id = ?",
                params![source.as_str(), id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| {
                ImportError::Db(crate::library::LibraryError::Database(DbError::from(error)))
            })
    }
}

/// The archived set for one release, read by id inside `sql`'s read.
pub(super) fn load_release_payloads_on(
    sql: &impl QueryOne,
    release: &MetadataRef,
) -> Result<Option<crate::import::payloads::ReleasePayloads>, ImportError> {
    crate::import::payloads::load_on(&StoredDocuments(sql), release)
}

impl Database {
    /// Freeze the interpretation that pending drafts used before source
    /// documents belonged to each metadata application.
    pub(crate) fn migrate_applied_sources(
        sql: &coven::MigrationContext<'_>,
    ) -> Result<(), DbError> {
        let rows = sql.query(
            "SELECT content_hash, source, release_id FROM import_candidate_draft_provenance WHERE kind = 'external_release'",
            [], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?)),
        )?;
        for (hash, catalog, key) in rows {
            let catalog = catalog.parse().map_err(DbError::Message)?;
            let record = MetadataRef::new(catalog, key);
            let Some(payloads) = load_release_payloads_on(sql, &record)
                .map_err(|error| DbError::Message(error.to_string()))?
            else {
                // Such a draft already lacked the documents required to import.
                tracing::warn!(
                    content_hash = hash,
                    "pending draft has no archived source document to preserve"
                );
                continue;
            };
            let scan: Option<(String, String)> = sql.query_row(
                "SELECT watched_folder_path, path FROM scan_candidate WHERE content_hash = ? ORDER BY watched_folder_path, path LIMIT 1",
                [&hash], |row| Ok((row.get(0)?, row.get(1)?)),
            ).optional()?;
            let audio_durations_ms = if let Some((root, path)) = scan {
                let files = super::folder_scans::read::load_files(sql, &root, Some(&path))?
                    .remove(&path)
                    .ok_or_else(|| {
                        DbError::Message(format!("candidate {path} has no scanned files"))
                    })?;
                let files = crate::import::folder_scanner::CategorizedFiles { files };
                let durations = crate::import::probe::source_durations(&files)
                    .map_err(|error| DbError::Message(error.to_string()))?;
                crate::import::track_slots::audio_durations(&files, &durations)
                    .map_err(|error| DbError::Message(error.to_string()))?
            } else {
                // An unscanned candidate cannot import. Preserve its document;
                // no measured audio was available to choose an index layout.
                tracing::warn!(
                    content_hash = hash,
                    "pending draft has no scanned audio for its preserved source"
                );
                Vec::new()
            };
            // This migration writes its historical snapshot shape. Later
            // migrations extend stored snapshots without reloading the anchor.
            let applied = serde_json::json!({
                "payloads": payloads,
                "audio_durations_ms": audio_durations_ms,
            });
            let json = serde_json::to_string(&applied)
                .map_err(|error| DbError::Message(error.to_string()))?;
            sql.execute("INSERT INTO import_candidate_applied_source (content_hash, snapshot) VALUES (?, ?)", params![hash, json])?;
        }
        Ok(())
    }

    /// Extend each stored application with the partner documents it previously
    /// read from the archive, preserving the already frozen primary document.
    pub(crate) fn migrate_applied_source_partners(
        sql: &coven::MigrationContext<'_>,
    ) -> Result<(), DbError> {
        let snapshots = sql.query(
            "SELECT content_hash, snapshot FROM import_candidate_applied_source ORDER BY content_hash",
            [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )?;
        for (hash, snapshot) in snapshots {
            let mut snapshot: serde_json::Value = serde_json::from_str(&snapshot)
                .map_err(|error| DbError::Message(error.to_string()))?;
            let object = snapshot.as_object_mut().ok_or_else(|| {
                DbError::Message(format!("candidate {hash} has a non-object source snapshot"))
            })?;
            let refs = sql.query(
                "SELECT source, release_id FROM import_candidate_provenance_partner WHERE content_hash = ? ORDER BY source",
                [&hash],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )?;
            let mut partners = Vec::with_capacity(refs.len());
            for (catalog, key) in refs {
                let record = MetadataRef::new(catalog.parse().map_err(DbError::Message)?, key);
                let partner = load_release_payloads_on(sql, &record)
                    .map_err(|error| DbError::Message(error.to_string()))?
                    .ok_or_else(|| DbError::Message(format!(
                        "candidate {hash} has no archived source document for partner {record:?}"
                    )))?;
                partners.push(partner);
            }
            object.insert("partners".into(), serde_json::to_value(partners)
                .map_err(|error| DbError::Message(error.to_string()))?);
            let json = serde_json::to_string(&snapshot)
                .map_err(|error| DbError::Message(error.to_string()))?;
            sql.execute(
                "UPDATE import_candidate_applied_source SET snapshot = ? WHERE content_hash = ?",
                params![json, hash],
            )?;
        }
        Ok(())
    }

    /// The archived set for one release: the anchor document and everything it
    /// names, each read by id in one read.
    pub(crate) async fn load_release_payloads(
        &self,
        release: &MetadataRef,
    ) -> Result<Option<crate::import::payloads::ReleasePayloads>, ImportError> {
        let release = release.clone();
        self.read(move |sql| Ok(load_release_payloads_on(&sql, &release)))
            .await
            .map_err(|error| ImportError::Db(crate::library::LibraryError::Database(error)))?
    }

    /// Write documents, replacing any already stored under the same entity. One
    /// transaction: a payload set is written whole or not at all, which is what
    /// lets a reader treat the anchor document's presence as the whole set's.
    pub async fn save_source_release_payloads(
        &self,
        payloads: &[DbSourceReleasePayload],
    ) -> Result<(), DbError> {
        self.write_source_release_payloads(payloads, &[]).await
    }

    /// Replace a lookup's documents atomically, clearing previously stored
    /// answers for related entities absent from the successful lookup set.
    /// Offline replay must not resurrect stale supporting metadata.
    pub(crate) async fn replace_release_payloads(
        &self,
        payloads: &[DbSourceReleasePayload],
        invalidated: &[(PayloadSource, String)],
    ) -> Result<(), DbError> {
        self.write_source_release_payloads(payloads, invalidated)
            .await
    }

    async fn write_source_release_payloads(
        &self,
        payloads: &[DbSourceReleasePayload],
        invalidated: &[(PayloadSource, String)],
    ) -> Result<(), DbError> {
        let payloads = payloads.to_vec();
        let invalidated = invalidated.to_vec();
        self.call(move |sql| {
            for (source, key) in &invalidated {
                sql.execute("DELETE FROM source_release_payloads WHERE source = ? AND source_release_id = ?", params![source.as_str(), key])?;
            }
            for payload in &payloads {
                sql.execute(
                    "INSERT INTO source_release_payloads \
                         (source, source_release_id, json, fetched_at) VALUES (?, ?, ?, ?) \
                     ON CONFLICT(source, source_release_id) DO UPDATE SET \
                         json = excluded.json, fetched_at = excluded.fetched_at",
                    params![
                        payload.source.as_str(),
                        payload.source_release_id,
                        payload.json,
                        payload.fetched_at.to_rfc3339(),
                    ],
                )?;
            }
            Ok(())
        })
        .await
    }

    /// The documents stored under each of `keys`, in one read. Keys with no row
    /// are absent from the map: the caller decides which of them it required.
    pub async fn load_source_release_payloads(
        &self,
        keys: &[(crate::import::PayloadSource, String)],
    ) -> Result<HashMap<(crate::import::PayloadSource, String), String>, DbError> {
        let keys = keys.to_vec();
        self.read(move |sql| {
            let mut found = HashMap::with_capacity(keys.len());
            for (source, source_release_id) in keys {
                let json: Option<String> = sql
                    .query_row(
                        "SELECT json FROM source_release_payloads \
                         WHERE source = ? AND source_release_id = ?",
                        params![source.as_str(), source_release_id],
                        |row| row.get(0),
                    )
                    .optional()?;
                if let Some(json) = json {
                    found.insert((source, source_release_id), json);
                }
            }
            Ok(found)
        })
        .await
    }
}
