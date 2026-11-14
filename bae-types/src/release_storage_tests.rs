use super::*;

#[test]
fn every_transition_reads_back_from_its_key() {
    for action in [
        ReleaseStorageAction::Upload,
        ReleaseStorageAction::Pin,
        ReleaseStorageAction::Unpin,
        ReleaseStorageAction::MoveToThisDevice,
    ] {
        assert_eq!(ReleaseStorageAction::from_key(action.key()), Some(action));
    }
    assert_eq!(ReleaseStorageAction::from_key("detach audio"), None);
}
