//! The most files a piece of work held open at once under a directory,
//! sampled from the process's real open descriptors.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// The files under `dir` this process held open together at one moment:
/// those named by two reads of its open files, one right after the other.
///
/// One read is not one moment. It lists the descriptors (on Windows, the
/// handles) and then names them one at a time, and a number freed in between
/// can be taken by a file opened later. So one read can name a file that
/// closed early in it beside one that opened late in it: two files never
/// open together, as when one track's file closes and the next track's opens
/// under the number another test's file just freed. A file named by both
/// reads was open at the moment between them, unless it closed and reopened
/// in the time between those two reads. So the files named by both reads were
/// open together at that moment.
pub fn open_together(dir: &Path) -> Vec<PathBuf> {
    let first = coven::open_files_under(dir);
    let second = coven::open_files_under(dir);
    named_by_both(first, second)
}

/// Each path as many times as both reads name it.
fn named_by_both(first: Vec<PathBuf>, mut second: Vec<PathBuf>) -> Vec<PathBuf> {
    first
        .into_iter()
        .filter(|path| match second.iter().position(|named| named == path) {
            Some(index) => {
                second.swap_remove(index);
                true
            }
            None => false,
        })
        .collect()
}

/// Samples the files open under a directory on its own thread until
/// finished, keeping the most it saw open at once.
pub struct OpenFilesPeak {
    stop: Arc<AtomicBool>,
    peak: Arc<AtomicUsize>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl OpenFilesPeak {
    pub fn start(dir: &Path) -> Self {
        let dir = dir.to_path_buf();
        let stop = Arc::new(AtomicBool::new(false));
        let peak = Arc::new(AtomicUsize::new(0));
        let thread = std::thread::spawn({
            let stop = stop.clone();
            let peak = peak.clone();
            move || {
                while !stop.load(Ordering::Relaxed) {
                    peak.fetch_max(open_together(&dir).len(), Ordering::Relaxed);
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
        });
        Self {
            stop,
            peak,
            thread: Some(thread),
        }
    }

    /// Stop sampling and return the most files seen open at once.
    pub fn finish(mut self) -> usize {
        self.stop.store(true, Ordering::Relaxed);
        self.thread
            .take()
            .expect("sampler runs until finished")
            .join()
            .expect("sampler thread");
        self.peak.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(names: &[&str]) -> Vec<PathBuf> {
        names.iter().map(PathBuf::from).collect()
    }

    /// One track's file named early in the first read and the next track's
    /// named late in it are one file at a time: only the file both reads
    /// name counts.
    #[test]
    fn a_file_counts_only_when_both_reads_name_it() {
        assert_eq!(
            named_by_both(paths(&["01.flac", "02.flac"]), paths(&["02.flac"])),
            paths(&["02.flac"])
        );
        assert_eq!(
            named_by_both(paths(&["01.flac"]), paths(&["02.flac"])),
            paths(&[])
        );
    }

    /// A file open through two descriptors in both reads counts twice; one
    /// the second read names once counts once.
    #[test]
    fn a_file_counts_as_often_as_both_reads_name_it() {
        assert_eq!(
            named_by_both(
                paths(&["image.flac", "image.flac"]),
                paths(&["image.flac", "image.flac"])
            ),
            paths(&["image.flac", "image.flac"])
        );
        assert_eq!(
            named_by_both(paths(&["image.flac", "image.flac"]), paths(&["image.flac"])),
            paths(&["image.flac"])
        );
    }

    #[test]
    fn a_file_held_open_is_open_together_until_it_closes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a");
        std::fs::write(&path, b"a").unwrap();
        let path = path.canonicalize().unwrap();

        let file = std::fs::File::open(&path).unwrap();
        assert_eq!(open_together(dir.path()), vec![path]);
        drop(file);
        assert!(open_together(dir.path()).is_empty());
    }
}
