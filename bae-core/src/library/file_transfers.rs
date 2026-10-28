//! How far each file in flight one way has got, for a surface that draws
//! each file on its own: coven's transfer watch, each file's progress as the
//! share of its encrypted object moved. Progress lives only in coven's
//! memory, so no table holds it and no live query reads it.

use std::collections::BTreeMap;

/// The share of a file's object moved so far, from 0 to 1. An object coven
/// reports no size for has moved none of it.
pub(crate) fn fraction(progress: coven::TransferProgress) -> f64 {
    if progress.bytes_total == 0 {
        return 0.0;
    }
    progress.bytes_transferred as f64 / progress.bytes_total as f64
}

/// Apply what a transfer watch told to `held`, the progress of each file in
/// flight as the reader last heard it.
pub(crate) fn apply(
    held: &mut BTreeMap<coven::FileId, coven::TransferProgress>,
    changes: Vec<coven::TransferChange>,
) {
    for change in changes {
        match change {
            coven::TransferChange::Reset(files) => *held = files,
            coven::TransferChange::Updated { file, progress } => {
                held.insert(file, progress);
            }
            coven::TransferChange::Removed { file } => {
                held.remove(&file);
            }
        }
    }
}

/// A change to one file's share moved, or every file's.
#[derive(Debug, Clone, PartialEq)]
pub enum FileTransferChange {
    /// Every file in flight, as a reader first hears them; a file not listed
    /// has nothing in flight.
    Reset(BTreeMap<coven::FileId, f64>),
    /// The file started, or its share moved.
    Updated { file: coven::FileId, fraction: f64 },
    /// The file's transfer ended, whether it finished or not.
    Removed { file: coven::FileId },
}

impl FileTransferChange {
    fn of(change: coven::TransferChange) -> Self {
        match change {
            coven::TransferChange::Reset(files) => Self::Reset(
                files
                    .into_iter()
                    .map(|(file, progress)| (file, fraction(progress)))
                    .collect(),
            ),
            coven::TransferChange::Updated { file, progress } => Self::Updated {
                file,
                fraction: fraction(progress),
            },
            coven::TransferChange::Removed { file } => Self::Removed { file },
        }
    }
}

/// Every file in flight one way, for a reader that draws each: all of them
/// first, then each file whose share moved, started or ended.
pub struct FileTransfersWatch {
    watch: coven::TransferWatch,
}

impl FileTransfersWatch {
    pub(crate) fn of(watch: coven::TransferWatch) -> Self {
        Self { watch }
    }

    /// Every file in flight as one `Reset` on the first read; after that,
    /// wait for a file's share to change and name each file it changed for.
    /// `None` once the store has closed.
    pub async fn next(&mut self) -> Option<Vec<FileTransferChange>> {
        Some(
            self.watch
                .next()
                .await?
                .into_iter()
                .map(FileTransferChange::of)
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn progress(bytes_transferred: u64, bytes_total: u64) -> coven::TransferProgress {
        coven::TransferProgress {
            bytes_transferred,
            bytes_total,
        }
    }

    /// A file's share is the bytes of its object moved over the object's
    /// size, and an object of no size has moved none.
    #[test]
    fn a_files_share_is_its_objects_bytes_moved() {
        assert_eq!(fraction(progress(25, 100)), 0.25);
        assert_eq!(fraction(progress(100, 100)), 1.0);
        assert_eq!(fraction(progress(0, 0)), 0.0);
    }

    /// A reader holds every file the reset names, then each one updated, and
    /// none removed.
    #[test]
    fn a_reader_holds_each_file_in_flight() {
        let (a, b) = (
            coven::FileId(uuid::Uuid::from_u128(1)),
            coven::FileId(uuid::Uuid::from_u128(2)),
        );
        let mut held = BTreeMap::new();
        apply(
            &mut held,
            vec![coven::TransferChange::Reset(
                [(a, progress(1, 10))].into_iter().collect(),
            )],
        );
        apply(
            &mut held,
            vec![
                coven::TransferChange::Updated {
                    file: b,
                    progress: progress(2, 10),
                },
                coven::TransferChange::Removed { file: a },
            ],
        );
        assert_eq!(held, [(b, progress(2, 10))].into_iter().collect());
    }
}
