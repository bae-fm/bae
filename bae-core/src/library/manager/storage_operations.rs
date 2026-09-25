use super::*;

/// The pin-state answer for a release file. A file id that names no
/// `release_files` row (e.g. a path-traversal token forged by a peer, or a since-
/// deleted file) is `RejectedBadId` rather than silently folded into `NotPinned`,
/// so the caller that holds the diagnostics sink can count it as an anomaly before
/// treating it as not pinned.
#[derive(Clone, Copy)]
pub(super) enum ReleasePinState {
    Pinned,
    NotPinned,
    RejectedBadId,
}

/// Whether each release is pinned offline, one answer per entry in the order
/// given, through the handle's set-based cache-state query. Each entry is a
/// release's representative file id, or `None` for a release with no files —
/// which is not pinned and asks coven nothing.
///
/// coven resolves each live `release_files` row and answers the pin question for
/// the whole set in one read, so a list that shows a pin marker per row costs one
/// call rather than one per row. A file id that names no such row can't be
/// pinned and is rejected (never trusted); a file with no committed cloud object
/// — a Local release's, or one whose upload has not landed — has nothing to keep
/// a copy of and is not pinned; a real I/O failure on the pin check still
/// surfaces.
pub(super) async fn release_file_pin_states(
    database: &Database,
    any_file_ids: &[Option<&str>],
) -> Result<Vec<ReleasePinState>, LibraryError> {
    let named = named_file_ids(any_file_ids);
    if named.is_empty() {
        return Ok(vec![ReleasePinState::NotPinned; any_file_ids.len()]);
    }
    let pinned = database
        .rows_pinned(crate::sync::RELEASE_FILES_NAMESPACE, named.clone())
        .await
        .map_err(|e| LibraryError::blob(format!("pin-state for {named:?}"), e))?;
    Ok(release_pin_states_from_answers(any_file_ids, &pinned))
}

/// The file ids coven is asked about: every release's representative file, in
/// order, skipping releases with none.
pub(super) fn named_file_ids(any_file_ids: &[Option<&str>]) -> Vec<String> {
    any_file_ids
        .iter()
        .flatten()
        .map(|file_id| (*file_id).to_string())
        .collect()
}

/// Each release's pin state from coven's answers for [`named_file_ids`]: one
/// answer per named id, in order, so stepping those answers through the named
/// slots puts each back beside the release it came from.
pub(super) fn release_pin_states_from_answers(
    any_file_ids: &[Option<&str>],
    answers: &[Option<bool>],
) -> Vec<ReleasePinState> {
    let mut answers = any_file_ids.iter().flatten().zip(answers);
    any_file_ids
        .iter()
        .map(
            |any_file_id| match any_file_id.and_then(|_| answers.next()) {
                Some((_, Some(true))) => ReleasePinState::Pinned,
                Some((_, Some(false))) | None => ReleasePinState::NotPinned,
                Some((file_id, None)) => {
                    warn!("pin-state check: no release_files row for {file_id}");
                    ReleasePinState::RejectedBadId
                }
            },
        )
        .collect()
}
