//! What a selection of candidates can be told to do — decided once, from
//! what each member offers, for every surface that acts on a selection: the
//! pane a multi-selection opens, and the menu a click on the list opens for
//! one row or many.

use super::CandidateAction;

/// One selected candidate and the actions it offers right now: its
/// [`CandidateLiveState`](super::CandidateLiveState)'s.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionMember {
    pub candidate_key: crate::CandidateKey,
    pub actions: Vec<CandidateAction>,
}

/// One action a selection offers, how many members it applies to, and whether
/// it can run as the selection stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionOffer {
    pub action: CandidateAction,
    pub count: u64,
    pub enabled: bool,
}

/// Some number of selected rows that offer the same actions: the selection's
/// offers are counted over such parts, one per set of actions, rather than
/// over each row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionPart {
    pub actions: Vec<CandidateAction>,
    pub count: u64,
}

impl SelectionPart {
    /// Whether this part's rows take part in `action`: those that offer it,
    /// or every row for combining, which acts on the whole selection.
    fn takes(&self, action: CandidateAction) -> bool {
        takes_part(&self.actions, action)
    }
}

/// Whether a row offering `offered` takes part in `action`: it offers it, or
/// the action is combining, which acts on the whole selection.
fn takes_part(offered: &[CandidateAction], action: CandidateAction) -> bool {
    action == CandidateAction::Combine || offered.contains(&action)
}

/// The keys `action` runs on over `members`: those that offer it, or every
/// member for combining, which acts on the whole selection.
pub fn keys_for(members: &[SelectionMember], action: CandidateAction) -> Vec<crate::CandidateKey> {
    members
        .iter()
        .filter(|member| takes_part(&member.actions, action))
        .map(|member| member.candidate_key.clone())
        .collect()
}

/// The actions a selection made of `parts` offers, in the order every surface
/// lists them.
///
/// Each action runs row by row over the rows that offer it, so it is offered
/// when any does. Combining is one action over the whole selection: it is
/// offered once there are two rows or more, and can run only when every one of
/// them could be combined — a surface shows it standing but unusable
/// otherwise, so the reason it cannot is on the row that holds it back rather
/// than a missing item.
pub fn selection_offers(parts: &[SelectionPart]) -> Vec<SelectionOffer> {
    let rows: u64 = parts.iter().map(|part| part.count).sum();
    CandidateAction::ALL
        .into_iter()
        .filter_map(|action| {
            let count = parts.iter().filter(|part| part.takes(action)).map(|part| part.count).sum();
            match action {
                CandidateAction::Combine => (rows >= 2).then(|| SelectionOffer {
                    action,
                    count,
                    enabled: parts
                        .iter()
                        .all(|part| part.actions.contains(&CandidateAction::Combine)),
                }),
                _ => (count > 0).then_some(SelectionOffer { action, count, enabled: true }),
            }
        })
        .collect()
}

#[cfg(test)]
#[path = "selection_tests.rs"]
mod tests;
