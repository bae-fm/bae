use super::*;

impl ImportServiceHandle {
    /// Mark the candidate at `path` skipped or unskipped, persisting the change
    /// and broadcasting it so the import view re-tabs the row (New ↔ Skipped).
    /// A no-op request (already in the requested state) persists nothing and
    /// emits no event.
    pub fn set_candidate_skipped(
        &self,
        path: String,
        skipped: bool,
    ) -> Result<(), crate::import::ImportError> {
        let library_dir = self.library_manager.library_dir();
        let mut registry = self.folder_registry.lock().unwrap();
        let changed = registry.set_skipped(&library_dir, path.clone(), skipped)?;
        drop(registry);
        if changed {
            self.candidate_state
                .lock()
                .unwrap()
                .set_skipped(&path, skipped);
            send_event(
                &self.event_tx,
                ImportEvent::Scan(ScanEvent::CandidateSkipChanged {
                    candidate_key: path,
                    skipped,
                }),
            );
        }
        Ok(())
    }

    /// Subscribe to scan events, filtered from the unified event channel.
    /// Returns an mpsc receiver that yields only ScanEvent variants.
    pub fn subscribe_folder_scan_events(&self) -> mpsc::UnboundedReceiver<ScanEvent> {
        let mut rx = self.event_tx.subscribe();
        let (tx, out_rx) = mpsc::unbounded_channel();
        self.runtime_handle.spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        if tx.is_closed() {
                            break;
                        }
                        if let ImportEvent::Scan(event) = event {
                            if tx.send(event).is_err() {
                                break;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("Scan event subscriber lagged by {n} events");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        out_rx
    }
}
