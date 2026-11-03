use super::*;
use DbUnstoredAudio::*;

fn device_a() -> OtherDevice {
    OtherDevice::Active { name: "Device A".to_string() }
}

#[test]
fn an_attached_release_plays_on_any_device() {
    assert_eq!(release_playability(&[]), ReleasePlayability::Playable);
}

#[test]
fn a_local_release_plays_where_its_originals_are() {
    assert_eq!(release_playability(&[Original, Original]), ReleasePlayability::Playable);
}

#[test]
fn a_local_release_from_another_device_names_that_device() {
    assert_eq!(
        release_playability(&[Original, Elsewhere(device_a())]),
        ReleasePlayability::OnOtherDevice(device_a())
    );
}
