use super::*;

/// The files being sent together are in their stream only while any is
/// being sent: a total with no bar holds nothing.
#[test]
fn the_upload_total_is_held_only_while_files_are_sent() {
    let bar = bae_core::library::UploadBar {
        bytes_done: 10,
        bytes_total: 40,
    };
    let sending = bae_core::library::UploadTransfer {
        bar: Some(bar),
        throughput_bps: 5,
    };
    assert_eq!(
        upload_total(sending),
        Some(BridgeFastValue::UploadTotal {
            transfer: crate::types::BridgeUploadTransfer {
                bar: Some(crate::types::BridgeUploadBar {
                    bytes_done: 10,
                    bytes_total: 40,
                }),
                throughput_bps: 5,
            },
        })
    );
    assert_eq!(
        upload_total(bae_core::library::UploadTransfer {
            bar: None,
            throughput_bps: 0,
        }),
        None
    );
}
