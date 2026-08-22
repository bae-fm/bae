//! The import surface of [`AppServices`]: identification triggers, the
//! import event bus, and the candidate/triage subscriptions.

use super::*;

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl AppServices {
    pub(crate) fn subscribe_import_events(
        &self,
    ) -> tokio::sync::broadcast::Receiver<crate::import::ImportEvent> {
        self.inner.import.subscribe_events()
    }

    /// Identify a folder candidate for a person who is looking at it: the run
    /// goes out at `Interactive`, and its verdict is persisted like the sweep's
    /// own.
    ///
    /// Re-identifying a library release is deliberately *not* routed through
    /// here: it has no candidate folder, so there is nothing to key a stored
    /// verdict by.
    pub fn identify_folder_candidate(&self, candidate_key: String) {
        self.inner.sweep.identify_for_selection(candidate_key);
    }

    /// Re-run a candidate's identification from the toolbar. Dispatches on
    /// where the run lives: a live driver re-combines from its retained
    /// signals; a candidate showing a resumed verdict has no driver, so a
    /// fresh interactive run replaces the stored answer. Re-identify keys
    /// always have a live driver while their sheet is open, so they take the
    /// first arm.
    pub fn rerun_identify(&self, candidate_key: String) {
        if self.inner.identify.is_running(&candidate_key) {
            self.inner.identify.rerun(&candidate_key);
        } else {
            self.inner.sweep.rerun_for_selection(candidate_key);
        }
    }

    /// The import candidate stream with each candidate's identify state made
    /// whole: a candidate with a run in flight carries that run's state, and
    /// a candidate with none carries the state its stored verdict stands back
    /// up as, read from the same database projection the triage rows use. A
    /// consumer never has to ask for an answer a second time — an answered
    /// candidate arrives answered, on every launch and through every rescan.
    ///
    /// Emissions pair a snapshot with a projection read subscribed *after*
    /// that snapshot existed, so a verdict whose write preceded the snapshot
    /// is always visible to the overlay — there is no interval in which a
    /// stored answer reads as absent.
    pub fn subscribe_import_candidates(
        &self,
        runtime_handle: &tokio::runtime::Handle,
    ) -> tokio::sync::watch::Receiver<crate::import::ImportCandidatesSnapshot> {
        let services = self.clone();
        let mut raw = self.inner.import.subscribe_import_candidates();
        // The initial value is the raw snapshot: the overlay needs a database
        // read, and the first projection emission below replaces this
        // immediately.
        let (tx, rx) = tokio::sync::watch::channel(raw.borrow().clone());
        runtime_handle.spawn(async move {
            let mut snapshot = raw.borrow_and_update().clone();
            let mut query = services
                .inner
                .manager
                .subscribe_import_triage(snapshot.clone());
            loop {
                tokio::select! {
                    result = query.next() => {
                        match result {
                            Ok(projection) => {
                                let mut merged = snapshot.clone();
                                match crate::import::triage::overlay_stored_verdicts(
                                    &mut merged,
                                    &projection,
                                ) {
                                    Ok(()) => {
                                        tx.send_replace(merged);
                                    }
                                    Err(error) => tracing::warn!(
                                        %error,
                                        "overlaying stored verdicts failed; \
                                         keeping the previous candidate snapshot"
                                    ),
                                }
                            }
                            Err(error) => tracing::warn!(
                                %error,
                                "import candidate projection read failed; \
                                 keeping the previous candidate snapshot"
                            ),
                        }
                    }
                    changed = raw.changed() => {
                        if changed.is_err() {
                            return;
                        }
                        snapshot = raw.borrow_and_update().clone();
                        query = services
                            .inner
                            .manager
                            .subscribe_import_triage(snapshot.clone());
                    }
                    _ = tx.closed() => return,
                }
            }
        });
        rx
    }

    pub fn subscribe_import_triage_values(
        &self,
        runtime_handle: &tokio::runtime::Handle,
    ) -> tokio::sync::mpsc::UnboundedReceiver<
        Result<crate::import::TriageQueue, crate::library::LibraryError>,
    > {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let services = self.clone();
        // The raw stream, not the overlaid one: `project_live` derives every
        // answered row from the stored verdicts itself, so feeding it resumed
        // states would only have it re-read what it already reads.
        let mut candidates = services.inner.import.subscribe_import_candidates();
        runtime_handle.spawn(async move {
            let mut snapshot = candidates.borrow().clone();
            let mut query = services
                .inner
                .manager
                .subscribe_import_triage(snapshot.clone());
            loop {
                tokio::select! {
                    result = query.next() => {
                        let value = match result {
                            Ok(projection) => services.inner.manager.resolve_import_triage(snapshot.clone(), projection),
                            Err(error) => Err(crate::library::LibraryError::Database(match error {
                                coven::CovenError::Database(error) => *error,
                                other => coven::DbError::Message(other.to_string()),
                            })),
                        };
                        if tx.send(value).is_err() { return; }
                    }
                    changed = candidates.changed() => {
                        if changed.is_err() { return; }
                        snapshot = candidates.borrow_and_update().clone();
                        query = services.inner.manager.subscribe_import_triage(snapshot.clone());
                    }
                }
            }
        });
        rx
    }
}
