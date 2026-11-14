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
            name: Some("Household".into()),
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

/// A store whose name can't be read crosses with no name.
#[test]
fn a_library_whose_name_cant_be_read_has_no_name() {
    let library = BridgeLibrary::from_core_info(bae_app_dir::LibraryInfo {
        id: "library".into(),
        name: None,
        path: "/libraries/library".into(),
        is_active: false,
        state: bae_app_dir::LibraryState::NameUnreadable,
    })
    .unwrap();
    assert_eq!(library.name, None);
    assert!(matches!(library.state, crate::BridgeLibraryState::NameUnreadable));
}

fn listed(id: &str, active: bool, openable: bool) -> bae_app_dir::LibraryInfo {
    bae_app_dir::LibraryInfo {
        id: id.into(),
        name: Some(id.into()),
        path: format!("/libraries/{id}").into(),
        is_active: active,
        state: if openable {
            bae_app_dir::LibraryState::Openable(None)
        } else {
            bae_app_dir::LibraryState::NameUnreadable
        },
    }
}

/// The active library names the libraries before and after it among those that
/// open, wrapping at the ends and skipping the ones that don't open.
#[test]
fn the_active_library_names_its_neighbours_that_open() {
    let libraries = BridgeLibrary::list_from_core(vec![
        listed("a", true, true),
        listed("b", false, false),
        listed("c", false, true),
        listed("d", false, true),
    ])
    .unwrap();
    let neighbours: Vec<_> = libraries
        .iter()
        .map(|library| (&library.next_openable_id, &library.previous_openable_id))
        .collect();
    assert_eq!(
        neighbours,
        vec![
            (&Some("c".to_string()), &Some("d".to_string())),
            (&None, &None),
            (&None, &None),
            (&None, &None),
        ]
    );
}

/// Every library says whether another is listed beside it.
#[test]
fn a_library_says_whether_another_is_listed_beside_it() {
    let alone = BridgeLibrary::list_from_core(vec![listed("a", true, true)]).unwrap();
    assert!(!alone[0].has_other_libraries);
    let pair =
        BridgeLibrary::list_from_core(vec![listed("a", true, true), listed("b", false, false)])
            .unwrap();
    assert!(pair.iter().all(|library| library.has_other_libraries));
}

/// With no other library that opens, or an active library that doesn't open,
/// nobody names a neighbour.
#[test]
fn no_neighbour_is_named_without_another_library_that_opens() {
    for listing in [
        vec![listed("a", true, true), listed("b", false, false)],
        vec![listed("a", true, false), listed("b", false, true), listed("c", false, true)],
    ] {
        let libraries = BridgeLibrary::list_from_core(listing).unwrap();
        assert!(libraries
            .iter()
            .all(|library| library.next_openable_id.is_none()
                && library.previous_openable_id.is_none()));
    }
}
