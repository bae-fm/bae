use super::*;

impl Database {
    /// Write one candidate's device-local `import_candidate_state` row, keyed
    /// by `content_hash`, replacing any existing row under that hash. Never
    /// synced. `identified_at` is stamped here from the injected clock, not
    /// taken from `state` — see [`NewImportCandidateState`]'s doc.
    pub async fn save_import_candidate_state(
        &self,
        state: &NewImportCandidateState,
    ) -> Result<(), DbError> {
        let state = state.clone();
        let now = self.inner.clock.now().to_rfc3339();
        self.call(move |conn| {
            conn.execute(
                "INSERT OR REPLACE INTO import_candidate_state \
                     (content_hash, folder_path, verdict, probed_total_duration_ms, identified_at) \
                     VALUES (?, ?, ?, ?, ?)",
                params![
                    state.content_hash,
                    state.folder_path,
                    state.verdict,
                    state.probed_total_duration_ms,
                    now,
                ],
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
    }

    /// Every stored `import_candidate_state` row, keyed by `content_hash`. The
    /// queue is small enough to read whole; callers classify in memory rather
    /// than filtering in SQL.
    pub async fn load_import_candidate_states(
        &self,
    ) -> Result<HashMap<String, DbImportCandidateState>, DbError> {
        self.read(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT content_hash, folder_path, verdict, probed_total_duration_ms, identified_at \
                     FROM import_candidate_state",
            )?;
            let mut rows = stmt.query([])?;
            let mut out = HashMap::new();
            while let Some(row) = rows.next()? {
                let content_hash: String = row.get("content_hash")?;
                let identified_at = rfc3339_column(row, "identified_at")?;
                out.insert(
                    content_hash.clone(),
                    DbImportCandidateState {
                        content_hash,
                        folder_path: row.get("folder_path")?,
                        verdict: row.get("verdict")?,
                        probed_total_duration_ms: row.get("probed_total_duration_ms")?,
                        identified_at,
                    },
                );
            }
            Ok(out)
        })
        .await
    }
}
