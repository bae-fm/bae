//! The import surface of [`AppServices`]: identification triggers, the
//! import event bus, and the candidate/triage subscriptions.

use super::*;

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

    /// The candidate list: the latest value of its live query, and every
    /// later one. Each value already carries every candidate's resumed
    /// identify state; a consumer pairs it with
    /// [`Self::subscribe_candidate_runtime`] for what is in flight.
    pub fn subscribe_import_candidates(
        &self,
    ) -> tokio::sync::watch::Receiver<crate::import::ImportCandidatesValue> {
        self.inner.import.subscribe_import_candidates()
    }

    /// Every candidate's runtime right now.
    pub fn candidate_runtimes(
        &self,
    ) -> std::collections::HashMap<String, crate::import::CandidateRuntimeSnapshot> {
        self.inner.import.candidate_runtimes()
    }

    /// Every candidate's runtime right now, and one change per key as runs
    /// advance.
    pub fn subscribe_candidate_runtime(
        &self,
    ) -> (
        std::collections::HashMap<String, crate::import::CandidateRuntimeSnapshot>,
        tokio::sync::broadcast::Receiver<crate::import::CandidateRuntimeChange>,
    ) {
        self.inner.import.subscribe_candidate_runtime()
    }

    /// The triage queue, re-projected when the candidate list changes and
    /// when a candidate's runtime changes in a way a row's placement reads —
    /// a run reaching a phase, an import claimed or finished. A progress tick
    /// changes nothing a row shows, so it re-projects nothing.
    pub fn subscribe_import_triage_values(
        &self,
        runtime_handle: &tokio::runtime::Handle,
    ) -> tokio::sync::mpsc::UnboundedReceiver<
        Result<crate::import::TriageQueue, crate::library::LibraryError>,
    > {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let mut candidates = self.inner.import.subscribe_import_candidates();
        let (initial_runtime, mut changes) = self.subscribe_candidate_runtime();
        let import = self.inner.import.clone();
        runtime_handle.spawn(async move {
            let mut runtime = initial_runtime;
            let mut facts: std::collections::HashMap<String, crate::import::triage::TriageRuntimeFacts> =
                runtime
                    .iter()
                    .map(|(key, runtime)| {
                        (
                            key.clone(),
                            crate::import::triage::TriageRuntimeFacts::of(runtime),
                        )
                    })
                    .collect();
            let project = |value: &crate::import::ImportCandidatesValue,
                           runtime: &std::collections::HashMap<
                String,
                crate::import::CandidateRuntimeSnapshot,
            >| match value.as_ref() {
                Ok(projection) => crate::import::triage::project_live(projection, runtime),
                Err(error) => Err(crate::library::LibraryError::Internal(format!(
                    "the import candidate list is unavailable: {error}"
                ))),
            };
            let first = candidates.borrow_and_update().clone();
            if tx.send(project(&first, &runtime)).is_err() {
                return;
            }
            loop {
                tokio::select! {
                    changed = candidates.changed() => {
                        if changed.is_err() {
                            return;
                        }
                        let value = candidates.borrow_and_update().clone();
                        if tx.send(project(&value, &runtime)).is_err() {
                            return;
                        }
                    }
                    change = changes.recv() => {
                        let placement_changed = match change {
                            Ok(crate::import::CandidateRuntimeChange::Updated { key, runtime: updated }) => {
                                let next = crate::import::triage::TriageRuntimeFacts::of(&updated);
                                let changed = facts.get(&key) != Some(&next);
                                facts.insert(key.clone(), next);
                                runtime.insert(key, updated);
                                changed
                            }
                            Ok(crate::import::CandidateRuntimeChange::Removed { key }) => {
                                runtime.remove(&key);
                                facts.remove(&key).is_some()
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                                tracing::warn!("triage projection dropped {count} runtime changes; re-reading every candidate's runtime");
                                runtime = import.candidate_runtimes();
                                facts = runtime
                                    .iter()
                                    .map(|(key, runtime)| {
                                        (key.clone(), crate::import::triage::TriageRuntimeFacts::of(runtime))
                                    })
                                    .collect();
                                true
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                        };
                        if placement_changed {
                            let value = candidates.borrow().clone();
                            if tx.send(project(&value, &runtime)).is_err() {
                                return;
                            }
                        }
                    }
                }
            }
        });
        rx
    }
}
