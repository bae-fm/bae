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
