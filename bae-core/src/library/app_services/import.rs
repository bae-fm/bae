//! The import surface of [`AppServices`]: identification triggers, the import
//! event bus, and the list and per-candidate subscriptions.

use super::*;
use crate::import::{
    ImportCandidateDetail, ImportCandidateDetailProjection, ImportListProjection,
    ImportListRequest, ImportListSubscription, ImportListView, TriageRuntimeFacts,
};

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

    /// Every key with something in flight right now.
    pub fn candidate_runtimes(
        &self,
    ) -> std::collections::HashMap<String, crate::import::CandidateRuntimeSnapshot> {
        self.inner.import.candidate_runtimes()
    }

    /// What is in flight for one key — the read a view does once when it
    /// appears, after it has subscribed to the changes.
    pub fn candidate_runtime(&self, key: &str) -> Option<crate::import::CandidateRuntimeSnapshot> {
        self.inner.import.candidate_runtime(key)
    }

    /// Every key with something in flight right now, and one change per key as
    /// runs advance.
    pub fn subscribe_candidate_runtime(
        &self,
    ) -> (
        std::collections::HashMap<String, crate::import::CandidateRuntimeSnapshot>,
        tokio::sync::broadcast::Receiver<crate::import::CandidateRuntimeChange>,
    ) {
        self.inner.import.subscribe_candidate_runtime()
    }

    /// The import list, reconfigurable by view and by window.
    ///
    /// The runtime stream is taken before the first read, so no change lands
    /// between the two; the subscription then keeps the request's runtime
    /// facts current on its own.
    pub fn subscribe_import_list(
        &self,
        view: ImportListView,
        runtime_handle: &tokio::runtime::Handle,
    ) -> ImportListSubscription {
        let (initial_runtime, changes) = self.subscribe_candidate_runtime();
        let request = ImportListRequest {
            view,
            windows: crate::library::LibraryPageWindows::new(),
            runtime_facts: crate::import::list::facts_of(&initial_runtime),
        };
        let query = self.inner.manager.subscribe_import_list(request.clone());
        let import = self.inner.import.clone();
        ImportListSubscription::start(
            query,
            request,
            changes,
            move || import.candidate_runtimes(),
            runtime_handle,
        )
    }

    /// One read of the list, for a caller with no subscription.
    pub async fn load_import_list(
        &self,
        view: ImportListView,
        windows: crate::library::LibraryPageWindows,
    ) -> Result<ImportListProjection, crate::library::LibraryError> {
        self.inner
            .manager
            .load_import_list(ImportListRequest {
                view,
                windows,
                runtime_facts: crate::import::list::facts_of(&self.candidate_runtimes()),
            })
            .await
    }

    /// One candidate as the pane reads it, once, with its runtime folded in.
    pub async fn load_import_candidate(
        &self,
        key: &str,
    ) -> Result<Option<ImportCandidateDetail>, crate::library::LibraryError> {
        let runtime = self.candidate_runtimes().remove(key);
        let facts = runtime
            .as_ref()
            .map(TriageRuntimeFacts::of)
            .unwrap_or_default();
        let identify = runtime
            .and_then(|runtime| runtime.identify)
            .unwrap_or(crate::identify::IdentifyState::Idle);
        Ok(self
            .inner
            .manager
            .load_import_candidate(key)
            .await?
            .map(|projection| projection.resolve(&facts, &identify)))
    }

    /// One candidate as the pane reads it, and every later read of it. `None`
    /// once the key names no scanned folder, which is what clears a selection.
    pub fn subscribe_import_candidate_values(
        &self,
        runtime_handle: &tokio::runtime::Handle,
        key: String,
    ) -> tokio::sync::mpsc::UnboundedReceiver<
        Result<Option<ImportCandidateDetail>, crate::library::LibraryError>,
    > {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let (initial_runtime, mut changes) = self.subscribe_candidate_runtime();
        let mut query = self.inner.manager.subscribe_import_candidate(&key);
        let import = self.inner.import.clone();
        runtime_handle.spawn(async move {
            let initial = initial_runtime.get(&key);
            let mut facts = initial.map(TriageRuntimeFacts::of).unwrap_or_default();
            // The whole identify state, not only the facts a row's placement
            // reads: the header's evidence badge names which signal turned the
            // picked release up, and a run in flight knows that before its
            // verdict is stored.
            let mut identify = initial
                .and_then(|runtime| runtime.identify.clone())
                .unwrap_or(crate::identify::IdentifyState::Idle);
            let mut projection: Option<ImportCandidateDetailProjection> = None;
            // The units this pane has already asked to be measured. The query
            // cannot open a file, so a projection that shows unmeasured rows
            // asks once and redraws when the write lands.
            let mut probing: Vec<crate::import::AudioFile> = Vec::new();
            let deliver = |projection: &Option<ImportCandidateDetailProjection>,
                           facts: &TriageRuntimeFacts,
                           identify: &crate::identify::IdentifyState| {
                projection
                    .clone()
                    .map(|projection| projection.resolve(facts, identify))
            };
            loop {
                tokio::select! {
                    value = query.next() => match value {
                        Ok(value) => {
                            projection = value;
                            if let Some(unprobed) = projection
                                .as_ref()
                                .map(|projection| &projection.unprobed)
                                .filter(|unprobed| !unprobed.is_empty() && **unprobed != probing)
                            {
                                probing = unprobed.clone();
                                let import = import.clone();
                                let key = key.clone();
                                let units = probing.clone();
                                tokio::spawn(async move {
                                    if let Err(error) =
                                        import.probe_candidate_durations(&key, units).await
                                    {
                                        tracing::warn!(
                                            "could not measure {key}'s audio: {error}"
                                        );
                                    }
                                });
                            }
                            if tx.send(Ok(deliver(&projection, &facts, &identify))).is_err() {
                                return;
                            }
                        }
                        Err(error) => {
                            let error = match error {
                                coven::CovenError::Database(error) => *error,
                                other => coven::DbError::Message(other.to_string()),
                            };
                            if tx
                                .send(Err(crate::library::LibraryError::Database(error)))
                                .is_err()
                            {
                                return;
                            }
                        }
                    },
                    change = changes.recv() => {
                        let next = match change {
                            Ok(crate::import::CandidateRuntimeChange::Updated {
                                key: changed,
                                runtime,
                            }) => {
                                if changed != key {
                                    continue;
                                }
                                Some(runtime)
                            }
                            Ok(crate::import::CandidateRuntimeChange::Removed { key: changed }) => {
                                if changed != key {
                                    continue;
                                }
                                None
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                                tracing::warn!(
                                    "the selected candidate dropped {count} runtime changes; \
                                     re-reading its runtime"
                                );
                                import.candidate_runtimes().remove(&key)
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                        };
                        let next_facts = next
                            .as_ref()
                            .map(TriageRuntimeFacts::of)
                            .unwrap_or_default();
                        let next_identify = next
                            .and_then(|runtime| runtime.identify)
                            .unwrap_or(crate::identify::IdentifyState::Idle);
                        if next_facts == facts && next_identify == identify {
                            continue;
                        }
                        facts = next_facts;
                        identify = next_identify;
                        if projection.is_some()
                            && tx.send(Ok(deliver(&projection, &facts, &identify))).is_err()
                        {
                            return;
                        }
                    }
                }
            }
        });
        rx
    }
}
