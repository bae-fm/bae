use super::*;

/// A write waiting for its circle's data to be raised to this version says
/// so; any other prerequisite reads as waiting for earlier changes.
#[test]
fn a_write_waiting_for_a_reload_or_a_catch_up_names_the_wait() {
    let key = |prerequisite| {
        BridgePendingReason::from_core(coven::PendingReason::Waits(prerequisite), None).key
    };
    assert_eq!(
        key(coven::Prerequisite::Reload(coven::Audience::Store)),
        "core.pending.reason.waits"
    );
    assert_eq!(key(coven::Prerequisite::SequenceCatchUp), "core.pending.reason.waits");
}

/// A joined device whose admission the library refused was never in it; its
/// records say it was removed, as coven shows it (coven §13.2), and a change
/// it asked for stopped as a removal stops one.
#[test]
fn a_refused_admission_reads_as_a_removal() {
    let key = |reason| BridgePendingReason::from_core(reason, None).key;
    assert_eq!(key(coven::PendingReason::NotAdmitted), key(coven::PendingReason::Removed));
    assert_eq!(
        crate::BridgeDeviceStop::from_core(coven::DeviceStop::NotAdmitted),
        crate::BridgeDeviceStop::Removed
    );
}

/// Only this device's uploads and moves of files are its sending files; a
/// download or a connection is not.
#[test]
fn a_record_sends_files_when_it_is_an_upload_or_a_move() {
    let sends = |subject| {
        BridgePendingRecord::from_core(bae_sync_status::PendingItem::before_open(
            coven::PendingRecord::new(subject, coven::PendingReason::NoStorage),
        ))
        .sends_files
    };
    assert!(sends(coven::PendingSubject::Move { id: coven::MoveId(3) }));
    assert!(!sends(coven::PendingSubject::Connection));
}
