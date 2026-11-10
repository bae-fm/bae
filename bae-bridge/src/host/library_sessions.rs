//! The libraries a host has open, and the stores still closing. An open
//! library leaves the record and its close is recorded in one step, so a call
//! that names a store finds the open library exactly while its session is
//! open, and from then on the close to wait for until the store is closed. A
//! call that found the open library holds its session open: the close runs
//! once every such call has ended.

use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};

/// What answers for an open library's standing with its storage, from its
/// live sync status.
pub(crate) trait OpenStanding: Send + Sync {
    fn standing(
        self: Arc<Self>,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<crate::BridgeLibraryStanding, crate::BridgeError>,
                > + Send,
        >,
    >;
}

/// An open library as its host's record holds it, to answer for its
/// standing: its services, and the runtime its calls run on.
pub(crate) struct OpenLibrary {
    services: bae_app::AppServices,
    runtime: crate::operation_runtime::OperationRuntime,
}

impl OpenLibrary {
    pub(crate) fn new(
        services: bae_app::AppServices,
        runtime: crate::operation_runtime::OperationRuntime,
    ) -> Self {
        Self { services, runtime }
    }
}

impl OpenStanding for OpenLibrary {
    fn standing(
        self: Arc<Self>,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<crate::BridgeLibraryStanding, crate::BridgeError>,
                > + Send,
        >,
    > {
        let runtime = self.runtime.clone();
        Box::pin(async move {
            runtime
                .run("AppHandle.standing", move || async move {
                    Ok(crate::BridgeLibraryStanding::from_core(self.services.standing().await?))
                })
                .await
        })
    }
}

/// Each library open on this host, by its store, and each close of a store
/// under way, and each call running on an open library. A session's library
/// is in the record from its open until it begins to close; its close is
/// recorded from then until it ends, and runs once the calls that found the
/// library have ended. A store whose `OpenedLibrary` was released unopened
/// closes the same way, with no library in the record first.
///
/// Nothing done while the lock is held can panic — a push, a retain, a scan —
/// so a poisoned lock says nothing about the record, and is not a panic for a
/// released handle's `Drop`, which ends a close, to raise.
pub(crate) struct LibrarySessions {
    sessions: Mutex<Sessions>,
    /// Told each time a close or a call on an open library ends, and, in a
    /// test, each time a wait for one begins.
    changed: Condvar,
    /// The next standing call a test holds once it has found the open
    /// library, until the test lets it go on.
    #[cfg(test)]
    held_call: Mutex<Option<HeldCall>>,
}

/// A standing call a test holds: `reached` hears when the call has found the
/// open library, and the call goes on once `release` hears from the test.
#[cfg(test)]
struct HeldCall {
    reached: std::sync::mpsc::Sender<()>,
    release: tokio::sync::oneshot::Receiver<()>,
}

/// What the record holds, behind its lock: each entry under the id its
/// record or close was given.
struct Sessions {
    open: Vec<(coven::StoreId, u64, Arc<dyn OpenStanding>)>,
    closing: Vec<(coven::StoreId, u64)>,
    /// The record id of the library each running call found, once per call.
    calls: Vec<u64>,
    next: u64,
    /// The store of each call that has waited for a close, so a test can hold
    /// the close until one does.
    #[cfg(test)]
    waited: Vec<coven::StoreId>,
    /// The record id of each close that has waited for its library's calls,
    /// so a test can hold a call until a close does.
    #[cfg(test)]
    closes_waited: Vec<u64>,
}

impl LibrarySessions {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            sessions: Mutex::new(Sessions {
                open: Vec::new(),
                closing: Vec::new(),
                calls: Vec::new(),
                next: 0,
                #[cfg(test)]
                waited: Vec::new(),
                #[cfg(test)]
                closes_waited: Vec::new(),
            }),
            changed: Condvar::new(),
            #[cfg(test)]
            held_call: Mutex::new(None),
        })
    }

    /// Record `library`, open on `store`. It stays in the record while its
    /// session holds what this returns, until the session closes it or lets
    /// it go.
    pub(crate) fn open(
        self: &Arc<Self>,
        store: coven::StoreId,
        library: Arc<dyn OpenStanding>,
    ) -> OpenRecord {
        let mut sessions = self.lock();
        let id = sessions.id();
        sessions.open.push((store, id, library));
        OpenRecord { sessions: self.clone(), store, id }
    }

    /// Record a close of `store`, which has no library open here, under way
    /// until what this returns is dropped.
    pub(crate) fn closing(self: &Arc<Self>, store: coven::StoreId) -> CloseUnderWay {
        let mut sessions = self.lock();
        let id = sessions.id();
        sessions.closing.push((store, id));
        CloseUnderWay { sessions: self.clone(), id }
    }

    /// Take `record`'s library out of the record and record its close, in one
    /// step under the lock, with the calls already running on the library for
    /// the close to wait for.
    fn close_open(self: &Arc<Self>, record: &OpenRecord) -> (CloseUnderWay, OpenCalls) {
        let mut sessions = self.lock();
        sessions.open.retain(|(_, open, _)| *open != record.id);
        let id = sessions.id();
        sessions.closing.push((record.store, id));
        (
            CloseUnderWay { sessions: self.clone(), id },
            OpenCalls { sessions: self.clone(), library: record.id },
        )
    }

    /// Ask the library open on `store`, if this host has one open, how it
    /// stands; `None` when none is.
    pub(crate) async fn open_standing(
        &self,
        store: coven::StoreId,
    ) -> Option<Result<crate::BridgeLibraryStanding, crate::BridgeError>> {
        let (library, _call) = {
            let mut sessions = self.lock();
            let (id, library) = sessions
                .open
                .iter()
                .find(|(open, _, _)| *open == store)
                .map(|(_, id, library)| (*id, library.clone()))?;
            sessions.calls.push(id);
            (library, RunningCall { sessions: self, library: id })
        };
        #[cfg(test)]
        self.hold_if_asked().await;
        Some(library.standing().await)
    }

    /// Hold the next standing call once it has found the open library: the
    /// receiver hears when it has, and the call goes on once the sender sends.
    #[cfg(test)]
    pub(crate) fn hold_next_call(
        &self,
    ) -> (std::sync::mpsc::Receiver<()>, tokio::sync::oneshot::Sender<()>) {
        let (reached, reaching) = std::sync::mpsc::channel();
        let (releasing, release) = tokio::sync::oneshot::channel();
        *self.held_call.lock().unwrap() = Some(HeldCall { reached, release });
        (reaching, releasing)
    }

    #[cfg(test)]
    async fn hold_if_asked(&self) {
        let held = self.held_call.lock().unwrap().take();
        if let Some(held) = held {
            held.reached.send(()).expect("the test waits for the held call");
            held.release.await.expect("the test lets the call go on");
        }
    }

    /// Wait until a close waits for the calls running on its library,
    /// failing the test if none ever does.
    #[cfg(test)]
    pub(crate) fn wait_for_close_waiting_on_calls(&self) {
        let sessions = self.lock();
        let (_sessions, waited) = self
            .changed
            .wait_timeout_while(sessions, bae_util::test_channels::HANG_GUARD, |sessions| {
                sessions.closes_waited.is_empty()
            })
            .unwrap();
        assert!(!waited.timed_out(), "no close ever waited for the library's calls");
    }

    /// Wait until every close of `store` under way now has ended. Blocks, so
    /// it runs off any async runtime. A close that failed was reported where
    /// it ran; it is not this call's failure.
    pub(crate) fn wait(&self, store: coven::StoreId) {
        let mut sessions = self.lock();
        let under_way: Vec<u64> = sessions
            .closing
            .iter()
            .filter(|(closing, _)| *closing == store)
            .map(|(_, id)| *id)
            .collect();
        #[cfg(test)]
        {
            sessions.waited.push(store);
            self.changed.notify_all();
        }
        while sessions.closing.iter().any(|(_, id)| under_way.contains(id)) {
            sessions = self.changed.wait(sessions).unwrap_or_else(PoisonError::into_inner);
        }
    }

    /// Wait until a call waits for a close of `store`, failing the test if
    /// none ever does or no close of it is under way.
    #[cfg(test)]
    pub(crate) fn wait_for_waiter(&self, store: coven::StoreId) {
        let sessions = self.lock();
        assert!(
            sessions.closing.iter().any(|(closing, _)| *closing == store),
            "no close of the store is under way"
        );
        let (_sessions, waited) = self
            .changed
            .wait_timeout_while(sessions, bae_util::test_channels::HANG_GUARD, |sessions| {
                !sessions.waited.contains(&store)
            })
            .unwrap();
        assert!(!waited.timed_out(), "no call ever waited for the close");
    }

    fn lock(&self) -> MutexGuard<'_, Sessions> {
        self.sessions.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Sessions {
    fn id(&mut self) -> u64 {
        self.next += 1;
        self.next
    }
}

