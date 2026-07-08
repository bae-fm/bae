use crate::import::handle::ImportEvent;
use crate::import::types::ImportProgress;
use tokio::sync::{broadcast, mpsc as tokio_mpsc};
use tracing::warn;
/// Filter criteria for progress subscriptions
#[derive(Debug, Clone)]
enum SubscriptionFilter {
    Release {
        release_id: String,
    },
    Import {
        import_id: String,
    },
    /// Matches any event that has an import_id (for toolbar dropdown)
    AllImports,
}
impl SubscriptionFilter {
    fn matches(&self, progress: &ImportProgress) -> bool {
        match self {
            SubscriptionFilter::Release { release_id } => match progress {
                ImportProgress::Preparing { .. } => false,
                ImportProgress::Started { id, .. } => id == release_id,
                ImportProgress::Progress { id, .. } => id == release_id,
                ImportProgress::Complete { id, .. } => id == release_id,
                ImportProgress::RemoteUploadQueued { id, .. } => id == release_id,
                // A failed import has no release row (it never reached the
                // atomic finalize), so a release-filtered subscriber never
                // sees Failed.
                ImportProgress::Failed { .. } => false,
            },
            SubscriptionFilter::Import { import_id } => match progress {
                ImportProgress::Preparing { import_id: iid, .. } => iid == import_id,
                ImportProgress::Started { import_id: iid, .. } => iid == import_id,
                ImportProgress::Progress { import_id: iid, .. } => iid == import_id,
                ImportProgress::Complete { import_id: iid, .. } => iid == import_id,
                ImportProgress::RemoteUploadQueued { import_id: iid, .. } => iid == import_id,
                ImportProgress::Failed { import_id: iid, .. } => iid == import_id,
            },
            SubscriptionFilter::AllImports => true,
        }
    }
}
/// Handle for subscribing to import progress updates
#[derive(Clone)]
pub struct ImportProgressHandle {
    event_tx: broadcast::Sender<ImportEvent>,
    runtime_handle: tokio::runtime::Handle,
}
impl ImportProgressHandle {
    /// Create a new progress handle.
    pub fn new(
        event_tx: broadcast::Sender<ImportEvent>,
        runtime_handle: tokio::runtime::Handle,
    ) -> Self {
        Self {
            event_tx,
            runtime_handle,
        }
    }

