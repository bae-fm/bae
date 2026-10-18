use crate::config::AppDir;
use crate::library::LibraryError;

/// Leave the store before removing this installation. Offline failures leave
/// the store and bae's preferences intact; the caller retries the same request.
/// The caller must stop its playback and import services first.
pub async fn remove_local_library(
    app_dir: &AppDir,
    handle: &coven::CovenHandle,
    unsent: coven::IfUnsent,
) -> Result<(), LibraryError> {
    let store_id = handle.store_id();
    handle.delete_store(unsent).await?;
    remove_preferences(app_dir, store_id).await
}

pub(super) async fn remove_preferences(
    app_dir: &AppDir,
    store_id: coven::StoreId,
) -> Result<(), LibraryError> {
    let app_dir = app_dir.clone();
    tokio::task::spawn_blocking(move || {
        let id = store_id.to_string();
        let directory = app_dir.registered_library(&id);
        match std::fs::remove_dir_all(&directory) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(LibraryError::Io(error)),
        }
        let pointer = app_dir.active_library_pointer();
        let current = match std::fs::read_to_string(&pointer) {
            Ok(current) => current,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(LibraryError::Io(error)),
        };
        if current.trim() == id {
            std::fs::remove_file(pointer)?;
        }
        Ok(())
    })
    .await?
}
