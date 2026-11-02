use super::*;
use tempfile::TempDir;

#[tokio::test]
async fn discovery_uses_coven_names_and_sorts_the_active_store_first() {
    let tmp = TempDir::new().unwrap();
    let app_dir = AppDir::at(tmp.path()).unwrap();
    let installation = coven::TestCoven::new();
    let first = crate::db::configure_store(installation.builder(app_dir.store_layout()))
        .create_store("First")
        .await
        .unwrap();
    let second = crate::db::configure_store(installation.builder(app_dir.store_layout()))
        .create_store("Second")
        .await
        .unwrap();
    let first_id = first.store_id();
    let second_id = second.store_id();
    app_dir.save_active_library(second_id).unwrap();
    first.close().await.unwrap();
    second.close().await.unwrap();
    let libraries = app_dir.discover_libraries().await.unwrap();
    assert_eq!(
        libraries.iter().map(|l| (l.id.as_str(), l.name.as_str(), l.is_active)).collect::<Vec<_>>(),
        vec![
            (second_id.to_string().as_str(), "Second", true),
            (first_id.to_string().as_str(), "First", false)
        ]
    );
    assert!(libraries.iter().all(|l| matches!(l.state, LibraryState::Openable(None))));
    assert_eq!(libraries[0].path, app_dir.registered_library(&second_id.to_string()));
}

#[tokio::test]
async fn discovery_reports_a_broken_store_instead_of_omitting_it() {
    let tmp = TempDir::new().unwrap();
    let app_dir = AppDir::at(tmp.path()).unwrap();
    let id = coven::StoreId(uuid::Uuid::new_v4());
    app_dir.store_layout().create_store_dir(id, &coven::UuidIds).unwrap();
    let error = app_dir.discover_libraries().await.unwrap_err();
    assert!(error.to_string().contains(&id.to_string()), "{error}");
}

/// A library whose storage settings can't be read is listed with why, and
/// the others still list as they are.
#[tokio::test]
async fn discovery_lists_a_library_whose_settings_cant_be_read() {
    let tmp = TempDir::new().unwrap();
    let app_dir = AppDir::at(tmp.path()).unwrap();
    let installation = coven::TestCoven::new();
    let mut ids = Vec::new();
    for name in ["Openable", "Unreadable"] {
        let handle = crate::db::configure_store(installation.builder(app_dir.store_layout()))
            .create_store(name)
            .await
            .unwrap();
        ids.push(handle.store_id());
        handle.close().await.unwrap();
    }
    std::fs::write(
        tmp.path().join("stores").join(ids[1].to_string()).join("storage.json"),
        "{not settings",
    )
    .unwrap();

    let libraries = app_dir.discover_libraries().await.unwrap();
    let state = |name: &str| {
        &libraries.iter().find(|library| library.name == name).expect("every store is listed").state
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
    std::fs::write(app_dir.active_library_pointer(), " \n").unwrap();
    assert!(app_dir.discover_libraries().await.is_err());
    std::fs::remove_file(app_dir.active_library_pointer()).unwrap();
    std::fs::create_dir(app_dir.active_library_pointer()).unwrap();
    assert!(app_dir.discover_libraries().await.is_err());
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

#[tokio::test]
async fn an_app_dir_under_a_fresh_home_exists_and_holds_a_new_store() {
    let home = TempDir::new().unwrap();
    let app_dir = AppDir::under_home(home.path()).unwrap();
    assert!(home.path().join(".bae").is_dir());
    let installation = coven::TestCoven::new();
    let handle = crate::db::configure_store(installation.builder(app_dir.store_layout()))
        .create_store("First")
        .await
        .unwrap();
    handle.close().await.unwrap();
}

#[test]
fn a_library_id_is_a_uuid_never_a_path() {
    let id = coven::StoreId(uuid::Uuid::new_v4());
    assert_eq!(parse_library_id(&id.to_string()).unwrap(), id);
    for id in ["../outside", "invalid"] {
        assert!(matches!(parse_library_id(id), Err(AppDirError::Invalid(_))));
    }
}
