use super::*;

fn step(before: u64, all: u64, done: u64, total: u64) -> coven::Progress<coven::DeviceOnlyStep> {
    coven::Progress {
        step: coven::DeviceOnlyStep::Downloading {
            file: coven::FileId(uuid::Uuid::from_u128(1)),
            before,
            all,
        },
        done,
        total,
    }
}

/// A release's progress is the file's bytes and, of the whole release,
/// the files before it and the file's own.
#[test]
fn a_release_has_come_as_far_as_its_files_before_and_its_file() {
    assert_eq!(
        MakeLocalTransferProgress::of(step(300, 1_000, 50, 200)),
        MakeLocalTransferProgress {
            file_bytes_done: 50,
            file_bytes_total: 200,
            bytes_done: 350,
            bytes_total: 1_000,
        }
    );
}
