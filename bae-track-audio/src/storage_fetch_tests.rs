use super::*;

const PLAYING: u64 = 1;
const PRELOAD: u64 = 2;

/// Fetches whose playing track is "track-1", reading from `PLAYING`.
fn playing() -> Arc<StorageFetches> {
    let fetches = Arc::new(StorageFetches::new());
    fetches.set_foreground(vec![PLAYING], "track-1");
    fetches
}

/// What coven tells `read`'s progress, into `buffer_id`, as its reporting
/// closure tells it once the read fetches from storage.
fn tell(read: &FetchProgress, buffer_id: u64) {
    read.fetches.fetches(read.id, buffer_id);
}

fn fetching(track_id: &str) -> Option<String> {
    Some(track_id.to_string())
}

/// The playing track fetches from storage from the first read coven tells
/// it fetches until the last such read ends.
#[test]
fn the_playing_track_fetches_while_its_reads_from_storage_run() {
    let fetches = playing();
    let told = fetches.subscribe();

    let first = fetches.read(PLAYING);
    let second = fetches.read(PLAYING);
    assert_eq!(told.fetching(), None, "a read coven told nothing reads no storage");
    tell(&first, PLAYING);
    tell(&second, PLAYING);
    assert_eq!(told.fetching(), fetching("track-1"));

    drop(first);
    assert_eq!(told.fetching(), fetching("track-1"));
    drop(second);
    assert_eq!(told.fetching(), None);
}

/// A preload's reads are not the playing track's.
#[test]
fn a_preloads_reads_are_not_the_playing_tracks() {
    let fetches = playing();
    let told = fetches.subscribe();

    let preload = fetches.read(PRELOAD);
    tell(&preload, PRELOAD);

    assert_eq!(told.fetching(), None);
}

/// A track's reads count as the playing track's from the moment it becomes
/// the playing track, the ones already running included.
#[test]
fn a_track_that_becomes_the_playing_one_brings_its_running_reads() {
    let fetches = playing();
    let told = fetches.subscribe();

    let next = fetches.read(PRELOAD);
    tell(&next, PRELOAD);
    assert_eq!(told.fetching(), None);

    fetches.set_foreground(vec![PRELOAD], "track-2");
    assert_eq!(told.fetching(), fetching("track-2"));
}
