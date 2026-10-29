import BaeKit
import SwiftUI

/// Adds another of this person's devices: the QR code the new device scans,
/// and the person's recovery code to type there instead. `result` is nil while
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

    @ViewBuilder
    private var codeContent: some View {
        switch result {
        case nil:
            VStack {
                Spacer()
                ProgressView()
                Spacer()
            }
        case .success(let adding):
            ScrollView {
                shareContent(adding.share)
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

    /// Holds the result the presenter normally writes after the sheet is up.
    private struct AddDeviceSheetPreview: View {
        @State
        var result: Result<BridgeAddDevice, Error>?

        var body: some View {
            AddDeviceSheet(result: $result, onDismiss: {})
        }

        /// A code to show, with a watch for no device in particular.
        static func adding(_ share: BridgeRestoreCodeShare) -> Result<
            BridgeAddDevice, Error
        > {
            .success(
                BridgeAddDevice(
                    share: share,
                    watch: BridgeAddedDeviceWatch(
                        noHandle: BridgeAddedDeviceWatch.NoHandle()
                    )
                )
            )
        }

        /// The person's devices, where `addedDevice` has joined.
        static func members(addedDevice: BridgeActiveDevice? = nil)
            -> MembershipStore
        {
            MembershipStore.stub(
                people: PreviewData.members,
                selfIsAdmin: true,
                addedDevice: addedDevice
            )
        }
    }

    // The environment sits on each #Preview root because the missing-environment
    // audit reads only the preview closure's modifier chain.
    #Preview("Loading") {
        AddDeviceSheetPreview(result: nil)
            .environment(AddDeviceSheetPreview.members())
    }

    #Preview("Code") {
        AddDeviceSheetPreview(
            result: AddDeviceSheetPreview.adding(
                BridgeRestoreCodeShare(
                    code: PreviewData.recoveryCode,
                    qrText: PreviewData.recoveryCode,
                    qrCarriesS3Key: false
                )
            )
        )
        .environment(AddDeviceSheetPreview.members())
    }

    #Preview("Code with S3 key") {
        AddDeviceSheetPreview(
            result: AddDeviceSheetPreview.adding(
                BridgeRestoreCodeShare(
                    code: PreviewData.recoveryCode,
                    qrText: "\(PreviewData.recoveryCode).s3-key-preview",
                    qrCarriesS3Key: true
                )
            )
        )
        .environment(AddDeviceSheetPreview.members())
    }

    #Preview("Device added") {
        AddDeviceSheetPreview(
            result: AddDeviceSheetPreview.adding(
                BridgeRestoreCodeShare(
                    code: PreviewData.recoveryCode,
                    qrText: PreviewData.recoveryCode,
                    qrCarriesS3Key: false
                )
            )
        )
        .environment(
            AddDeviceSheetPreview.members(
                addedDevice: BridgeActiveDevice(
                    id: 7,
                    name: "New Phone",
                    isSelf: false,
                    canRemove: true
                )
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
        .environment(AddDeviceSheetPreview.members())
    }
#endif
