use super::*;

#[derive(Clone, Copy)]
pub(super) enum ReleasePinState {
    Pinned,
    NotPinned,
    RejectedBadId,
}

pub(super) async fn release_file_pin_states(
    database: &Database,
    any_file_ids: &[Option<&str>],
) -> Result<Vec<ReleasePinState>, LibraryError> {
    let groups = database
        .release_pin_files(
            any_file_ids
                .iter()
                .map(|id| id.map(str::to_string))
                .collect(),
        )
        .await?;
    let pinned = database
        .rows_pinned(crate::sync::RELEASE_FILES_NAMESPACE, pin_file_ids(&groups))
        .await?;
    Ok(release_pin_states_from_answers(&groups, &pinned))
}

pub(super) fn pin_file_ids(groups: &[Option<Vec<String>>]) -> Vec<String> {
    groups.iter().flatten().flatten().cloned().collect()
}

/// Every file must be pinned; coven can report intermediate per-file completion.
pub(super) fn release_pin_states_from_answers(
    groups: &[Option<Vec<String>>],
    answers: &[Option<bool>],
) -> Vec<ReleasePinState> {
    let mut offset = 0;
    let states = groups
        .iter()
        .map(|group| match group {
            None => {
                warn!("pin-state check: representative release file no longer exists");
                ReleasePinState::RejectedBadId
            }
            Some(files) => {
                let end = offset + files.len();
                let pinned = !files.is_empty()
                    && answers[offset..end]
                        .iter()
                        .all(|answer| *answer == Some(true));
                offset = end;
                if pinned {
                    ReleasePinState::Pinned
                } else {
                    ReleasePinState::NotPinned
                }
            }
        })
        .collect();
    assert_eq!(
        offset,
        answers.len(),
        "coven returned one pin answer per requested file"
    );
    states
}
