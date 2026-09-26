//! What a selection of candidates can be told to do — decided once, from
//! what each member offers, for every surface that acts on a selection: the
//! pane a multi-selection opens, and the menu a click on the list opens for
//! one row or many.

use super::CandidateAction;

/// One selected candidate and the actions it offers right now: its
/// [`CandidateLiveState`](super::CandidateLiveState)'s.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionMember {
    pub candidate_key: String,
    pub actions: Vec<CandidateAction>,
}

/// One action a selection offers, the members it applies to, and whether it
/// can run as the selection stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionOffer {
    pub action: CandidateAction,
    pub candidate_keys: Vec<String>,
    pub enabled: bool,
}

/// The actions `members` offer, in the order every surface lists them.
///
/// Each action runs member by member over the members that offer it, so it is
/// offered when any does. Combining is one action over the whole selection: it
/// is offered once there are two members or more, and can run only when every
/// one of them could be combined — a surface shows it standing but unusable
/// otherwise, so the reason it cannot is on the member that holds it back
/// rather than a missing item.
pub fn selection_offers(members: &[SelectionMember]) -> Vec<SelectionOffer> {
    CandidateAction::ALL
        .into_iter()
        .filter_map(|action| {
            let offering: Vec<String> = members
                .iter()
                .filter(|member| member.actions.contains(&action))
                .map(|member| member.candidate_key.clone())
                .collect();
            match action {
                CandidateAction::Combine => (members.len() >= 2).then(|| SelectionOffer {
                    action,
                    enabled: offering.len() == members.len(),
                    candidate_keys: members
                        .iter()
                        .map(|member| member.candidate_key.clone())
                        .collect(),
                }),
                _ => (!offering.is_empty()).then_some(SelectionOffer {
                    action,
                    candidate_keys: offering,
                    enabled: true,
                }),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use CandidateAction as A;

    fn member(key: &str, actions: &[CandidateAction]) -> SelectionMember {
        SelectionMember {
            candidate_key: key.to_string(),
            actions: actions.to_vec(),
        }
    }

    fn actions(offers: &[SelectionOffer]) -> Vec<CandidateAction> {
        offers.iter().map(|offer| offer.action).collect()
    }

    /// One row's menu is that row's own actions, in the surfaces' order, and
    /// offers no Combine: one folder is not a selection to combine.
    #[test]
    fn one_member_offers_its_own_actions_in_order() {
        let offers = selection_offers(&[member(
            "Album",
            &[A::RevealFolder, A::Skip, A::Combine, A::Identify, A::ImportReady],
        )]);
        assert_eq!(
            actions(&offers),
            vec![A::ImportReady, A::Identify, A::Skip, A::RevealFolder]
        );
        assert!(offers.iter().all(|offer| offer.enabled));
    }

    /// Each action applies to the members that offer it.
    #[test]
    fn each_action_applies_to_the_members_that_offer_it() {
        let offers = selection_offers(&[
            member("Ready Album", &[A::ImportReady, A::Identify, A::RevealFolder]),
            member("Other Album", &[A::Identify, A::RevealFolder]),
        ]);
        let keys = |action| {
            offers
                .iter()
                .find(|offer| offer.action == action)
                .map(|offer| offer.candidate_keys.clone())
        };
        assert_eq!(keys(A::ImportReady), Some(vec!["Ready Album".to_string()]));
        assert_eq!(
            keys(A::Identify),
            Some(vec!["Ready Album".to_string(), "Other Album".to_string()])
        );
        assert_eq!(keys(A::Skip), None);
    }

    /// Combining is the whole selection's: offered for two or more, runnable
    /// only when every member can be combined.
    #[test]
    fn combining_is_offered_over_the_whole_selection() {
        let both = selection_offers(&[
            member("Disc 1", &[A::Combine]),
            member("Disc 2", &[A::Combine]),
        ]);
        assert_eq!(
            both,
            vec![SelectionOffer {
                action: A::Combine,
                candidate_keys: vec!["Disc 1".to_string(), "Disc 2".to_string()],
                enabled: true,
            }]
        );
        let held_back = selection_offers(&[
            member("Disc 1", &[A::Combine]),
            member("Importing", &[A::CancelImport]),
        ]);
        let combine = held_back
            .iter()
            .find(|offer| offer.action == A::Combine)
            .expect("two members are offered a combination");
        assert!(!combine.enabled);
    }
}
