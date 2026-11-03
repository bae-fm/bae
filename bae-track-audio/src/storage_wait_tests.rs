use super::*;

const PLAYING: u64 = 1;
const PRELOAD: u64 = 2;

/// Waits whose playing track is "track-1", reading from `PLAYING`.
fn playing() -> Arc<StorageWaits> {
    let waits = Arc::new(StorageWaits::new());
    waits.set_foreground(vec![PLAYING], "track-1");
    waits
}

/// What coven tells `read`'s progress, into `buffer_id`, as its reporting
/// closure tells it once the read fetches from storage.
fn tell(read: &FetchProgress, buffer_id: u64) {
    read.waits.fetches(read.id, buffer_id);
}

fn waiting(track_id: &str) -> Option<String> {
    Some(track_id.to_string())
}

/// While the person waits on the playing track's load, it waits on storage
/// from the first read coven tells it fetches until the last such read ends.
#[test]
fn a_waited_load_waits_on_its_reads_from_storage() {
    let waits = playing();
    let mut told = waits.subscribe();
    waits.set_waiting(Some("track-1"));

    let first = waits.read(PLAYING);
    let second = waits.read(PLAYING);
    assert_eq!(told.waiting(), None, "a read coven told nothing reads no storage");
    tell(&first, PLAYING);
    tell(&second, PLAYING);
    assert_eq!(told.waiting(), waiting("track-1"));

    drop(first);
    assert_eq!(told.waiting(), waiting("track-1"));
    drop(second);
    assert_eq!(told.waiting(), None);
}

/// A preload's reads are not what the playing track's load waits on.
#[test]
fn a_preloads_reads_are_no_wait() {
    let waits = playing();
    let mut told = waits.subscribe();
    waits.set_waiting(Some("track-1"));

    let preload = waits.read(PRELOAD);
    tell(&preload, PRELOAD);

    assert_eq!(told.waiting(), None);
}

/// Reading ahead while the track plays is no wait: nothing is told until a
/// load is waited on, and nothing once it ends.
#[test]
fn reads_while_nothing_loads_are_no_wait() {
    let waits = playing();
    let mut told = waits.subscribe();

    let read = waits.read(PLAYING);
    tell(&read, PLAYING);
    assert_eq!(told.waiting(), None);

    waits.set_waiting(Some("track-1"));
    assert_eq!(told.waiting(), waiting("track-1"));
    waits.set_waiting(None);
    assert_eq!(told.waiting(), None);
}

/// A load of a track that isn't playing yet waits on none of the playing
/// track's reads: it waits once the loading track becomes the playing one.
#[test]
fn a_load_of_another_track_waits_on_none_of_the_playing_ones_reads() {
    let waits = playing();
    let mut told = waits.subscribe();
    waits.set_waiting(Some("track-2"));

    let read = waits.read(PLAYING);
    tell(&read, PLAYING);
    assert_eq!(told.waiting(), None);

    let next = waits.read(PRELOAD);
    tell(&next, PRELOAD);
    waits.set_foreground(vec![PRELOAD], "track-2");
    assert_eq!(told.waiting(), waiting("track-2"));
}
