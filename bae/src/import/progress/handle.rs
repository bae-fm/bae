use crate::import::types::ImportProgress;
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};
use tokio::sync::mpsc as tokio_mpsc;
use tracing::info;
type SubscriptionId = u64;
/// Filter criteria for progress subscriptions
#[derive(Debug, Clone)]
enum SubscriptionFilter {
    Release {
        release_id: String,
    },
    Track {
        track_id: String,
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
                ImportProgress::Complete {
                    id,
                    release_id: rid,
                    ..
                } => id == release_id || rid.as_ref() == Some(release_id),
                ImportProgress::Failed { id, .. } => id == release_id,
            },
            SubscriptionFilter::Track { track_id } => match progress {
                ImportProgress::Preparing { .. } => false,
                ImportProgress::Started { id, .. } => id == track_id,
                ImportProgress::Progress { id, .. } => id == track_id,
                ImportProgress::Complete { id, .. } => id == track_id,
                ImportProgress::Failed { id, .. } => id == track_id,
            },
            SubscriptionFilter::Import { import_id } => match progress {
                ImportProgress::Preparing { import_id: iid, .. } => iid == import_id,
                ImportProgress::Started { import_id: iid, .. } => iid.as_ref() == Some(import_id),
                ImportProgress::Progress { import_id: iid, .. } => iid.as_ref() == Some(import_id),
                ImportProgress::Complete { import_id: iid, .. } => iid.as_ref() == Some(import_id),
                ImportProgress::Failed { import_id: iid, .. } => iid.as_ref() == Some(import_id),
            },
            SubscriptionFilter::AllImports => match progress {
                ImportProgress::Preparing { .. } => true,
                ImportProgress::Started { import_id, .. } => import_id.is_some(),
                ImportProgress::Progress { import_id, .. } => import_id.is_some(),
                ImportProgress::Complete { import_id, .. } => import_id.is_some(),
                ImportProgress::Failed { import_id, .. } => import_id.is_some(),
            },
        }
    }
}
struct Subscription {
    filter: SubscriptionFilter,
    tx: tokio_mpsc::UnboundedSender<ImportProgress>,
}
/// Handle for subscribing to import progress updates
#[derive(Clone)]
pub struct ImportProgressHandle {
    subscriptions: Arc<Mutex<HashMap<SubscriptionId, Subscription>>>,
    next_id: Arc<AtomicU64>,
}
impl ImportProgressHandle {
    /// Create a new progress handle and spawn background task to process progress updates
    pub fn new(
        mut progress_rx: tokio_mpsc::UnboundedReceiver<ImportProgress>,
        runtime_handle: tokio::runtime::Handle,
    ) -> Self {
        let subscriptions: Arc<Mutex<HashMap<SubscriptionId, Subscription>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let subscriptions_clone = subscriptions.clone();
        runtime_handle.spawn(async move {
            loop {
                match progress_rx.recv().await {
                    Some(progress) => {
                        let mut subs = subscriptions_clone.lock().unwrap();
                        let mut to_remove = Vec::new();
                        for (id, subscription) in subs.iter() {
                            if subscription.filter.matches(&progress)
                                && subscription.tx.send(progress.clone()).is_err()
                            {
                                to_remove.push(*id);
                            }
                        }
                        for id in to_remove {
                            subs.remove(&id);
                        }
                    }
                    None => {
                        info!("Channel closed, exiting");
                        break;
                    }
                }
            }
        });
        Self {
            subscriptions,
            next_id: Arc::new(AtomicU64::new(1)),
        }
    }
    /// Subscribe to progress updates for a specific release
    /// Returns a receiver that yields only progress updates for the specified release
    /// Subscription is automatically removed when receiver is dropped
    pub fn subscribe_release(
        &self,
        release_id: String,
    ) -> tokio_mpsc::UnboundedReceiver<ImportProgress> {
        let (tx, rx) = tokio_mpsc::unbounded_channel();
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let subscription = Subscription {
            filter: SubscriptionFilter::Release { release_id },
            tx,
        };
        self.subscriptions.lock().unwrap().insert(id, subscription);
        rx
    }
    /// Subscribe to progress updates for a specific track
    /// Returns a receiver that yields only progress updates for the specified track
    /// Subscription is automatically removed when receiver is dropped
    pub fn subscribe_track(
        &self,
        track_id: String,
    ) -> tokio_mpsc::UnboundedReceiver<ImportProgress> {
        let (tx, rx) = tokio_mpsc::unbounded_channel();
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let subscription = Subscription {
            filter: SubscriptionFilter::Track { track_id },
            tx,
        };
        self.subscriptions.lock().unwrap().insert(id, subscription);
        rx
    }
    /// Subscribe to progress updates for a specific import operation
    /// Returns a receiver that yields Preparing events and any event with matching import_id
    /// Subscription is automatically removed when receiver is dropped
    pub fn subscribe_import(
        &self,
        import_id: String,
    ) -> tokio_mpsc::UnboundedReceiver<ImportProgress> {
        let (tx, rx) = tokio_mpsc::unbounded_channel();
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let subscription = Subscription {
            filter: SubscriptionFilter::Import { import_id },
            tx,
        };
        self.subscriptions.lock().unwrap().insert(id, subscription);
        rx
    }
    /// Subscribe to progress updates for ALL import operations
    /// Returns a receiver that yields any event with an import_id (for toolbar dropdown)
    /// Subscription is automatically removed when receiver is dropped
    pub fn subscribe_all_imports(&self) -> tokio_mpsc::UnboundedReceiver<ImportProgress> {
        let (tx, rx) = tokio_mpsc::unbounded_channel();
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let subscription = Subscription {
            filter: SubscriptionFilter::AllImports,
            tx,
        };
        self.subscriptions.lock().unwrap().insert(id, subscription);
        rx
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
            import_id: None,
        },),);
        assert!(filter.matches(&ImportProgress::Progress {
            id: "release-1".to_string(),
            percent: 50,
            phase: Some(ImportPhase::Store),
            import_id: None,
        },),);
        assert!(filter.matches(&ImportProgress::Complete {
            id: "release-1".to_string(),
            release_id: None,
            cover_image_id: None,
            import_id: None,
        },),);
        assert!(!filter.matches(&ImportProgress::Progress {
            id: "release-2".to_string(),
            percent: 50,
            phase: Some(ImportPhase::Store),
            import_id: None,
        },),);
        assert!(!filter.matches(&ImportProgress::Preparing {
            import_id: "import-1".to_string(),
            step: PrepareStep::ParsingMetadata,
            album_title: "Test".to_string(),
            artist_name: "Artist".to_string(),
            cover_art_url: None,
        },),);
    }
    #[test]
    fn test_release_filter_matches_track_completion_for_release() {
        let filter = SubscriptionFilter::Release {
            release_id: "release-1".to_string(),
        };
        assert!(filter.matches(&ImportProgress::Complete {
            id: "track-1".to_string(),
            release_id: Some("release-1".to_string()),
            cover_image_id: None,
            import_id: None,
        },),);
        assert!(!filter.matches(&ImportProgress::Complete {
            id: "track-1".to_string(),
            release_id: Some("release-2".to_string()),
            cover_image_id: None,
            import_id: None,
        },),);
    }
    #[test]
    fn test_track_filter_matches_track_events() {
        let filter = SubscriptionFilter::Track {
            track_id: "track-1".to_string(),
        };
        assert!(filter.matches(&ImportProgress::Progress {
            id: "track-1".to_string(),
            percent: 75,
            phase: Some(ImportPhase::Store),
            import_id: None,
        },),);
        assert!(filter.matches(&ImportProgress::Complete {
            id: "track-1".to_string(),
            release_id: Some("release-1".to_string()),
            cover_image_id: None,
            import_id: None,
        },),);
        assert!(!filter.matches(&ImportProgress::Progress {
            id: "track-2".to_string(),
            percent: 50,
            phase: Some(ImportPhase::Store),
            import_id: None,
        },),);
        assert!(!filter.matches(&ImportProgress::Preparing {
            import_id: "import-1".to_string(),
            step: PrepareStep::ParsingMetadata,
            album_title: "Test".to_string(),
            artist_name: "Artist".to_string(),
            cover_art_url: None,
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
            cover_art_url: None,
        },),);
        assert!(filter.matches(&ImportProgress::Preparing {
            import_id: "import-1".to_string(),
            step: PrepareStep::DownloadingCoverArt,
            album_title: "Test".to_string(),
            artist_name: "Artist".to_string(),
            cover_art_url: None,
        },),);
        assert!(!filter.matches(&ImportProgress::Preparing {
            import_id: "import-2".to_string(),
            step: PrepareStep::ParsingMetadata,
            album_title: "Test".to_string(),
            artist_name: "Artist".to_string(),
            cover_art_url: None,
        },),);
    }
    #[test]
    fn test_import_filter_matches_events_with_import_id() {
        let filter = SubscriptionFilter::Import {
            import_id: "import-1".to_string(),
        };
        assert!(filter.matches(&ImportProgress::Started {
            id: "release-1".to_string(),
            import_id: Some("import-1".to_string()),
        },),);
        assert!(filter.matches(&ImportProgress::Progress {
            id: "release-1".to_string(),
            percent: 50,
            phase: Some(ImportPhase::Store),
            import_id: Some("import-1".to_string()),
        },),);
        assert!(filter.matches(&ImportProgress::Complete {
            id: "release-1".to_string(),
            release_id: None,
            cover_image_id: None,
            import_id: Some("import-1".to_string()),
        },),);
        assert!(filter.matches(&ImportProgress::Failed {
            id: "release-1".to_string(),
            error: "error".to_string(),
            import_id: Some("import-1".to_string()),
        },),);
        assert!(!filter.matches(&ImportProgress::Progress {
            id: "release-1".to_string(),
            percent: 50,
            phase: Some(ImportPhase::Store),
            import_id: Some("import-2".to_string()),
        },),);
        assert!(!filter.matches(&ImportProgress::Progress {
            id: "release-1".to_string(),
            percent: 50,
            phase: Some(ImportPhase::Store),
            import_id: None,
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
            cover_art_url: None,
        },),);
        assert!(filter.matches(&ImportProgress::Preparing {
            import_id: "import-2".to_string(),
            step: PrepareStep::DownloadingCoverArt,
            album_title: "Test".to_string(),
            artist_name: "Artist".to_string(),
            cover_art_url: None,
        },),);
        assert!(filter.matches(&ImportProgress::Started {
            id: "release-1".to_string(),
            import_id: Some("import-1".to_string()),
        },),);
        assert!(filter.matches(&ImportProgress::Progress {
            id: "release-1".to_string(),
            percent: 50,
            phase: Some(ImportPhase::Store),
            import_id: Some("import-2".to_string()),
        },),);
        assert!(filter.matches(&ImportProgress::Complete {
            id: "release-1".to_string(),
            release_id: None,
            cover_image_id: None,
            import_id: Some("import-3".to_string()),
        },),);
        assert!(filter.matches(&ImportProgress::Failed {
            id: "release-1".to_string(),
            error: "error".to_string(),
            import_id: Some("import-4".to_string()),
        },),);
        assert!(!filter.matches(&ImportProgress::Started {
            id: "release-1".to_string(),
            import_id: None,
        },),);
        assert!(!filter.matches(&ImportProgress::Progress {
            id: "release-1".to_string(),
            percent: 50,
            phase: Some(ImportPhase::Store),
            import_id: None,
        },),);
        assert!(!filter.matches(&ImportProgress::Complete {
            id: "release-1".to_string(),
            release_id: None,
            cover_image_id: None,
            import_id: None,
        },),);
    }
    #[test]
    fn test_all_prepare_steps_exist() {
        let steps = [
            PrepareStep::ParsingMetadata,
            PrepareStep::DownloadingCoverArt,
            PrepareStep::DiscoveringFiles,
            PrepareStep::ValidatingTracks,
            PrepareStep::SavingToDatabase,
            PrepareStep::ExtractingDurations,
        ];
        for step in steps {
            let event = ImportProgress::Preparing {
                import_id: "test".to_string(),
                step,
                album_title: "Test".to_string(),
                artist_name: "Artist".to_string(),
                cover_art_url: None,
            };
            let filter = SubscriptionFilter::Import {
                import_id: "test".to_string(),
            };
            assert!(filter.matches(&event));
        }
    }
}
