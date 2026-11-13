import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("AddDeviceSheet")

/// Adds another of this person's devices: the QR code the new device scans
/// and the code to type there instead. `result` is nil while the presenter
/// starts the pairing. While it shows the code, it follows the pairing's
/// status: a new device's request replaces the code with the number to
/// compare, and once the device is in the sheet says so. Closing it ends the
/// pairing.
struct AddDeviceSheet: View {
    @Environment(MembershipStore.self)
    private var store
    @Environment(Sync.self)
    private var sync
    @Binding
    var result: Result<BridgeAddDevice, Error>?
    let onDismiss: () -> Void
    @State
    private var task: Task<Void, Never>?
    @State
    private var error: DisplayError?

    var body: some View {
        AddDeviceSheetContent(
            result: result,
            face: store.addDeviceStatus?.face ?? .code,
            busy: task != nil,
            error: error,
            onApprove: { decide(sync.addDevice.approve) },
            onDecline: { decide(sync.addDevice.decline) },
            onDismiss: onDismiss
        )
        .task(id: adding?.id) {
            guard let adding else { return }
            await store.followAddDevice(adding.id)
        }
        .onChange(of: store.addDeviceStatus?.face) { _, face in
            if face == .declined { onDismiss() }
        }
        .onDisappear { end() }
    }

    /// The pairing the code shown belongs to.
    private var adding: BridgeAddDevice? {
        guard case .success(let adding) = result else { return nil }
        return adding
    }

    /// Approve or decline the request the status shows.
    private func decide(
        _ decision:
            @escaping @Sendable (BridgeAddDeviceRequest) async throws -> Void
    ) {
        guard case .waiting(let request?) = store.addDeviceStatus else {
            return
        }
        error = nil
        task = Task {
            defer { task = nil }
            do { try await decision(request) }
            catch {
                logger.error(
                    "Add device decision failed: \(String(reflecting: error))"
                )
                self.error = DisplayError(error)
            }
        }
    }

    /// The sheet closed: end the pairing, whatever its status.
    private func end() {
        guard let adding else { return }
        let end = sync.addDevice.end
        Task {
            do { try await end(adding.id) }
            catch {
                logger.error(
                    "Ending the pairing failed: \(String(reflecting: error))"
                )
            }
        }
    }
}

/// The sheet's face, by where the pairing is: a new device's request, to
/// add when its screen shows the number; the name of the device that was
/// added; or until then the code to scan or type, a spinner while it starts,
/// or why it couldn't.
struct AddDeviceSheetContent: View {
    let result: Result<BridgeAddDevice, Error>?
    let face: AddDeviceFace
    let busy: Bool
    let error: DisplayError?
    let onApprove: () -> Void
    let onDecline: () -> Void
    let onDismiss: () -> Void

    var body: some View {
        if case .request(let deviceName, let confirmation) = face {
            MacSheet(
                "\(deviceName) wants to join",
                message: "Add it if its screen shows this code:",
                width: 440
            ) {
                VStack(alignment: .leading, spacing: ThemeSpace.group) {
                    MacMatchCode(code: confirmation, dialog: true)
                    if let error {
                        Notice(error: error)
                    }
                }
            } footer: {
                MacSheetFooter {
                    Button("Decline", action: onDecline)
                        .disabled(busy)
                    Button("Add", action: onApprove)
                        .buttonStyle(PrimaryButtonStyle())
                        .keyboardShortcut(.defaultAction)
                        .disabled(busy)
                }
            }
        }
        else {
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
    }

    @ViewBuilder
    private var content: some View {
        switch face {
        case .added(let name):
            Text("\(name) added")
                .themeText(.body)
        case .adding(let name):
            MacSpinner("Adding \(name)…")
        case .expired:
            Text(
                "This code has ended. Close this and choose Add device again for a new one."
            )
            .themeText(.body)
        case .refused:
            Text(
                "The new device’s request was refused, so it was not added. Try again with a new code."
            )
            .themeText(.body)
        case .code, .request, .declined:
            switch result {
            case nil:
                MacSpinner("Making a code for your new device…")
            case .success(let adding):
                // Copy and Share hand out the code to type; the QR code holds
                // the key on S3, so a device that scans it asks for none.
                MacScanCode(
                    instruction: "Scan or enter the code on the new device.",
                    qr: adding.qrText,
                    code: adding.code
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
