//! How many files an import holds open at once.
//!
//! A macOS GUI app runs under a soft `RLIMIT_NOFILE` of 256, and an idle
//! library already holds dozens of descriptors (the store, its WAL, sockets,
//! kqueues), so an import may hold only a constant number of source files open
//! however many tracks it has. This binary holds one test so no other test's
//! descriptors move the counts it reads: it imports a three-disc folder of 300
//! tracks under a soft limit only a few dozen descriptors above what the
//! library already holds, samples the process's open descriptors throughout,
//! and requires the source files held open at once to stay within a constant
//! and none of them to remain open once the import is done.

use bae_core::open_files_peak::OpenFilesPeak;
use bae_test_support as support;

use std::fs;
use std::path::Path;
use std::time::Instant;

const DISCS: usize = 3;
const TRACKS_PER_DISC: usize = 100;

/// Source files the import may hold open at once, whatever the track count:
/// it measures as many tracks at once as there are cores, and each track reads
/// one file here.
fn max_sources_open() -> usize {
    std::thread::available_parallelism()
        .expect("available parallelism")
        .get()
}

/// Descriptors the import may hold above the idle library's, whatever the
/// track count: its source files, and the library's own connections and
/// pipes. The soft limit is set this far above the descriptors already open,
/// so an import that exceeds it fails outright.
fn headroom() -> usize {
    max_sources_open() + 32
}

/// The highest descriptor number this process holds open.
fn highest_open_fd() -> usize {
    fs::read_dir("/dev/fd")
        .expect("list /dev/fd")
        .filter_map(|entry| entry.ok()?.file_name().to_str()?.parse().ok())
        .max()
        .expect("stdio is open")
}

/// The process's soft descriptor limit lowered for the test's duration and
/// restored when it ends, including on a failed assertion.
struct LoweredFileLimit {
    original: libc::rlimit,
}

impl LoweredFileLimit {
    fn to(soft: libc::rlim_t) -> Self {
        let mut original = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        // SAFETY: getrlimit/setrlimit write and read the rlimit we pass.
        assert_eq!(
            unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut original) },
            0
        );
        let lowered = libc::rlimit {
            rlim_cur: soft.min(original.rlim_max),
            rlim_max: original.rlim_max,
        };
        assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &lowered) }, 0);
        Self { original }
    }
}

impl Drop for LoweredFileLimit {
    fn drop(&mut self) {
        // SAFETY: restores the limit read in `to`.
        unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &self.original) };
    }
}

fn write_box_set(album_dir: &Path) {
    let flac = fs::read(bae_test_support::fixture_dir!(
        "flac",
        "01 Test Track 1.flac"
    ))
    .expect("FLAC fixture");
    for disc in 1..=DISCS {
        let disc_dir = album_dir.join(format!("CD{disc}"));
        fs::create_dir_all(&disc_dir).unwrap();
        for track in 1..=TRACKS_PER_DISC {
            fs::write(
                disc_dir.join(format!("{track:03} - Track Title {track}.flac")),
                &flac,
            )
            .unwrap();
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_box_set_import_holds_a_bounded_number_of_files_open() {
    let (library_manager, album_dir, _temp) = support::setup_test_library_with_album_dir().await;
    write_box_set(&album_dir);

    let release = bae_core::discogs::DiscogsRelease {
        tracklist: (1..=DISCS)
            .flat_map(|disc| {
                (1..=TRACKS_PER_DISC).map(move |track| {
                    support::discogs_track(
                        &format!("{disc}-{track}"),
                        &format!("Track Title {track}"),
                        "3:00",
                    )
                })
            })
            .collect(),
        ..support::discogs_test_release("test-box-set-open-files", "Album Title", &[])
    };
    let release_key = support::seed_discogs_test_release(library_manager.providers(), release);
    let import_handle =
        support::start_test_import(tokio::runtime::Handle::current(), library_manager.clone())
            .await;

    let _limit = LoweredFileLimit::to((highest_open_fd() + 1 + headroom()) as libc::rlim_t);

    let sampler = OpenFilesPeak::start(&album_dir);
    let started = Instant::now();
    let import_id = uuid::Uuid::new_v4().to_string();
    import_handle
        .send_command(support::folder_import(
            &import_id,
            album_dir.clone(),
            support::discogs_release(release_key),
        ))
        .await
        .expect("import command is accepted");
    let mut progress_rx = import_handle.subscribe_import(import_id);
    let result = support::try_wait_for_import_complete(&mut progress_rx).await;
    let peak_sources = sampler.finish();
    eprintln!(
        "{} tracks imported in {:.1?} with at most {peak_sources} source files open at once",
        DISCS * TRACKS_PER_DISC,
        started.elapsed()
    );

    let (release_id, _album_id) = result.unwrap_or_else(|error| {
        panic!(
            "the box set import failed under a soft limit of {} spare descriptors: {error}",
            headroom()
        )
    });
    assert!(
        peak_sources <= max_sources_open(),
        "the import held {peak_sources} source files open at once, over {} cores",
        max_sources_open()
    );
    let tracks = library_manager
        .get_tracks_for_release(&release_id)
        .await
        .expect("read imported tracks");
    assert_eq!(tracks.len(), DISCS * TRACKS_PER_DISC);

    // Every source file the import opened is closed once it is done.
    coven::assert_no_open_files_under(&album_dir);
}