/// An open library's place in its host's record, held by its session while
/// the library is open. Dropping it takes the library out of the record.
pub(crate) struct OpenRecord {
    sessions: Arc<LibrarySessions>,
    store: coven::StoreId,
    id: u64,
}

impl OpenRecord {
    /// The session begins to close: its library leaves the record and its
    /// close is recorded in the same step, so a call that names the store
    /// finds one or the other. The close is under way until the returned
    /// close is dropped, and runs once the returned calls, those that found
    /// the library before it left, have ended.
    pub(crate) fn close(self) -> (CloseUnderWay, OpenCalls) {
        self.sessions.close_open(&self)
    }
}

impl Drop for OpenRecord {
    fn drop(&mut self) {
        self.sessions.lock().open.retain(|(_, open, _)| *open != self.id);
    }
}

/// A close of a store under way. Dropping it, as the close ends, takes it out
/// of the record and lets every call waiting for it go on.
pub(crate) struct CloseUnderWay {
    sessions: Arc<LibrarySessions>,
    id: u64,
}

impl Drop for CloseUnderWay {
    fn drop(&mut self) {
        self.sessions.lock().closing.retain(|(_, closing)| *closing != self.id);
        self.sessions.changed.notify_all();
    }
}

/// One call running on an open library, from when it found the library in
/// the record until it ends: it holds the library's session open.
struct RunningCall<'a> {
    sessions: &'a LibrarySessions,
    library: u64,
}

impl Drop for RunningCall<'_> {
    fn drop(&mut self) {
        let mut sessions = self.sessions.lock();
        if let Some(at) = sessions.calls.iter().position(|call| *call == self.library) {
            sessions.calls.swap_remove(at);
        }
        drop(sessions);
        self.sessions.changed.notify_all();
    }
}

/// The calls running on a library that has left the record, which its close
/// waits for before it runs.
pub(crate) struct OpenCalls {
    sessions: Arc<LibrarySessions>,
    library: u64,
}

impl OpenCalls {
    /// Wait until every call that found the library has ended. Blocks, so it
    /// runs where the close runs, off the library's runtime.
    pub(crate) fn wait(self) {
        let mut sessions = self.sessions.lock();
        #[cfg(test)]
        {
            sessions.closes_waited.push(self.library);
            self.sessions.changed.notify_all();
        }
        while sessions.calls.contains(&self.library) {
            sessions = self.sessions.changed.wait(sessions).unwrap_or_else(PoisonError::into_inner);
        }
    }
}
