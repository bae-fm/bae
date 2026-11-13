use super::*;
use tempfile::TempDir;

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
    let tmp = TempDir::new().unwrap();
    let app_dir = AppDir::at(tmp.path()).unwrap();
    let installation = installation(&app_dir);
    let first_id = store(&installation, "First").await;
    let second_id = store(&installation, "Second").await;
    app_dir.save_active_library(second_id).unwrap();
    let libraries = app_dir
        .discover_libraries(&installation, coven::task::HeardBugs::shared().as_ref())
        .await
        .unwrap();
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
    let tmp = TempDir::new().unwrap();
    let app_dir = AppDir::at(tmp.path()).unwrap();
    let installation = installation(&app_dir);
    let named = store(&installation, "Named").await;
    let broken = coven::StoreId(coven::IdSource::new_id(&coven::UuidIds));
    installation.create_store_dir(broken, &coven::UuidIds).unwrap();
    let libraries = app_dir
        .discover_libraries(&installation, coven::task::HeardBugs::shared().as_ref())
        .await
        .unwrap();
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
async fn discovery_lists_a_library_whose_settings_cant_be_read() {
    let tmp = TempDir::new().unwrap();
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
        .unwrap();
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
    let tmp = TempDir::new().unwrap();
    let app_dir = AppDir::at(tmp.path()).unwrap();
    let installation = installation(&app_dir);
    std::fs::write(app_dir.active_library_pointer(), " \n").unwrap();
    assert!(app_dir
        .discover_libraries(&installation, coven::task::HeardBugs::shared().as_ref())
        .await
        .is_err());
    std::fs::remove_file(app_dir.active_library_pointer()).unwrap();
    std::fs::create_dir(app_dir.active_library_pointer()).unwrap();
    assert!(app_dir
        .discover_libraries(&installation, coven::task::HeardBugs::shared().as_ref())
        .await
        .is_err());
}

#[test]
fn installation_id_is_shared_by_libraries_and_survives_reopening() {
    let tmp = TempDir::new().unwrap();
    let app_dir = AppDir::at(tmp.path()).unwrap();
    let ids = coven::SequentialIds::new();
    let first = app_dir.install_id(&ids).unwrap();
    assert_eq!(app_dir.install_id(&ids).unwrap(), first);
    assert_eq!(AppDir::at(tmp.path()).unwrap().install_id(&ids).unwrap(), first);
    std::fs::write(tmp.path().join("install-id"), "invalid").unwrap();
    assert!(app_dir.install_id(&ids).is_err());
}

#[test]
fn an_unreadable_installation_id_names_its_file() {
    let tmp = TempDir::new().unwrap();
    let app_dir = AppDir::at(tmp.path()).unwrap();
    let ids = coven::SequentialIds::new();
    let path = tmp.path().join("install-id");
    std::fs::create_dir(&path).unwrap();
    let error = app_dir.install_id(&ids).unwrap_err();
    assert!(error.to_string().contains(&path.display().to_string()), "{error}");
    std::fs::remove_dir(&path).unwrap();
    std::fs::write(&path, "invalid").unwrap();
    let error = app_dir.install_id(&ids).unwrap_err();
    assert!(error.to_string().contains(&path.display().to_string()), "{error}");
}

#[tokio::test]
async fn an_app_dir_under_a_fresh_home_exists_and_holds_a_new_store() {
    let home = TempDir::new().unwrap();
    let app_dir = AppDir::under_home(home.path()).unwrap();
    assert!(home.path().join(".bae").is_dir());
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
