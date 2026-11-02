use super::*;

const PLAYING: u64 = 1;
const PRELOAD: u64 = 2;

/// Downloads whose playing track is "track-1", reading from `PLAYING`.
fn playing() -> Arc<TrackDownloads> {
    let downloads = Arc::new(TrackDownloads::new());
    downloads.set_foreground(PLAYING, "track-1");
    downloads
}

/// What coven tells `read`'s progress: `here` of its `len` bytes into
/// `buffer_id`, as its reporting closure tells it.
fn tell(read: &FetchProgress, buffer_id: u64, len: u64, here: u64) {
    read.downloads.told(read.id, buffer_id, len, here);
}

fn track_download(here: u64, total: u64) -> Option<TrackDownload> {
    Some(TrackDownload { track_id: "track-1".into(), here, total })
}

/// While the person waits on the playing track's load, its reads from
/// storage are the download: their bytes here of their lengths, summed,
/// until each read ends.
#[test]
fn a_waited_load_tells_its_reads_from_storage() {
    let downloads = playing();
    let told = downloads.subscribe();
    downloads.set_waiting(Some("track-1"));

    let first = downloads.read(PLAYING, 100);
    let second = downloads.read(PLAYING, 50);
    assert_eq!(*told.borrow(), None, "a read coven told nothing is not a download");
    tell(&first, PLAYING, 100, 30);
    tell(&second, PLAYING, 50, 10);
    assert_eq!(*told.borrow(), track_download(40, 150));

    drop(first);
    assert_eq!(*told.borrow(), track_download(10, 50));
    drop(second);
    assert_eq!(*told.borrow(), None);
}

/// A preload's reads are not what the playing track's load waits on.
#[test]
fn a_preloads_reads_are_no_download() {
    let downloads = playing();
    let told = downloads.subscribe();
    downloads.set_waiting(Some("track-1"));

    let preload = downloads.read(PRELOAD, 100);
    tell(&preload, PRELOAD, 100, 30);

    assert_eq!(*told.borrow(), None);
}

/// Reading ahead while the track plays is no wait: nothing is told until a
/// load is waited on, and nothing once it ends.
#[test]
fn reads_while_nothing_loads_are_no_download() {
    let downloads = playing();
    let told = downloads.subscribe();

    let read = downloads.read(PLAYING, 100);
    tell(&read, PLAYING, 100, 30);
    assert_eq!(*told.borrow(), None);

    downloads.set_waiting(Some("track-1"));
    assert_eq!(*told.borrow(), track_download(30, 100));
    downloads.set_waiting(None);
    assert_eq!(*told.borrow(), None);
}

/// A load of a track that isn't playing yet waits on none of the playing
/// track's reads: they are told once the loading track becomes the playing
/// one.
#[test]
fn a_load_of_another_track_waits_on_none_of_the_playing_ones_reads() {
    let downloads = playing();
    let told = downloads.subscribe();
    downloads.set_waiting(Some("track-2"));

    let read = downloads.read(PLAYING, 100);
    tell(&read, PLAYING, 100, 30);
    assert_eq!(*told.borrow(), None);

    let next = downloads.read(PRELOAD, 80);
    tell(&next, PRELOAD, 80, 20);
    downloads.set_foreground(PRELOAD, "track-2");
    assert_eq!(
        *told.borrow(),
        Some(TrackDownload { track_id: "track-2".into(), here: 20, total: 80 })
    );
}
