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

pub(super) fn validate_scan_item_ownership(
    watched_folder_path: &str,
    entry_key: &str,
    item: &crate::import::folder_scanner::ScanItem,
) -> Result<(), DbError> {
    if item.persisted_key() != entry_key {
        return Err(DbError::Message(format!(
            "folder scan entry key {entry_key} does not match its item key {}",
            item.persisted_key()
        )));
    }
    let root = std::path::Path::new(watched_folder_path);
    let (item_root, item_path) = match item {
        crate::import::folder_scanner::ScanItem::Discovered(candidate)
        | crate::import::folder_scanner::ScanItem::Valid(candidate) => (
            candidate.watched_folder_path.as_str(),
            candidate.path.as_path(),
        ),
        crate::import::folder_scanner::ScanItem::Invalid(candidate) => (
            candidate.watched_folder_path.as_str(),
            candidate.path.as_path(),
        ),
        crate::import::folder_scanner::ScanItem::Boundary(boundary) => (
            boundary.key.watched_folder_path.as_str(),
            std::path::Path::new(entry_key),
        ),
    };
    if item_root != watched_folder_path || !item_path.starts_with(root) {
        return Err(DbError::Message(format!(
            "folder scan entry {entry_key} does not belong to watched folder {watched_folder_path}"
        )));
    }
    match item {
        crate::import::folder_scanner::ScanItem::Discovered(candidate)
        | crate::import::folder_scanner::ScanItem::Valid(candidate) => {
            if !candidate.file_root.starts_with(root) {
                return Err(DbError::Message(format!(
                    "folder scan entry {entry_key} reads files outside its watched folder"
                )));
            }
            for resolved in &candidate.resolved_boundaries {
                validate_decision_key_ownership(watched_folder_path, &resolved.key)?;
            }
            if let Some(key) = &candidate.combine_ancestor_key {
                validate_decision_key_ownership(watched_folder_path, key)?;
            }
        }
        crate::import::folder_scanner::ScanItem::Invalid(candidate) => {
            for resolved in &candidate.resolved_boundaries {
                validate_decision_key_ownership(watched_folder_path, &resolved.key)?;
            }
        }
        crate::import::folder_scanner::ScanItem::Boundary(_) => {}
    }
    if let crate::import::folder_scanner::ScanItem::Boundary(boundary) = item {
        validate_decision_key_ownership(watched_folder_path, &boundary.key)?;
        for row in &boundary.tree_rows {
            validate_decision_key_ownership(watched_folder_path, &row.decision_key)?;
            for ancestor in &row.ancestor_decision_keys {
                validate_decision_key_ownership(watched_folder_path, ancestor)?;
            }
        }
        if boundary
            .candidate_keys
            .iter()
            .any(|key| !std::path::Path::new(key).starts_with(root))
        {
            return Err(DbError::Message(format!(
                "folder scan boundary {entry_key} contains a candidate outside its watched folder"
            )));
        }
    }
    Ok(())
}

pub(super) fn validate_decision_key_ownership(
    watched_folder_path: &str,
    key: &FolderReleaseDecisionKey,
) -> Result<(), DbError> {
    if key.watched_folder_path != watched_folder_path {
        return Err(DbError::Message(format!(
            "folder release decision belongs to {} instead of {watched_folder_path}",
            key.watched_folder_path
        )));
    }
    crate::import::folder_registry::validate_relative_path(&key.relative_folder_path)
        .map_err(|error| DbError::Message(error.to_string()))
}
