//! What this device's disk says about the originals the library recorded at
//! import: still the file that was imported, or gone or changed since.
//!
//! Looked at apart from the database. The originals sit wherever they were
//! imported from, often a network share; while an import reads its own files
//! from that share, the share answers a look late. A database read never waits
//! on it: it reads what the library recorded, and what the disk says arrives
//! from here when the disk says it.

use super::*;
use crate::album_detail::{OriginalOnDisk, OriginalsSeen};
use crate::db::RecordedOriginal;

impl LibraryManager {
    /// Look at each of `originals` on this device's disk.
    pub(crate) async fn look_at_originals(
        &self,
        originals: Vec<RecordedOriginal>,
    ) -> Result<OriginalsSeen, LibraryError> {
        if originals.is_empty() {
            return Ok(OriginalsSeen::default());
        }
        #[cfg(test)]
        let disk = self.disk_hold.clone();
        blocking::blocking_io(move || {
            #[cfg(test)]
            disk.wait();
            originals
                .into_iter()
                .map(|original| {
                    let on_disk = on_disk(&original)?;
                    Ok((original, on_disk))
                })
                .collect()
        })
        .await
    }

    /// Hold every look at this device's originals until the returned hold
    /// drops: a volume busy serving other reads.
    #[cfg(test)]
    pub(crate) fn hold_disk(&self) -> DiskHeld {
        *self.disk_hold.held.lock().unwrap() = true;
        DiskHeld(self.disk_hold.clone())
    }
}

/// Whether `original` is still the file recorded for it: the same size and
/// modification time, to the nanosecond the record holds.
fn on_disk(original: &RecordedOriginal) -> Result<OriginalOnDisk, LibraryError> {
    match std::fs::metadata(&original.path) {
        Ok(metadata) => {
            let unchanged = i64::try_from(metadata.len()) == Ok(original.size)
                && coven::unix_seconds_and_nanos(metadata.modified()?)
                    == (original.modified_seconds, original.modified_nanos);
            Ok(if unchanged {
                OriginalOnDisk::Unchanged
            } else {
                OriginalOnDisk::Changed
            })
        }
        Err(error) => {
            debug!(
                path = %original.path.display(),
                %error,
                "a release's original is not readable"
            );
            Ok(OriginalOnDisk::Changed)
        }
    }
}

/// Where [`LibraryManager::hold_disk`] holds the looks at originals.
#[cfg(test)]
#[derive(Default)]
pub(crate) struct DiskHold {
    held: std::sync::Mutex<bool>,
    released: std::sync::Condvar,
}

#[cfg(test)]
impl DiskHold {
    fn wait(&self) {
        let _released = self
            .released
            .wait_while(self.held.lock().unwrap(), |held| *held)
            .unwrap();
    }
}

/// Lets the looks at originals through when dropped.
#[cfg(test)]
pub(crate) struct DiskHeld(Arc<DiskHold>);

#[cfg(test)]
impl Drop for DiskHeld {
    fn drop(&mut self) {
        *self.0.held.lock().unwrap() = false;
        self.0.released.notify_all();
    }
}

#[cfg(test)]
#[path = "originals_tests.rs"]
mod tests;
