import BaeKit
import SwiftUI

/// Adds another of this person's devices: the QR code the new device scans,
/// and the person's restore code to type there instead. `result` is nil while
/// the presenter generates them. While it shows the code, it runs the watch
/// that came with it, and once a new device joins it says so in place of the
/// code.
struct AddDeviceSheet: View {
    @Environment(MembershipStore.self)
    private var store
    @Binding
    var result: Result<BridgeAddDevice, Error>?
    let onDismiss: () -> Void

    var body: some View {
        AddDeviceSheetContent(
            result: result,
            addedDevice: store.addedDevice,
            onDismiss: onDismiss
        )
        .task(id: watch.map(ObjectIdentifier.init)) {
            guard let watch else { return }
            await store.watchForAddedDevice(watch)
        }
    }

    /// The watch for the device that joins from the code shown.
    private var watch: BridgeAddedDeviceWatch? {
        guard case .success(let adding) = result else { return nil }
        return adding.watch
    }
}

/// The sheet's face: the device that joined, `addedDevice`, once one has;
/// until then the code to scan or type, a spinner while it generates, or why
/// it couldn't.
struct AddDeviceSheetContent: View {
    let result: Result<BridgeAddDevice, Error>?
    let addedDevice: BridgeActiveDevice?
    let onDismiss: () -> Void

    var body: some View {
        MacSheet("Add a device", width: 440) {
            content
        } footer: {
            MacSheetFooter {
                Button("Done", action: onDismiss)
                    .buttonStyle(PrimaryButtonStyle())
                    .keyboardShortcut(.defaultAction)
            }
        }
    }

    @ViewBuilder
    private var content: some View {
        if let device = addedDevice {
            Text("\(device.name) added")
                .themeText(.body)
        }
        else {
            switch result {
            case nil:
                MacSpinner("Making a code for your new device…")
            case .success(let adding):
                // Copy and Share hand out the restore code; the QR code also
                // holds this device's S3 key, which a device that scans it
                // then doesn't ask for.
                MacScanCode(
                    instruction: "Scan or enter the code on the new device.",
                    qr: adding.share.qrText,
                    code: adding.share.code
                )
            case .failure(let error):
                // The presenter only records failures that have a line.
                if let shown = DisplayError(error) {
                    Notice(error: shown)
                }
            }
        }
    }
}
