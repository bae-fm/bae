//! The storage transitions in flight, one per release, and the stream the
//! storage rows read them from.
//!
//! A release is in at most one transition at a time: an upload being
//! admitted, a pin or unpin the download worker drives, or a make-Local
//! transfer this process copies file by file. Whichever it is, it is
//! registered here as one entry that says what it is and, where the
//! transfer can be stopped from here, how — so cancelling dispatches on what
//! is recorded instead of trying each kind in turn.

use crate::album_detail::ReleaseStorageAction;
use crate::library::{CancellationToken, LibraryError};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// One release's transition.
struct InFlight {
    action: ReleaseStorageAction,
    /// How to stop a transfer this process drives between files. `None` for
    /// a transition stopped elsewhere: a download through its queue, an
    /// upload through coven's outbox.
    cancel: Option<CancellationToken>,
    /// Whether the action is on the value stream. A driven transfer is
    /// registered when it is set up, so its cancellation is reachable at
    /// once, and published when it reports that it started.
    published: bool,
}

#[derive(Clone)]
pub(crate) struct StorageTransitions {
    in_flight: Arc<Mutex<HashMap<String, InFlight>>>,
    values: tokio::sync::watch::Sender<HashMap<String, ReleaseStorageAction>>,
}

/// Holds a registration for as long as its transition runs. Dropping it —
/// on completion, or when the future driving the transfer is dropped by a
/// dismissed view — removes the entry and republishes, so a dropped transfer
/// never leaves a stale action or token behind.
pub(crate) struct TransitionGuard {
    transitions: StorageTransitions,
    release_ids: Vec<String>,
}

impl Drop for TransitionGuard {
    fn drop(&mut self) {
        let mut in_flight = self.transitions.in_flight.lock().unwrap();
        for release_id in &self.release_ids {
            in_flight.remove(release_id);
        }
        self.transitions.publish_locked(&in_flight);
    }
}

impl StorageTransitions {
    pub(crate) fn new() -> Self {
        let (values, _) = tokio::sync::watch::channel(HashMap::new());
        Self {
            in_flight: Arc::new(Mutex::new(HashMap::new())),
            values,
        }
    }

    /// Admit every release of a foreground command under one lock: either
    /// all of them become active on the value stream or none does. A release
    /// already active in another transition refuses the whole batch.
    pub(crate) fn admit(
        &self,
        release_ids: &[String],
        action: ReleaseStorageAction,
    ) -> Result<TransitionGuard, LibraryError> {
        let mut in_flight = self.in_flight.lock().unwrap();
        let mut selected = std::collections::HashSet::with_capacity(release_ids.len());
        for release_id in release_ids {
            if !selected.insert(release_id) {
                return Err(LibraryError::Validation(format!(
                    "release {release_id} appears more than once in the storage batch"
                )));
            }
            if in_flight
                .get(release_id)
                .is_some_and(|transition| transition.published)
            {
                return Err(LibraryError::Storage(format!(
                    "release {release_id} already has an active storage transition"
                )));
            }
        }
        for release_id in release_ids {
            in_flight.insert(
                release_id.clone(),
                InFlight {
                    action,
                    cancel: None,
                    published: true,
                },
            );
        }
        self.publish_locked(&in_flight);
        Ok(TransitionGuard {
            transitions: self.clone(),
            release_ids: release_ids.to_vec(),
        })
    }

    /// Register a transfer about to be driven, before it reports anything:
    /// its cancellation is reachable from here on, and its action reaches the
    /// value stream when [`Self::started`] says it began.
    pub(crate) fn track(
        &self,
        release_id: &str,
        action: ReleaseStorageAction,
        cancel: Option<CancellationToken>,
    ) -> TransitionGuard {
        self.in_flight.lock().unwrap().insert(
            release_id.to_string(),
            InFlight {
                action,
                cancel,
                published: false,
            },
        );
        TransitionGuard {
            transitions: self.clone(),
            release_ids: vec![release_id.to_string()],
        }
    }

    /// The driven transfer reported that it started: its action goes on the
    /// value stream.
    pub(crate) fn started(&self, release_id: &str, action: ReleaseStorageAction) {
        let mut in_flight = self.in_flight.lock().unwrap();
        match in_flight.get_mut(release_id) {
            Some(transition) => {
                transition.action = action;
                transition.published = true;
            }
            None => {
                in_flight.insert(
                    release_id.to_string(),
                    InFlight {
                        action,
                        cancel: None,
                        published: true,
                    },
                );
            }
        }
        self.publish_locked(&in_flight);
    }

    /// The action a release is in, as the storage rows show it.
    pub(crate) fn current(&self, release_id: &str) -> Option<ReleaseStorageAction> {
        self.in_flight
            .lock()
            .unwrap()
            .get(release_id)
            .filter(|transition| transition.published)
            .map(|transition| transition.action)
    }

    /// Stop what this process can stop of a release's transition, and say
    /// which kind it is so the caller can stop the rest. A transfer with a
    /// token is cancelled here; the lookup and the fire share one lock, so a
    /// deregistering guard cannot slip between them. `None` when nothing is
    /// registered.
    pub(crate) fn cancel(&self, release_id: &str) -> Option<ReleaseStorageAction> {
        let in_flight = self.in_flight.lock().unwrap();
        let transition = in_flight.get(release_id)?;
        if let Some(cancel) = &transition.cancel {
            cancel.cancel();
        }
        Some(transition.action)
    }

    pub(crate) fn subscribe(
        &self,
    ) -> tokio::sync::watch::Receiver<HashMap<String, ReleaseStorageAction>> {
        self.values.subscribe()
    }

    /// Send the stream a fresh copy of what is published, unchanged — load
    /// for a test that races readers against the sender.
    #[cfg(test)]
    pub(crate) fn republish(&self) {
        let in_flight = self.in_flight.lock().unwrap();
        self.publish_locked(&in_flight);
    }

    fn publish_locked(&self, in_flight: &HashMap<String, InFlight>) {
        self.values.send_replace(
            in_flight
                .iter()
                .filter(|(_, transition)| transition.published)
                .map(|(release_id, transition)| (release_id.clone(), transition.action))
                .collect(),
        );
    }
}
