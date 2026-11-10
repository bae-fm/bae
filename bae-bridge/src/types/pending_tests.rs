use super::*;

/// A write waiting for its circle's data to be raised to this version says
/// so; any other prerequisite reads as waiting for earlier changes.
#[test]
fn a_write_waiting_for_a_raise_names_the_raise() {
    let key = |prerequisite| {
        BridgePendingReason::from_core(coven::PendingReason::Waits(prerequisite), None).key
    };
    assert_eq!(
        key(coven::Prerequisite::Raise { audience: coven::Audience::Store, version: 2 }),
        "core.pending.reason.waits_for_raise"
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
