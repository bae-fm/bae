use super::*;
use bae_util::test_files::TestFolder;

/// A store named `name`, created in `installation` and closed, as discovery
/// lists it.
async fn store(installation: &coven::Coven, name: &str) -> coven::StoreId {
    installation.create_closed_store(name, std::sync::Arc::new(coven::UuidIds)).await.unwrap().id()
}

/// An in-memory installation of the stores under `app_dir`.
fn installation(app_dir: &AppDir) -> coven::Coven {
    coven::Coven::in_memory(app_dir.coven_app_dir())
}

#[tokio::test]
async fn discovery_uses_coven_names_and_sorts_the_active_store_first() {
    let tmp = TestFolder::new();
    let app_dir = AppDir::at(tmp.path()).unwrap();
    let installation = installation(&app_dir);
    let first_id = store(&installation, "First").await;
    let second_id = store(&installation, "Second").await;
    app_dir.save_active_library(second_id).unwrap();
    let libraries = app_dir
        .discover_libraries(&installation, coven::task::HeardBugs::shared().as_ref())
        .await
        .unwrap()
        .libraries;
    assert_eq!(
        libraries
            .iter()
            .map(|l| (l.id.as_str(), l.name.as_deref().unwrap(), l.is_active))
            .collect::<Vec<_>>(),
        vec![
            (second_id.to_string().as_str(), "Second", true),
            (first_id.to_string().as_str(), "First", false)
        ]
    );
    assert!(libraries.iter().all(|l| matches!(l.state, LibraryState::Openable(None))));
    assert_eq!(libraries[0].path, app_dir.registered_library(&second_id.to_string()));
}

/// A store whose name coven can't read is listed by its id beside the others
/// instead of failing the whole listing.
#[tokio::test]
async fn discovery_lists_a_store_whose_name_cant_be_read() {
    let tmp = TestFolder::new();
    let app_dir = AppDir::at(tmp.path()).unwrap();
    let installation = installation(&app_dir);
    let named = store(&installation, "Named").await;
    let broken = coven::StoreId(coven::IdSource::new_id(&coven::UuidIds));
    installation.create_store_dir(broken, &coven::UuidIds).unwrap();
    let libraries = app_dir
        .discover_libraries(&installation, coven::task::HeardBugs::shared().as_ref())
        .await
        .unwrap()
        .libraries;
    assert_eq!(libraries.len(), 2);
    assert_eq!(libraries[0].id, named.to_string());
    assert_eq!(libraries[0].name.as_deref(), Some("Named"));
    assert_eq!(libraries[1].id, broken.to_string());
    assert_eq!(libraries[1].name, None);
    assert!(matches!(libraries[1].state, LibraryState::NameUnreadable));
}

/// A library whose storage settings can't be read is listed with why, and
/// the others still list as they are.
#[tokio::test]
#[expect(
    clippy::disallowed_methods,
    reason = "file system: known violation, a test that reads or writes a library's own files by path instead of through coven"
)]
async fn discovery_lists_a_library_whose_settings_cant_be_read() {
    let tmp = TestFolder::new();
    let app_dir = AppDir::at(tmp.path()).unwrap();
    let installation = installation(&app_dir);
    let mut ids = Vec::new();
    for name in ["Openable", "Unreadable"] {
        ids.push(store(&installation, name).await);
    }
    std::fs::write(
        tmp.path().join("stores").join(ids[1].to_string()).join("storage.json"),
        "{not settings",
    )
    .unwrap();

    let libraries = app_dir
        .discover_libraries(&installation, coven::task::HeardBugs::shared().as_ref())
        .await
        .unwrap()
        .libraries;
    let state = |name: &str| {
        &libraries
            .iter()
            .find(|library| library.name.as_deref() == Some(name))
            .expect("every store is listed")
            .state
    };
    assert!(matches!(state("Openable"), LibraryState::Openable(None)));
    assert!(
        matches!(state("Unreadable"), LibraryState::Unreadable(_)),
        "{:?}",
        state("Unreadable")
    );
}

