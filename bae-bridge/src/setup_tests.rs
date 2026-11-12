use super::*;

/// A library whose replacement of this device's copy is unfinished
/// crosses with the call that finishes it.
#[test]
fn a_library_waiting_on_a_replacement_crosses_with_the_call_that_finishes_it() {
    for (core, bridge) in [
        (coven::ReplacedBy::Restore, crate::BridgeReplacedBy::Restore),
        (coven::ReplacedBy::Reset, crate::BridgeReplacedBy::Reset),
    ] {
        let library = BridgeLibrary::from_core_info(bae_app_dir::LibraryInfo {
            id: "library".into(),
            name: "Household".into(),
            path: "/libraries/library".into(),
            is_active: true,
            state: bae_app_dir::LibraryState::ReplacementUnfinished(core),
        })
        .unwrap();
        assert!(
            matches!(
                library.state,
                crate::BridgeLibraryState::ReplacementUnfinished { by, .. } if by == bridge
            ),
            "{:?}",
            library.state
        );
    }
}
