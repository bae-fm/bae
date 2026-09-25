//! Gathering what a filesystem watch reports into what is read again.
//!
//! A folder directly under a watched root is the unit a root is read in, so
//! it is the unit changes are gathered in too: each such folder's events wait
//! until that folder has been quiet for [`QUIET`], and then go out together.
//! A copy into one album holds that album back until the copy pauses, and
//! holds back nothing beside it — a download client writing into one folder
//! for hours does not keep every other folder from being read.
//!
//! Every event is kept, including every event that says the watch lost track
//! of changes (FSEvents dropping events, inotify's queue overflowing). Such an
//! event names where to start reading again, and a gatherer that kept only the
//! latest of them would lose the others' folders.

use notify::Event;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// How long a folder has to go without a change before what changed in it is
/// read.
pub(super) const QUIET: Duration = Duration::from_secs(1);

/// What the watch reported: one folder's events once it went quiet, or the
/// errors the watch backend raised, which go out the moment they arrive.
pub(crate) type WatchReport = Result<Vec<Event>, Vec<notify::Error>>;

/// Where one event's path is read again from.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Gathering {
    /// One folder directly under a watched root.
    Folder { root: PathBuf, folder: String },
    /// A whole watched root: the root itself changed, or the watch lost track
    /// of changes somewhere that takes in all of it.
    Root(PathBuf),
    /// A path under no watched root — a watch left behind by a folder that is
    /// no longer watched. Passed on, and matched against nothing.
    Unrouted,
}

struct Pending {
    events: Vec<Event>,
    seen: HashSet<Event>,
    last: Instant,
}

/// The events gathered so far, by the folder they wait on.
pub(super) struct WatchBatches {
    pending: HashMap<Gathering, Pending>,
}

impl WatchBatches {
    pub(super) fn new() -> Self {
        Self {
            pending: HashMap::new(),
        }
    }

    /// Take in one event reported at `now` under `roots`. An event naming
    /// several paths — a rename's two ends — is filed once under each, as an
    /// event naming that path alone.
    ///
    /// An event that says the watch lost track, and names no path or a path
    /// that holds a root, is filed as a change to each root it takes in,
    /// naming that root: whatever reads it reads the root whole.
    pub(super) fn add(&mut self, event: Event, roots: &BTreeSet<PathBuf>, now: Instant) {
        let rescan = event.need_rescan();
        let single = |path: &Path| {
            let mut single = Event::new(event.kind).add_path(path.to_path_buf());
            if let Some(flag) = event.flag() {
                single = single.set_flag(flag);
            }
            single
        };
        if event.paths.is_empty() {
            if !rescan {
                return;
            }
            for root in roots {
                self.file(Gathering::Root(root.clone()), single(root), now);
            }
            return;
        }
        for path in &event.paths {
            if let Some(root) = roots.iter().find(|root| path.starts_with(root)) {
                let gathering = match path.strip_prefix(root).ok().and_then(|relative| {
                    relative
                        .components()
                        .next()
                        .map(|first| first.as_os_str().to_string_lossy().into_owned())
                }) {
                    Some(folder) => Gathering::Folder {
                        root: root.clone(),
                        folder,
                    },
                    None => Gathering::Root(root.clone()),
                };
                self.file(gathering, single(path), now);
                continue;
            }
            let held: Vec<&PathBuf> = roots.iter().filter(|root| root.starts_with(path)).collect();
            if rescan && !held.is_empty() {
                for root in held {
                    self.file(Gathering::Root(root.clone()), single(root), now);
                }
                continue;
            }
            self.file(Gathering::Unrouted, single(path), now);
        }
    }

    fn file(&mut self, gathering: Gathering, event: Event, now: Instant) {
        let pending = self.pending.entry(gathering).or_insert_with(|| Pending {
            events: Vec::new(),
            seen: HashSet::new(),
            last: now,
        });
        pending.last = now;
        if pending.seen.insert(event.clone()) {
            pending.events.push(event);
        }
    }

    /// Every gathering that has been quiet for [`QUIET`] by `now`, as the
    /// batches to read. A root read whole takes the folders under it with it:
    /// it reads them anyway.
    pub(super) fn take_quiet(&mut self, now: Instant) -> Vec<Vec<Event>> {
        let mut quiet: Vec<Gathering> = self
            .pending
            .iter()
            .filter(|(_, pending)| now.saturating_duration_since(pending.last) >= QUIET)
            .map(|(gathering, _)| gathering.clone())
            .collect();
        // Roots first, so each takes its folders before they go out alone.
        quiet.sort_by_key(|gathering| !matches!(gathering, Gathering::Root(_)));
        let mut batches = Vec::new();
        for gathering in &quiet {
            let Some(mut pending) = self.pending.remove(gathering) else {
                continue;
            };
            if let Gathering::Root(root) = gathering {
                let under: Vec<Gathering> = self
                    .pending
                    .keys()
                    .filter(|other| matches!(other, Gathering::Folder { root: of, .. } if of == root))
                    .cloned()
                    .collect();
                for folder in under {
                    if let Some(folder) = self.pending.remove(&folder) {
                        pending.events.extend(folder.events);
                    }
                }
            }
            batches.push(pending.events);
        }
        batches
    }