#[tokio::test]
async fn discovery_reports_invalid_or_unreadable_active_pointers() {
    let tmp = TestFolder::new();
    let app_dir = AppDir::at(tmp.path()).unwrap();
    let installation = installation(&app_dir);
    tmp.write(app_dir.active_library_pointer(), " \n");
    assert!(app_dir
        .discover_libraries(&installation, coven::task::HeardBugs::shared().as_ref())
        .await
        .is_err());
    tmp.remove(app_dir.active_library_pointer());
    tmp.create_folder(app_dir.active_library_pointer());
    assert!(app_dir
        .discover_libraries(&installation, coven::task::HeardBugs::shared().as_ref())
        .await
        .is_err());
}

#[test]
fn installation_id_is_shared_by_libraries_and_survives_reopening() {
    let tmp = TestFolder::new();
    let app_dir = AppDir::at(tmp.path()).unwrap();
    let ids = coven::SequentialIds::new();
    let first = app_dir.install_id(&ids).unwrap();
    assert_eq!(app_dir.install_id(&ids).unwrap(), first);
    assert_eq!(AppDir::at(tmp.path()).unwrap().install_id(&ids).unwrap(), first);
    tmp.write("install-id", "invalid");
    assert!(app_dir.install_id(&ids).is_err());
}

#[test]
fn an_unreadable_installation_id_names_its_file() {
    let tmp = TestFolder::new();
    let app_dir = AppDir::at(tmp.path()).unwrap();
    let ids = coven::SequentialIds::new();
    let path = tmp.path().join("install-id");
    tmp.create_folder(&path);
    let error = app_dir.install_id(&ids).unwrap_err();
    assert!(error.to_string().contains(&path.display().to_string()), "{error}");
    tmp.remove(&path);
    tmp.write(&path, "invalid");
    let error = app_dir.install_id(&ids).unwrap_err();
    assert!(error.to_string().contains(&path.display().to_string()), "{error}");
}

#[tokio::test]
async fn an_app_dir_under_a_fresh_home_exists_and_holds_a_new_store() {
    let home = TestFolder::new();
    let app_dir = AppDir::under_home(home.path()).unwrap();
    assert!(home.metadata(".bae").is_dir());
    store(&installation(&app_dir), "First").await;
}

#[test]
fn a_library_id_is_a_uuid_never_a_path() {
    let id = coven::StoreId(coven::IdSource::new_id(&coven::UuidIds));
    assert_eq!(parse_library_id(&id.to_string()).unwrap(), id);
    for id in ["../outside", "invalid"] {
        assert!(matches!(parse_library_id(id), Err(AppDirError::Invalid(_))));
    }
}

#[test]
fn a_panicking_store_listing_is_a_bug_not_a_configuration() {
    use bae_diagnostics::UiErrorCategory as C;
    let bug = || coven::task::Bug::new("list the stores", "boom");
    assert_eq!(AppDirError::from(coven::StoreListError::Bug(bug())).category(), C::Internal);
    assert_eq!(AppDirError::File(coven::FileError::Bug(bug())).category(), C::Internal);
    let unreadable = coven::FileError::Io {
        operation: "list store directories",
        path: "stores".into(),
        source: "permission denied".into(),
    };
    assert_eq!(AppDirError::from(coven::StoreListError::from(unreadable)).category(), C::Config);
}

fn listed(id: &str, active: bool, openable: bool) -> LibraryInfo {
    LibraryInfo {
        id: id.into(),
        name: Some(id.into()),
        path: format!("/libraries/{id}").into(),
        is_active: active,
        state: if openable { LibraryState::Openable(None) } else { LibraryState::NameUnreadable },
    }
}

fn neighbors(next: &str, previous: &str) -> Option<LibraryNeighbors> {
    Some(LibraryNeighbors { next_id: next.into(), previous_id: previous.into() })
}

/// The listing names the libraries before and after the active one among
/// those that open, wrapping at the ends and skipping the ones that don't
/// open.
#[test]
fn the_listing_names_the_active_librarys_neighbors_that_open() {
    let listing = LibraryListing::of(vec![
        listed("a", true, true),
        listed("b", false, false),
        listed("c", false, true),
        listed("d", false, true),
    ]);
    assert_eq!(listing.active_neighbors, neighbors("c", "d"));
    let listing = LibraryListing::of(vec![
        listed("a", false, true),
        listed("b", false, true),
        listed("c", true, true),
    ]);
    assert_eq!(listing.active_neighbors, neighbors("a", "b"));
}

