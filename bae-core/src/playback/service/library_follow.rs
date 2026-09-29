//! A live library read the run loop keeps pointed at what the service is
//! playing, and wakes on when the library changes what it reads.
//!
//! The loop hands it its request every turn, like the side-pause countdown
//! wait: an unchanged request keeps the query, a new one re-points it. The
//! query runs on a task of its own, so the loop's `select!` dropping the
//! branch never throws away a read in flight; the loop's only branch is a
//! channel receive.

use tracing::error;

use crate::library::LibraryManager;
use crate::playback::{PlaybackTrackInfo, TrackDisplay};

/// The library reads the service keeps pointed at what it plays.
pub(super) struct ServiceFollows {
    /// The display of the track playing on a remote device, which the device
    /// is loaded with again when it changes.
    remote_display: LibraryFollow<Option<String>, Option<TrackDisplay>>,
    /// The sides of the staged crossing's tracks, whose crossing is taken back
    /// when an edit puts a side or disc boundary between them.
    staged_sides: LibraryFollow<Vec<String>, Vec<PlaybackTrackInfo>>,
}

/// A change the library made to something the service plays.
pub(super) enum LibraryChange {
    /// What the track playing on a remote device shows; `None` when the
    /// library no longer holds it.
    RemoteDisplay {
        track_id: String,
        display: Option<TrackDisplay>,
    },
    /// The sides of the staged crossing's tracks, leaving out one the library
    /// no longer holds.
    StagedSides(Vec<PlaybackTrackInfo>),
}

impl ServiceFollows {
    /// Follows that read nothing until the service plays.
    pub(super) fn new(library_manager: &LibraryManager) -> Self {
        Self {
            remote_display: LibraryFollow::new(
                library_manager.subscribe_track_display(None),
                None,
                "the remote track's display",
            ),
            staged_sides: LibraryFollow::new(
                library_manager.subscribe_playback_track_infos(Vec::new()),
                Vec::new(),
                "the staged crossing's sides",
            ),
        }
    }

    /// Point the reads at the track playing on a remote device and the
    /// staged crossing's tracks.
    pub(super) fn follow(&mut self, remote_track: Option<String>, staged_crossing: Vec<String>) {
        self.remote_display.follow(remote_track);
        self.staged_sides.follow(staged_crossing);
    }

    /// The next change to what the reads follow.
    pub(super) async fn next(&mut self) -> LibraryChange {
        loop {
            tokio::select! {
                (track_id, display) = self.remote_display.next() => {
                    if let Some(track_id) = track_id {
                        return LibraryChange::RemoteDisplay { track_id, display };
                    }
                }
                (_, sides) = self.staged_sides.next() => {
                    return LibraryChange::StagedSides(sides);
                }
            }
        }
    }
}

struct LibraryFollow<Request, Value> {
    requests: coven::LiveQueryRequests<Request>,
    request: Request,
    reads: tokio::sync::mpsc::UnboundedReceiver<(Request, Value)>,
    task: tokio::task::JoinHandle<()>,
}

impl<Request, Value> LibraryFollow<Request, Value>
where
    Request: Clone + PartialEq + Send + Sync + std::fmt::Debug + 'static,
    Value: Clone + PartialEq + Send + 'static,
{
    /// Follow `query` on a task of its own. A read that fails is logged under
    /// `what` and the follow keeps running: the next change reads again.
    fn new(
        mut query: coven::ReconfigurableLiveQuery<Request, Value>,
        request: Request,
        what: &'static str,
    ) -> Self {
        let requests = query.requests();
        let (tx, reads) = tokio::sync::mpsc::unbounded_channel();
        let task = tokio::spawn(async move {
            loop {
                let event = query.next().await;
                let request = event.request().clone();
                match event.into_result() {
                    Ok(value) => {
                        if tx.send((request, value)).is_err() {
                            return;
                        }
                    }
                    Err(error) => error!("reading {what} for {request:?} failed: {error}"),
                }
            }
        });
        Self {
            requests,
            request,
            reads,
            task,
        }
    }

    /// Point the read at `request`, unless it already reads it.
    fn follow(&mut self, request: Request) {
        if request != self.request {
            self.requests
                .set(request.clone())
                .expect("the follow task holds its query while this handle lives");
            self.request = request;
        }
    }

    /// The next value read for the current request. Reads that answer an
    /// earlier request are skipped.
    async fn next(&mut self) -> (Request, Value) {
        loop {
            let Some((request, value)) = self.reads.recv().await else {
                return std::future::pending().await;
            };
            if request == self.request {
                return (request, value);
            }
        }
    }
}

impl<Request, Value> Drop for LibraryFollow<Request, Value> {
    fn drop(&mut self) {
        self.task.abort();
    }
}
