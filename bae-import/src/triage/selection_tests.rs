use super::*;
use CandidateAction as A;

fn member(key: &str, actions: &[CandidateAction]) -> SelectionMember {
    SelectionMember { candidate_key: crate::CandidateKey::test(key), actions: actions.to_vec() }
}

/// One row offering `actions`.
fn one(actions: &[CandidateAction]) -> SelectionPart {
    SelectionPart { actions: actions.to_vec(), count: 1 }
}

fn actions(offers: &[SelectionOffer]) -> Vec<CandidateAction> {
    offers.iter().map(|offer| offer.action).collect()
}

/// One row's menu is that row's own actions, in the surfaces' order, and
/// offers no Combine: one folder is not a selection to combine.
#[test]
fn one_member_offers_its_own_actions_in_order() {
    let offers =
        selection_offers(&[one(&[A::RevealFolder, A::Skip, A::Combine, A::Identify, A::Import])]);
    assert_eq!(actions(&offers), vec![A::Import, A::Identify, A::Skip, A::RevealFolder]);
    assert!(offers.iter().all(|offer| offer.enabled));
}

/// Each action applies to the members that offer it.
#[test]
fn each_action_applies_to_the_members_that_offer_it() {
    let members = [
        member("Album", &[A::Import, A::Identify, A::RevealFolder]),
        member("Other Album", &[A::Identify, A::RevealFolder]),
    ];
    let offers = selection_offers(&[
        one(&[A::Import, A::Identify, A::RevealFolder]),
        one(&[A::Identify, A::RevealFolder]),
    ]);
    let count =
        |action| offers.iter().find(|offer| offer.action == action).map(|offer| offer.count);
    assert_eq!(count(A::Import), Some(1));
    assert_eq!(count(A::Identify), Some(2));
    assert_eq!(count(A::Skip), None);
    assert_eq!(keys_for(&members, A::Import), vec![crate::CandidateKey::test("Album")]);
}

/// A part counts each of its rows: an action is offered for as many rows
/// as the parts offering it hold, and two rows alike make a selection to
/// combine.
#[test]
fn each_part_counts_every_row_it_holds() {
    let offers = selection_offers(&[
        SelectionPart { actions: vec![A::Import, A::Combine], count: 3 },
        one(&[A::Combine]),
    ]);
    assert_eq!(
        offers,
        vec![
            SelectionOffer { action: A::Import, count: 3, enabled: true },
            SelectionOffer { action: A::Combine, count: 4, enabled: true },
        ]
    );
}

/// Combining is the whole selection's: offered for two or more, runnable
/// only when every member can be combined.
#[test]
fn combining_is_offered_over_the_whole_selection() {
    let both = selection_offers(&[SelectionPart { actions: vec![A::Combine], count: 2 }]);
    assert_eq!(both, vec![SelectionOffer { action: A::Combine, count: 2, enabled: true }]);
    let held_back = selection_offers(&[one(&[A::Combine]), one(&[A::CancelImport])]);
    let combine = held_back
        .iter()
        .find(|offer| offer.action == A::Combine)
        .expect("two members are offered a combination");
    assert!(!combine.enabled);
}
