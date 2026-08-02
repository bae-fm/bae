//! The `source_release_payloads` store: provider documents keyed by the source
//! entity they describe.
//!
//! One table serves both halves of a release's life. Identification writes its
//! rows before it stores the verdict that names the release, so a candidate the
//! user opens replays what identification fetched with no network; the release
//! that candidate becomes reads the same rows back through its
//! `metadata_source` / `metadata_source_release_id` pointer.

use super::*;

impl Database {
    /// Write documents, replacing any already stored under the same entity. One
    /// transaction: a payload set is written whole or not at all, which is what
    /// lets a reader treat the anchor document's presence as the whole set's.
    pub async fn save_source_release_payloads(
        &self,
        payloads: &[DbSourceReleasePayload],
    ) -> Result<(), DbError> {
        let payloads = payloads.to_vec();
        self.call(move |sql| {
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
