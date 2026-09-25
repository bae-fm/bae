//! The most files a piece of work held open at once under a directory,
//! sampled from the process's real open descriptors.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

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
                    peak.fetch_max(coven::open_files_under(&dir).len(), Ordering::Relaxed);
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
