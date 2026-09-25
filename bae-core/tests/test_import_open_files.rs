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

use bae_test_support as support;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const DISCS: usize = 3;
const TRACKS_PER_DISC: usize = 100;

/// Descriptors the import may hold above the idle library's, whatever the
/// track count: the file being read, the decode's source, and the library's
/// own connections and pipes. The soft limit is set this far above the
/// descriptors already open, so an import that exceeds it fails outright.
const HEADROOM: usize = 48;

/// Source files the import may hold open at once, whatever the track count:
/// the one being read or decoded, and the one before it while its stream
/// closes.
const MAX_SOURCES_OPEN: usize = 2;

/// The descriptors this process holds open: every entry of `/dev/fd`, which
/// includes the one the listing itself opens, so every reading is off by the
/// same one. `None` when the listing itself is refused for want of a
/// descriptor.
fn open_fds() -> Option<Vec<i32>> {
    let entries = fs::read_dir("/dev/fd").ok()?;
    Some(
        entries
            .filter_map(|entry| entry.ok()?.file_name().to_str()?.parse().ok())
            .collect(),
    )
}

/// The file descriptor `fd` refers to, when it is a file with a path (not a
/// socket, pipe, or kqueue, and still open).
fn fd_path(fd: i32) -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let mut buf = vec![0u8; libc::PATH_MAX as usize];
        // SAFETY: F_GETPATH writes at most PATH_MAX bytes into `buf`.
        if unsafe { libc::fcntl(fd, libc::F_GETPATH, buf.as_mut_ptr()) } != 0 {
            return None;
        }
        let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        Some(PathBuf::from(
            String::from_utf8_lossy(&buf[..len]).into_owned(),
        ))
    }
    #[cfg(not(target_os = "macos"))]
    {
        fs::read_link(format!("/proc/self/fd/{fd}")).ok()
    }
}

/// How many descriptors are open in all, and how many of them are files under
/// `dir`.
fn count_open(dir: &Path) -> Option<(usize, usize)> {
    let fds = open_fds()?;
    let under_dir = fds
        .iter()
        .filter(|&&fd| fd_path(fd).is_some_and(|path| path.starts_with(dir)))
        .count();
    Some((fds.len(), under_dir))
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

/// Samples the open descriptors on its own thread until stopped, keeping the
/// highest total and the most files under the source folder it saw at once.
struct FdPeakSampler {
    stop: Arc<AtomicBool>,
    peak_total: Arc<AtomicUsize>,
    peak_sources: Arc<AtomicUsize>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl FdPeakSampler {
    fn start(source_dir: PathBuf) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let peak_total = Arc::new(AtomicUsize::new(0));
        let peak_sources = Arc::new(AtomicUsize::new(0));
        let thread = std::thread::spawn({
            let stop = stop.clone();
            let peak_total = peak_total.clone();
            let peak_sources = peak_sources.clone();
            move || {
                while !stop.load(Ordering::Relaxed) {
                    // A listing refused for want of a descriptor is itself the
                    // limit being hit; the import's own failure reports it.
                    if let Some((total, sources)) = count_open(&source_dir) {
                        peak_total.fetch_max(total, Ordering::Relaxed);
                        peak_sources.fetch_max(sources, Ordering::Relaxed);
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
        });
        Self {
            stop,
            peak_total,
            peak_sources,
            thread: Some(thread),
        }
    }

    /// (peak total descriptors, peak source files open at once)
    fn finish(mut self) -> (usize, usize) {
        self.stop.store(true, Ordering::Relaxed);
        self.thread.take().unwrap().join().unwrap();
        (
            self.peak_total.load(Ordering::Relaxed),
            self.peak_sources.load(Ordering::Relaxed),
        )
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

    // Descriptors report canonical paths (`/private/var/...` on macOS).
    let source_dir = album_dir.canonicalize().expect("canonical album dir");
    let before = open_fds().expect("list descriptors");
    let baseline = before.len();
    let highest_fd = *before.iter().max().expect("stdio is open") as usize;
    let _limit = LoweredFileLimit::to((highest_fd + 1 + HEADROOM) as libc::rlim_t);

    let sampler = FdPeakSampler::start(source_dir.clone());
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
    let (peak_total, peak_sources) = sampler.finish();
    eprintln!(
        "open descriptors over {} tracks in {:.1?}: baseline {baseline}, peak {peak_total}; \
         source files open at once: at most {peak_sources}",
        DISCS * TRACKS_PER_DISC,
        started.elapsed()
    );

    let (release_id, _album_id) = result.unwrap_or_else(|error| {
        panic!(
            "the box set import failed under a soft limit of {HEADROOM} spare descriptors: {error}"
        )
    });
    assert!(
        peak_sources <= MAX_SOURCES_OPEN,
        "the import held {peak_sources} source files open at once"
    );
    let tracks = library_manager
        .get_tracks_for_release(&release_id)
        .await
        .expect("read imported tracks");
    assert_eq!(tracks.len(), DISCS * TRACKS_PER_DISC);

    // Every source file the import opened is closed once it is done. The
    // library's own descriptors (its store connections) are not the import's
    // and may stay.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let (_, sources_open) = count_open(&source_dir).expect("list descriptors");
        if sources_open == 0 {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "{sources_open} source files stayed open after the import finished"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}
