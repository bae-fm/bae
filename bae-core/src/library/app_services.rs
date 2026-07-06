use super::manager::LibraryManager;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use crate::identify::IdentifyServiceHandle;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use crate::import::ImportServiceHandle;
use crate::playback::PlaybackHandle;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use crate::signals::ExtractionServiceHandle;
use std::sync::Arc;

struct AppServicesInner {
    manager: LibraryManager,
    playback: PlaybackHandle,
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    import: ImportServiceHandle,
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    identify: IdentifyServiceHandle,
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    extraction: ExtractionServiceHandle,
}

/// The running application: library data layer + all service handles.
#[derive(Clone)]
pub struct AppServices {
    inner: Arc<AppServicesInner>,
}

impl std::fmt::Debug for AppServices {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppServices")
            .field("manager", &self.inner.manager)
            .finish_non_exhaustive()
    }
}

impl PartialEq for AppServices {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}

impl AppServices {
    pub fn new(
        manager: LibraryManager,
        playback: PlaybackHandle,
        #[cfg(not(any(target_os = "ios", target_os = "android")))] import: ImportServiceHandle,
        #[cfg(not(any(target_os = "ios", target_os = "android")))] identify: IdentifyServiceHandle,
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        extraction: ExtractionServiceHandle,
    ) -> Self {
        AppServices {
            inner: Arc::new(AppServicesInner {
                manager,
                playback,
                #[cfg(not(any(target_os = "ios", target_os = "android")))]
                import,
                #[cfg(not(any(target_os = "ios", target_os = "android")))]
                identify,
                #[cfg(not(any(target_os = "ios", target_os = "android")))]
                extraction,
            }),
        }
    }

    pub fn library_manager(&self) -> &LibraryManager {
        &self.inner.manager
    }

    pub async fn get_queue_snapshot(
        &self,
    ) -> Result<crate::queue::ResolvedQueueSnapshot, crate::library::LibraryError> {
        let projection = self
            .inner
            .playback
            .queue_projection()
            .await
            .map_err(crate::library::LibraryError::Playback)?;
        self.inner
            .manager
            .resolve_queue_projection(projection)
            .await
    }

    pub fn get_sync_status(&self) -> crate::library::SyncStatusSnapshot {
        self.inner.manager.get_sync_status()
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub fn get_import_candidates(&self) -> crate::import::ImportCandidatesSnapshot {
        self.inner.import.get_import_candidates()
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub fn get_candidate(&self, key: &str) -> Option<crate::import::ImportCandidateSnapshot> {
        self.inner.import.get_candidate(key)
    }

    pub fn playback(&self) -> &PlaybackHandle {
        &self.inner.playback
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub fn import(&self) -> &ImportServiceHandle {
        &self.inner.import
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub fn identify(&self) -> &IdentifyServiceHandle {
        &self.inner.identify
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub fn extraction(&self) -> &ExtractionServiceHandle {
        &self.inner.extraction
    }
}
