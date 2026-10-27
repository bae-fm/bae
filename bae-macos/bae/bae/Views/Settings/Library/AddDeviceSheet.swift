import BaeKit
import SwiftUI

/// Adds another of this person's devices: the QR code the new device scans,
/// and the person's recovery code to type there instead. `result` is nil while
/// the presenter generates them. While it shows, it watches the person's
/// devices, and once a new one joins it says so in place of the code.
struct AddDeviceSheet: View {
    @Environment(MembershipStore.self)
    private var store
    @Binding
    var result: Result<BridgeRestoreCodeShare, Error>?
    let onDismiss: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text("Add a device")
                    .themeText(.heading)
                Spacer()
                Button("Done") { onDismiss() }
                    .buttonStyle(.borderless)
            }
            .padding()

            Divider()

            if let device = store.addedDevice {
                VStack {
                    Spacer()
                    Text("\(device.name) added")
                        .themeText(.heading)
                        .multilineTextAlignment(.center)
                    Spacer()
                }
                .padding()
            }
            else {
                codeContent
            }
        }
        .frame(width: 400, height: 600)
        .task { await store.watchForAddedDevice() }
    }

    @ViewBuilder
    private var codeContent: some View {
        switch result {
        case nil:
            VStack {
                Spacer()
                ProgressView()
                Spacer()
            }
        case .success(let share):
            ScrollView {
                shareContent(share)
                    .padding()
            }
        case .failure(let error):
            VStack {
                Spacer()
                // The presenter only records failures that have a line.
                if let shown = DisplayError(error) {
                    ErrorDetailDisclosure(error: shown)
                }
                Spacer()
            }
            .padding()
        }
    }

    private func shareContent(_ share: BridgeRestoreCodeShare) -> some View {
        VStack(spacing: ThemeSpace.edge) {
            QRCode(text: share.qrText)
                .accessibilityLabel("Your recovery code")

            Text(
                "On the new device, choose “Add device”, then scan this or type:"
            )
            .themeText(.detail)
            .foregroundStyle(.secondary)
            .multilineTextAlignment(.center)

            Text(share.code)
                .themeText(.mono)
                .textSelection(.enabled)
                .multilineTextAlignment(.center)

            Button("Copy") {
                SystemActions.copyToPasteboard(share.code)
            }

            if share.qrCarriesS3Key {
                Text(
                    "The QR code also carries your S3 access key, so a device that scans it won't ask for the key."
                )
                .themeText(.detail)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
            }
        }
        .frame(maxWidth: .infinity)
    }
}

#if DEBUG
    // MARK: - Previews

    /// Holds the result the presenter normally writes after the sheet is up,
    /// over the person's devices, where `addedDevice` has joined.
    private struct AddDeviceSheetPreview: View {
        @State
        var result: Result<BridgeRestoreCodeShare, Error>?
        var addedDevice: BridgeActiveDevice?

        var body: some View {
            AddDeviceSheet(result: $result, onDismiss: {})
                .environment(
                    MembershipStore.stub(
                        people: PreviewData.members,
                        selfIsAdmin: true,
                        addedDevice: addedDevice
                    )
                )
        }
    }

    #Preview("Loading") {
        AddDeviceSheetPreview(result: nil)
    }

    #Preview("Code") {
        AddDeviceSheetPreview(
            result: .success(
                BridgeRestoreCodeShare(
                    code: PreviewData.recoveryCode,
                    qrText: PreviewData.recoveryCode,
                    qrCarriesS3Key: false
                )
            )
        )
    }

    #Preview("Code with S3 key") {
        AddDeviceSheetPreview(
            result: .success(
                BridgeRestoreCodeShare(
                    code: PreviewData.recoveryCode,
                    qrText: "\(PreviewData.recoveryCode).s3-key-preview",
                    qrCarriesS3Key: true
                )
            )
        )
    }

    #Preview("Device added") {
        AddDeviceSheetPreview(
            result: .success(
                BridgeRestoreCodeShare(
                    code: PreviewData.recoveryCode,
                    qrText: PreviewData.recoveryCode,
                    qrCarriesS3Key: false
                )
            ),
            addedDevice: BridgeActiveDevice(
                id: 7,
                name: "New Phone",
                isSelf: false,
                canRemove: true
            )
        )
    }

    #Preview("Failed") {
        AddDeviceSheetPreview(
            result: .failure(
                NSError(
                    domain: "preview",
                    code: 1,
                    userInfo: [
                        NSLocalizedDescriptionKey:
                            "Couldn't generate a recovery code."
                    ],
                )
            )
        )
    }
#endif
