use super::*;

pub(super) fn decode_import_candidate_state_row(
    row: &coven::rusqlite::Row<'_>,
) -> Result<DbImportCandidateState, DbError> {
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
            return Err(DbError::Message(format!(
                "import_candidate_state row {content_hash} holds a half-written identify result"
            )));
        }
    };
    let file_edits = decode_candidate_file_edits_row(row, &content_hash)?;
    Ok(DbImportCandidateState {
        content_hash,
        folder_path: row.get("folder_path")?,
        identify,
        file_edits,
        identity_pick: row.get("identity_pick")?,
    })
}

pub(super) fn decode_candidate_file_edits_row(
    row: &coven::rusqlite::Row<'_>,
    content_hash: &str,
) -> Result<CandidateFileEdits, DbError> {
    let stored: String = row.get("sheet_bindings")?;
    let sheet_bindings = serde_json::from_str(&stored).map_err(|error| {
        DbError::Message(format!(
            "import_candidate_state row {content_hash} has unreadable sheet bindings: {error}"
        ))
    })?;
    let stored: String = row.get("file_roles")?;
    let file_roles = serde_json::from_str(&stored).map_err(|error| {
        DbError::Message(format!(
            "import_candidate_state row {content_hash} has unreadable file roles: {error}"
        ))
    })?;
    let stored: String = row.get("sheet_discs")?;
    let sheet_discs = serde_json::from_str(&stored).map_err(|error| {
        DbError::Message(format!(
            "import_candidate_state row {content_hash} has unreadable sheet discs: {error}"
        ))
    })?;
    let edit_revision: i64 = row.get("edit_revision")?;
    let edit_revision = u64::try_from(edit_revision).map_err(|_| {
        DbError::Message(format!(
            "import_candidate_state row {content_hash} has negative edit_revision"
        ))
    })?;
    Ok(CandidateFileEdits {
        sheet_bindings,
        file_roles,
        sheet_discs,
        revision: edit_revision,
    })
}
