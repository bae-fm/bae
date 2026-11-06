use super::*;
use bae_people::membership::MembershipMember;

fn device(id: u64) -> ActiveDevice {
    ActiveDevice {
        id: coven::DeviceId(id),
        name: format!("Device {id}"),
        is_self: id == 1,
        can_remove: id != 1,
    }
}

fn member(id: u128, is_self: bool, devices: &[u64]) -> MembershipMember {
    MembershipMember {
        id: coven::MemberId(uuid::Uuid::from_u128(id)),
        keys: coven::MemberKeys::generate().unwrap().public_keys(),
        role: coven::MemberRole::Member,
        access: coven::MemberAccess::S3AccessKey { access_key_id: format!("key-{id}") },
        is_self,
        can_remove: false,
        active_devices: devices.iter().copied().map(device).collect(),
        former_devices: Vec::new(),
    }
}

/// Add Device's sheet watches for the person's device that joins after
/// it made the code: what the person had then is no news, nor is another
/// person's new device.
#[test]
fn a_watch_finds_the_persons_device_that_joins_after_the_code() {
    let membership = |own: &[u64], others: &[u64]| Members {
        members: vec![member(1, true, own), member(2, false, others)],
        self_is_admin: false,
    };
    let watch = AddedDeviceWatch::new(&membership(&[1, 2], &[3]));

    assert_eq!(watch.added(&membership(&[1, 2], &[3])), None);
    assert_eq!(watch.added(&membership(&[1, 2], &[3, 4])), None);
    assert_eq!(watch.added(&membership(&[1, 2, 5], &[3, 4])), Some(device(5)));
}