    /// Everything still waiting, as batches — what goes out when the watch
    /// itself goes away.
    pub(super) fn take_all(&mut self) -> Vec<Vec<Event>> {
        self.pending
            .drain()
            .map(|(_, pending)| pending.events)
            .collect()
    }

    /// When the next gathering goes quiet, if any is waiting.
    pub(super) fn next_quiet_at(&self) -> Option<Instant> {
        self.pending.values().map(|pending| pending.last + QUIET).min()
    }
}

/// Gather what the watch reports on `raw` and send each quiet folder's batch
/// on `reports`, for as long as the watch sends. Runs on a thread of its own.
pub(super) fn run(
    raw: std::sync::mpsc::Receiver<notify::Result<Event>>,
    roots: std::sync::Arc<std::sync::Mutex<BTreeSet<PathBuf>>>,
    reports: tokio::sync::mpsc::UnboundedSender<WatchReport>,
) {
    let mut batches = WatchBatches::new();
    let send = |report: WatchReport| {
        if reports.send(report).is_err() {
            tracing::warn!("folder watcher event dropped: task receiver gone");
        }
    };
    loop {
        let received = match batches.next_quiet_at() {
            Some(at) => raw.recv_timeout(at.saturating_duration_since(Instant::now())),
            None => raw
                .recv()
                .map_err(|_| std::sync::mpsc::RecvTimeoutError::Disconnected),
        };
        match received {
            Ok(Ok(event)) => {
                let roots = roots.lock().unwrap().clone();
                batches.add(event, &roots, Instant::now());
            }
            Ok(Err(error)) => send(Err(vec![error])),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                for batch in batches.take_all() {
                    send(Ok(batch));
                }
                return;
            }
        }
        for batch in batches.take_quiet(Instant::now()) {
            send(Ok(batch));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{CreateKind, Flag};
    use notify::EventKind;

    fn created(path: &str) -> Event {
        Event::new(EventKind::Create(CreateKind::File)).add_path(PathBuf::from(path))
    }

    fn lost_track(path: Option<&str>) -> Event {
        let event = Event::new(EventKind::Other).set_flag(Flag::Rescan);
        match path {
            Some(path) => event.add_path(PathBuf::from(path)),
            None => event,
        }
    }

    fn paths(batches: &[Vec<Event>]) -> Vec<Vec<String>> {
        let mut paths: Vec<Vec<String>> = batches
            .iter()
            .map(|batch| {
                let mut paths: Vec<String> = batch
                    .iter()
                    .flat_map(|event| &event.paths)
                    .map(|path| path.to_string_lossy().into_owned())
                    .collect();
                paths.sort();
                paths
            })
            .collect();
        paths.sort();
        paths
    }

    fn roots() -> BTreeSet<PathBuf> {
        [PathBuf::from("/music")].into_iter().collect()
    }

    /// A folder still being written to is held back; one that went quiet
    /// goes out alone.
    #[test]
    fn each_folder_goes_out_once_it_is_quiet_and_not_before() {
        let mut batches = WatchBatches::new();
        let start = Instant::now();
        batches.add(created("/music/One/01.flac"), &roots(), start);
        batches.add(created("/music/Two/01.flac"), &roots(), start);
        batches.add(created("/music/Two/02.flac"), &roots(), start + QUIET / 2);

        assert!(batches.take_quiet(start + QUIET / 2).is_empty());
        assert_eq!(
            paths(&batches.take_quiet(start + QUIET)),
            vec![vec!["/music/One/01.flac".to_string()]]
        );
        assert_eq!(
            paths(&batches.take_quiet(start + QUIET / 2 + QUIET)),
            vec![vec![
                "/music/Two/01.flac".to_string(),
                "/music/Two/02.flac".to_string()
            ]]
        );
    }

    /// Every lost-track event is kept, each under the folder it names: none
    /// replaces another.
    #[test]
    fn every_lost_track_event_is_kept() {
        let mut batches = WatchBatches::new();
        let start = Instant::now();
        batches.add(lost_track(Some("/music/One")), &roots(), start);
        batches.add(lost_track(Some("/music/Two/Disc 1")), &roots(), start);

        assert_eq!(
            paths(&batches.take_quiet(start + QUIET)),
            vec![
                vec!["/music/One".to_string()],
                vec!["/music/Two/Disc 1".to_string()]
            ]
        );
    }

    /// A lost-track event naming no path, or a path holding the root, reads
    /// the root: it goes out naming the root, with the folders under it.
    #[test]
    fn losing_track_of_the_whole_root_names_the_root() {
        for event in [lost_track(None), lost_track(Some("/"))] {
            let mut batches = WatchBatches::new();
            let start = Instant::now();
            batches.add(created("/music/One/01.flac"), &roots(), start);
            batches.add(event, &roots(), start);

            let taken = batches.take_quiet(start + QUIET);
            assert_eq!(taken.len(), 1);
            assert_eq!(
                paths(&taken),
                vec![vec!["/music".to_string(), "/music/One/01.flac".to_string()]]
            );
            assert!(taken[0].iter().any(|event| event.need_rescan()));
        }
    }
}
