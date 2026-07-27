use super::*;
use crate::import::folder_scanner::{
    CandidateFileEdits, FileRoleEdits, SheetBindingEdits, StoredCandidateEdits,
};

impl Database {
    /// Record one candidate's terminal identify verdict, keyed by
    /// `content_hash`. Never synced. `identified_at` is stamped here from the
    /// injected clock, not taken from `verdict` — see
    /// [`NewImportCandidateVerdict`]'s doc.
    ///
    /// The row's other half — the user's file decisions — is left untouched: an
    /// upsert rather than a replace, because a candidate can hold a decision
    /// before anything has identified it, and a verdict must not erase the
    /// decision that produced the shape it was derived from.
    pub async fn save_import_candidate_verdict(
        &self,
        verdict: &NewImportCandidateVerdict,
    ) -> Result<(), DbError> {
        let verdict = verdict.clone();
        let now = self.inner.clock.now().to_rfc3339();
        self.call(move |conn| {
            conn.execute(
                "INSERT INTO import_candidate_state \
                     (content_hash, folder_path, verdict, probed_total_duration_ms, identified_at, sheet_bindings, file_roles) \
                     VALUES (?, ?, ?, ?, ?, '{}', '{}') \
                 ON CONFLICT(content_hash) DO UPDATE SET \
                     folder_path = excluded.folder_path, \
                     verdict = excluded.verdict, \
                     probed_total_duration_ms = excluded.probed_total_duration_ms, \
                     identified_at = excluded.identified_at",
                params![
                    verdict.content_hash,
                    verdict.folder_path,
                    verdict.verdict,
                    verdict.probed_total_duration_ms,
                    now,
                ],
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
    }

    /// Record one candidate's user-set file decisions, **and clear whatever
    /// identification had concluded about it**, in one statement.
    ///
    /// The two are one operation, not two: binding a sheet or taking a file out
    /// of the tracklist changes what the folder is — a one-track image becomes
    /// a twelve-track disc, and its disc ID becomes computable — so the stored
    /// verdict was derived from a shape that no longer exists. Writing the
    /// decision without clearing it would leave the queue believing an answer
    /// to a question that changed.
    ///
    /// The content hash covers files, never role decisions, so this addresses
    /// the same row the verdict lived in rather than orphaning it.
    pub async fn save_import_candidate_file_edits(
        &self,
        content_hash: &str,
        folder_path: &str,
        edits: &CandidateFileEdits,
    ) -> Result<(), DbError> {
        let content_hash = content_hash.to_string();
        let folder_path = folder_path.to_string();
        let bindings = serde_json::to_string(&edits.sheet_bindings)
            .map_err(|e| DbError::Message(format!("encoding candidate sheet bindings: {e}")))?;
        let roles = serde_json::to_string(&edits.file_roles)
            .map_err(|e| DbError::Message(format!("encoding candidate file roles: {e}")))?;
        self.call(move |conn| {
            conn.execute(
                "INSERT INTO import_candidate_state \
                     (content_hash, folder_path, verdict, probed_total_duration_ms, identified_at, sheet_bindings, file_roles) \
                     VALUES (?, ?, NULL, NULL, NULL, ?, ?) \
                 ON CONFLICT(content_hash) DO UPDATE SET \
                     folder_path = excluded.folder_path, \
                     sheet_bindings = excluded.sheet_bindings, \
                     file_roles = excluded.file_roles, \
                     verdict = NULL, \
                     probed_total_duration_ms = NULL, \
                     identified_at = NULL",
                params![content_hash, folder_path, bindings, roles],
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
    }

    /// Every candidate's user-set file decisions, keyed by `content_hash` — the
    /// shape a folder scan takes so the roles it reports are the ones the user
    /// settled, not only the ones its filenames propose.
    ///
    /// Projected from the one stored-row read rather than a query of its own:
    /// the sweep and the scan want different halves of the same few hundred
    /// rows, and two queries over one table is two things to keep in step.
    pub async fn load_stored_candidate_edits(&self) -> Result<StoredCandidateEdits, DbError> {
        Ok(StoredCandidateEdits::new(
            self.load_import_candidate_states()
                .await?
                .into_iter()
                .filter(|(_, state)| !state.file_edits.is_empty())
                .map(|(hash, state)| (hash, state.file_edits))
                .collect(),
        ))
    }

    /// Every stored `import_candidate_state` row, keyed by `content_hash`. The
    /// queue is small enough to read whole; callers classify in memory rather
    /// than filtering in SQL.
    ///
    /// A row whose identify columns are half-set cannot come from either write
    /// path above, so it is external damage: the result reads as un-identified
    /// and the candidate is answered again, which is what a row this build
    /// cannot make sense of already does.
    ///
    /// Decisions that no longer decode read as none — same reasoning, and the
    /// scan then falls back to what the `FILE` directives and the extensions
    /// propose.
    pub async fn load_import_candidate_states(
        &self,
    ) -> Result<HashMap<String, DbImportCandidateState>, DbError> {
        self.read(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT content_hash, folder_path, verdict, probed_total_duration_ms, identified_at, sheet_bindings, file_roles \
                     FROM import_candidate_state",
            )?;
            let mut rows = stmt.query([])?;
            let mut out = HashMap::new();
            while let Some(row) = rows.next()? {
                let content_hash: String = row.get("content_hash")?;
                let verdict: Option<String> = row.get("verdict")?;
                let probed_total_duration_ms: Option<i64> = row.get("probed_total_duration_ms")?;
                let identified_at: Option<String> = row.get("identified_at")?;
                let identify = match (verdict, probed_total_duration_ms, identified_at) {
                    (Some(verdict), Some(probed_total_duration_ms), Some(_)) => {
                        Some(DbCandidateIdentifyResult {
                            verdict,
                            probed_total_duration_ms,
                            identified_at: rfc3339_column(row, "identified_at")?,
                        })
                    }
                    (None, None, None) => None,
                    _ => {
                        tracing::warn!(
                            "import_candidate_state row {content_hash} holds a half-written \
                             identify result; reading it as un-identified"
                        );
                        None
                    }
                };
                let stored: String = row.get("sheet_bindings")?;
                let sheet_bindings = serde_json::from_str(&stored).unwrap_or_else(|e| {
                    tracing::warn!(
                        "import_candidate_state row {content_hash} holds sheet bindings this \
                         build cannot read ({e}); the scan's own proposals stand"
                    );
                    SheetBindingEdits::default()
                });
                let stored: String = row.get("file_roles")?;
                let file_roles = serde_json::from_str(&stored).unwrap_or_else(|e| {
                    tracing::warn!(
                        "import_candidate_state row {content_hash} holds file roles this \
                         build cannot read ({e}); the scan's own proposals stand"
                    );
                    FileRoleEdits::default()
                });
                out.insert(
                    content_hash.clone(),
                    DbImportCandidateState {
                        content_hash,
                        folder_path: row.get("folder_path")?,
                        identify,
                        file_edits: CandidateFileEdits {
                            sheet_bindings,
                            file_roles,
                        },
                    },
                );
            }
            Ok(out)
        })
        .await
    }
}
