use super::*;
use notify::event::{CreateKind, Flag};
use notify::EventKind;

fn created(path: impl Into<PathBuf>) -> Event {
    Event::new(EventKind::Create(CreateKind::File)).add_path(path.into())
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

fn root() -> WatchedRoot {
    WatchedRoot { path: PathBuf::from("/music"), reported: PathBuf::from("/music") }
}

/// A folder still being written to is held back; one that went quiet
/// goes out alone.
#[test]
fn each_folder_goes_out_once_it_is_quiet_and_not_before() {
    let mut batches = WatchBatches::new();
    let start = Instant::now();
    batches.add(&root(), created("/music/One/01.flac"), start);
    batches.add(&root(), created("/music/Two/01.flac"), start);
    batches.add(&root(), created("/music/Two/02.flac"), start + QUIET / 2);

    assert!(batches.take_quiet(start + QUIET / 2).is_empty());
    assert_eq!(
        paths(&batches.take_quiet(start + QUIET)),
        vec![vec!["/music/One/01.flac".to_string()]]
    );
    assert_eq!(
        paths(&batches.take_quiet(start + QUIET / 2 + QUIET)),
        vec![vec!["/music/Two/01.flac".to_string(), "/music/Two/02.flac".to_string()]]
    );
}

/// Every lost-track event is kept, each under the folder it names: none
/// replaces another.
#[test]
fn every_lost_track_event_is_kept() {
    let mut batches = WatchBatches::new();
    let start = Instant::now();
    batches.add(&root(), lost_track(Some("/music/One")), start);
    batches.add(&root(), lost_track(Some("/music/Two/Disc 1")), start);

    assert_eq!(
        paths(&batches.take_quiet(start + QUIET)),
        vec![vec!["/music/One".to_string()], vec!["/music/Two/Disc 1".to_string()]]
    );
}

/// A lost-track event naming no path, or a path holding the root, reads
/// the root: it goes out naming the root, with the folders under it.
#[test]
fn losing_track_of_the_whole_root_names_the_root() {
    for event in [lost_track(None), lost_track(Some("/"))] {
        let mut batches = WatchBatches::new();
        let start = Instant::now();
        batches.add(&root(), created("/music/One/01.flac"), start);
        batches.add(&root(), event, start);

        let taken = batches.take_quiet(start + QUIET);
        assert_eq!(taken.len(), 1);
        assert_eq!(
            paths(&taken),
            vec![vec!["/music".to_string(), "/music/One/01.flac".to_string()]]
        );
        assert!(taken[0].iter().any(|event| event.need_rescan()));
    }
}

/// A root reached through a symlink is reported where it resolves to; its
/// events go out spelled as the root is, gathered under its folders.
#[test]
fn a_resolved_spelling_goes_out_as_the_roots_own() {
    let root = WatchedRoot {
        path: PathBuf::from("/var/music"),
        reported: PathBuf::from("/private/var/music"),
    };
    // A watch names a file under the spelling it reports the root by,
    // joined with the platform's separator.
    let under = |root: &Path, file: &str| root.join("One").join(file);
    let mut batches = WatchBatches::new();
    let start = Instant::now();
    batches.add(&root, created(under(&root.reported, "01.flac")), start);
    batches.add(&root, created(under(&root.reported, "02.flac")), start);
    batches.add(&root, lost_track(Some("/private/var")), start + QUIET);

    let spelled = |file: &str| under(&root.path, file).to_string_lossy().into_owned();
    assert_eq!(
        paths(&batches.take_quiet(start + QUIET)),
        vec![vec![spelled("01.flac"), spelled("02.flac")]]
    );
    let whole = batches.take_quiet(start + QUIET + QUIET);
    assert_eq!(paths(&whole), vec![vec!["/var/music".to_string()]]);
    assert!(whole[0][0].need_rescan());
}
