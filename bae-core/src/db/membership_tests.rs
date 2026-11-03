use super::*;

#[test]
fn an_admin_removes_any_member_but_themself() {
    assert!(MembershipMember::removable(true, false));
    assert!(!MembershipMember::removable(true, true));
    assert!(!MembershipMember::removable(false, false));
}

#[test]
fn a_member_removes_only_their_own_other_devices() {
    // A member: their own other device, nobody else's, never this one.
    assert!(ActiveDevice::removable(false, true, false));
    assert!(!ActiveDevice::removable(false, false, false));
    assert!(!ActiveDevice::removable(false, true, true));
    // An admin: anyone's device, still never this one.
    assert!(ActiveDevice::removable(true, false, false));
    assert!(!ActiveDevice::removable(true, true, true));
}