    fn subscribe_filtered(
        &self,
        filter: SubscriptionFilter,
    ) -> tokio_mpsc::UnboundedReceiver<ImportProgress> {
        let mut event_rx = self.event_tx.subscribe();
        let (tx, rx) = tokio_mpsc::unbounded_channel();
        self.runtime_handle.spawn(async move {
            loop {
                match event_rx.recv().await {
                    Ok(ImportEvent::ImportProgress { progress, .. }) => {
                        if tx.is_closed() {
                            break;
                        }
                        if filter.matches(&progress) && tx.send(progress).is_err() {
                            break;
                        }
                    }
                    Ok(_) => {
                        if tx.is_closed() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!("Import progress lagged by {n} events");
                        continue;
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        rx
    }

    /// Subscribe to progress updates for a specific release
    /// Returns a receiver that yields only progress updates for the specified release
    /// Subscription is automatically removed when receiver is dropped
    pub fn subscribe_release(
        &self,
        release_id: String,
    ) -> tokio_mpsc::UnboundedReceiver<ImportProgress> {
        self.subscribe_filtered(SubscriptionFilter::Release { release_id })
    }
    /// Subscribe to progress updates for a specific import operation
    /// Returns a receiver that yields Preparing events and any event with matching import_id
    /// Subscription is automatically removed when receiver is dropped
    pub fn subscribe_import(
        &self,
        import_id: String,
    ) -> tokio_mpsc::UnboundedReceiver<ImportProgress> {
        self.subscribe_filtered(SubscriptionFilter::Import { import_id })
    }
    /// Subscribe to progress updates for ALL import operations
    /// Returns a receiver that yields any event with an import_id (for toolbar dropdown)
    /// Subscription is automatically removed when receiver is dropped
    pub fn subscribe_all_imports(&self) -> tokio_mpsc::UnboundedReceiver<ImportProgress> {
        self.subscribe_filtered(SubscriptionFilter::AllImports)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::types::{ImportPhase, PrepareStep};
    #[test]
    fn test_release_filter_matches_release_events() {
        let filter = SubscriptionFilter::Release {
            release_id: "release-1".to_string(),
        };
        assert!(filter.matches(&ImportProgress::Started {
            id: "release-1".to_string(),
            import_id: "import-1".to_string(),
        },),);
        assert!(filter.matches(&ImportProgress::Progress {
            id: "release-1".to_string(),
            percent: 50,
            phase: ImportPhase::ReferencingFiles,
            import_id: "import-1".to_string(),
        },),);
        assert!(filter.matches(&ImportProgress::Complete {
            id: "release-1".to_string(),
            import_id: "import-1".to_string(),
            album_id: "album-1".to_string(),
        },),);
        assert!(filter.matches(&ImportProgress::RemoteUploadQueued {
            id: "release-1".to_string(),
            import_id: "import-1".to_string(),
            album_id: "album-1".to_string(),
        },),);
        assert!(!filter.matches(&ImportProgress::Progress {
            id: "release-2".to_string(),
            percent: 50,
            phase: ImportPhase::ReferencingFiles,
            import_id: "import-1".to_string(),
        },),);
        assert!(!filter.matches(&ImportProgress::Preparing {
            import_id: "import-1".to_string(),
            step: PrepareStep::ParsingMetadata,
            album_title: "Test".to_string(),
            artist_name: "Artist".to_string(),
        },),);
        // A failed import has no release row, so a release-filtered
        // subscriber never sees Failed.
        assert!(!filter.matches(&ImportProgress::Failed {
            error: "error".to_string(),
            import_id: "import-1".to_string(),
        },),);
    }
    #[test]
    fn test_import_filter_matches_preparing_events() {
        let filter = SubscriptionFilter::Import {
            import_id: "import-1".to_string(),
        };
        assert!(filter.matches(&ImportProgress::Preparing {
            import_id: "import-1".to_string(),
            step: PrepareStep::ParsingMetadata,
            album_title: "Test".to_string(),
            artist_name: "Artist".to_string(),
        },),);
        assert!(filter.matches(&ImportProgress::Preparing {
            import_id: "import-1".to_string(),
            step: PrepareStep::WritingCoverArt,
            album_title: "Test".to_string(),
            artist_name: "Artist".to_string(),
        },),);
        assert!(!filter.matches(&ImportProgress::Preparing {
            import_id: "import-2".to_string(),
            step: PrepareStep::ParsingMetadata,
            album_title: "Test".to_string(),
            artist_name: "Artist".to_string(),
        },),);
    }
    #[test]
    fn test_import_filter_matches_events_with_import_id() {
        let filter = SubscriptionFilter::Import {
            import_id: "import-1".to_string(),
        };
        assert!(filter.matches(&ImportProgress::Started {
            id: "release-1".to_string(),
            import_id: "import-1".to_string(),
        },),);
        assert!(filter.matches(&ImportProgress::Progress {
            id: "release-1".to_string(),
            percent: 50,
            phase: ImportPhase::ReferencingFiles,
            import_id: "import-1".to_string(),
        },),);
        assert!(filter.matches(&ImportProgress::Complete {
            id: "release-1".to_string(),
            import_id: "import-1".to_string(),
            album_id: "album-1".to_string(),
        },),);
        assert!(filter.matches(&ImportProgress::RemoteUploadQueued {
            id: "release-1".to_string(),
            import_id: "import-1".to_string(),
            album_id: "album-1".to_string(),
        },),);
        assert!(filter.matches(&ImportProgress::Failed {
            error: "error".to_string(),
            import_id: "import-1".to_string(),
        },),);
        assert!(!filter.matches(&ImportProgress::Progress {
            id: "release-1".to_string(),
            percent: 50,
            phase: ImportPhase::ReferencingFiles,
            import_id: "import-2".to_string(),
        },),);
    }
    #[test]
    fn test_all_imports_filter_matches_any_import_event() {
        let filter = SubscriptionFilter::AllImports;
        assert!(filter.matches(&ImportProgress::Preparing {
            import_id: "import-1".to_string(),
            step: PrepareStep::ParsingMetadata,
            album_title: "Test".to_string(),
            artist_name: "Artist".to_string(),
        },),);
        assert!(filter.matches(&ImportProgress::Preparing {
            import_id: "import-2".to_string(),
            step: PrepareStep::WritingCoverArt,
            album_title: "Test".to_string(),
            artist_name: "Artist".to_string(),
        },),);
        assert!(filter.matches(&ImportProgress::Started {
            id: "release-1".to_string(),
            import_id: "import-1".to_string(),
        },),);
        assert!(filter.matches(&ImportProgress::Progress {
            id: "release-1".to_string(),
            percent: 50,
            phase: ImportPhase::ReferencingFiles,
            import_id: "import-2".to_string(),
        },),);
        assert!(filter.matches(&ImportProgress::Complete {
            id: "release-1".to_string(),
            import_id: "import-3".to_string(),
            album_id: "album-1".to_string(),
        },),);
        assert!(filter.matches(&ImportProgress::RemoteUploadQueued {
            id: "release-1".to_string(),
            import_id: "import-3".to_string(),
            album_id: "album-1".to_string(),
        },),);
        assert!(filter.matches(&ImportProgress::Failed {
            error: "error".to_string(),
            import_id: "import-4".to_string(),
        },),);
    }
    #[test]
    fn test_all_prepare_steps_exist() {
        let steps = [
            PrepareStep::ParsingMetadata,
            PrepareStep::WritingCoverArt,
            PrepareStep::DiscoveringFiles,
            PrepareStep::ValidatingTracks,
            PrepareStep::SavingToDatabase,
        ];
        for step in steps {
            let event = ImportProgress::Preparing {
                import_id: "test".to_string(),
                step,
                album_title: "Test".to_string(),
                artist_name: "Artist".to_string(),
            };
            let filter = SubscriptionFilter::Import {
                import_id: "test".to_string(),
            };
            assert!(filter.matches(&event));
        }
    }
}