/// The switcher shows when the device lists a second library, whether or
/// not it opens.
#[test]
fn the_switcher_shows_for_any_second_library() {
    assert!(!LibraryListing::of(vec![listed("a", true, true)]).shows_switcher);
    assert!(
        LibraryListing::of(vec![listed("a", true, true), listed("b", false, false)]).shows_switcher
    );
}

/// With no other library that opens, or an active library that doesn't
/// open, the listing names no neighbor.
#[test]
fn no_neighbor_is_named_without_another_library_that_opens() {
    for libraries in [
        vec![listed("a", true, true), listed("b", false, false)],
        vec![listed("a", true, false), listed("b", false, true), listed("c", false, true)],
        vec![listed("a", false, true), listed("b", false, true)],
    ] {
        assert_eq!(LibraryListing::of(libraries).active_neighbors, None);
    }
}

#[test]
fn removal_records_are_listed_until_cleared_and_clearing_twice_succeeds() {
    let tmp = TestFolder::new();
    let app_dir = AppDir::at(tmp.path()).unwrap();
    let first = coven::StoreId(uuid::Uuid::from_u128(1));
    let second = coven::StoreId(uuid::Uuid::from_u128(2));
    assert!(app_dir.recorded_removals().unwrap().is_empty());
    app_dir.record_removal(first).unwrap();
    app_dir.record_removal(second).unwrap();
    app_dir.record_removal(second).unwrap();
    assert_eq!(app_dir.recorded_removals().unwrap(), [first, second].into());
    app_dir.clear_removal_record(first).unwrap();
    app_dir.clear_removal_record(first).unwrap();
    assert_eq!(app_dir.recorded_removals().unwrap(), [second].into());
}

#[test]
fn recorded_removals_pass_over_files_that_are_not_records() {
    let tmp = TestFolder::new();
    let app_dir = AppDir::at(tmp.path()).unwrap();
    let id = coven::StoreId(uuid::Uuid::from_u128(1));
    app_dir.record_removal(id).unwrap();
    let records = app_dir.test_removal_records();
    tmp.write(records.join("not-an-id"), "x");
    tmp.write(records.join(id.to_string().to_uppercase()), "x");
    tmp.create_folder(records.join(coven::StoreId(uuid::Uuid::from_u128(3)).to_string()));
    assert_eq!(app_dir.recorded_removals().unwrap(), [id].into());
}

#[test]
fn deleting_a_library_clears_the_pointer_only_when_it_names_that_library() {
    let tmp = TestFolder::new();
    let app_dir = AppDir::at(tmp.path()).unwrap();
    let removed = coven::StoreId(uuid::Uuid::from_u128(1));
    let kept = coven::StoreId(uuid::Uuid::from_u128(2));
    tmp.create_folder(app_dir.registered_library(&removed.to_string()).join("cache"));
    app_dir.save_active_library(kept).unwrap();
    app_dir.delete_library_files(removed).unwrap();
    assert!(!tmp.exists(app_dir.registered_library(&removed.to_string())));
    assert_eq!(app_dir.active_library_id().unwrap(), Some(kept.to_string()));
    app_dir.delete_library_files(kept).unwrap();
    assert_eq!(app_dir.active_library_id().unwrap(), None);
    app_dir.delete_library_files(kept).unwrap();
}

#[test]
fn the_removal_lock_excludes_another_holder_until_released() {
    let tmp = TestFolder::new();
    let app_dir = AppDir::at(tmp.path()).unwrap();
    let waiting = app_dir.when_removal_lock_waits();
    let held = app_dir.lock_removals().unwrap();
    let second = bae_util::test_spawn::thread({
        let app_dir = app_dir.clone();
        move || drop(app_dir.lock_removals().unwrap())
    });
    waiting.blocking_recv().unwrap();
    assert!(!second.is_finished());
    drop(held);
    second.join().unwrap();
}
