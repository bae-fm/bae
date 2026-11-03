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
/// the files before it and the file's own; it is listed while the report
/// lives and gone once it drops.
#[test]
fn a_report_lists_its_release_until_it_drops() {
    let transfers = MakeLocalTransfers::new();
    let read = transfers.subscribe();
    let report = transfers.report("release");
    report.progress(step(300, 1_000, 50, 200));
    assert_eq!(
        read.borrow().get("release"),
        Some(&MakeLocalTransferProgress {
            file_bytes_done: 50,
            file_bytes_total: 200,
            bytes_done: 350,
            bytes_total: 1_000,
        })
    );
    drop(report);
    assert!(read.borrow().is_empty());
}
