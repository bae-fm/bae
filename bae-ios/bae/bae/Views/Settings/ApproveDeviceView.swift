import BaeKit
import Foundation
import SwiftUI
import UIKit
import os.log

private let logger = Logger.bae("ApproveDevice")

/// Owner-side flow for adding a device: read the joining device's join-request
/// code (camera scan or pasted text), preview its public key, approve it, then
/// show the sealed invitation while both devices complete the join.
struct ApproveDeviceView: View {
    let sync: Sync
    let onDismiss: () -> Void
    /// Called once a device has been approved, so the caller can refresh its
    /// member list.
    let onApproved: () -> Void

    private struct JoinRequestSelection {
        let code: String
        let info: BridgeJoinRequestInfo
    }

    private struct DeviceInvitation {
        let bytes: Data
        let code: String
    }

    private enum Step {
        case capture
        case confirm(JoinRequestSelection)
        case inviting(JoinRequestSelection)
        case invited(DeviceInvitation)
    }

    @State
    private var step: Step = .capture
    @State
    private var pasteInput = ""
    @State
    private var error: String?
    @State
    private var inviteTask: Task<Void, Never>?
    @State
    private var showScanner = false
    @State
    private var isPresented = false

    var body: some View {
        NavigationStack {
            content
                .padding()
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .navigationTitle("Add a device")
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .confirmationAction) {
                        Button("Done") { onDismiss() }
                    }
                }
        }
        .onAppear { isPresented = true }
        .onDisappear { withdrawInvitation() }
        .fullScreenCover(isPresented: $showScanner) {
            scannerSheet
        }
    }

    @ViewBuilder
    private var content: some View {
        switch step {
        case .capture:
            captureStep
        case .confirm(let request):
            confirmStep(request)
        case .inviting:
            VStack(spacing: 12) {
                ProgressView()
                Text("Approving device\u{2026}")
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }
        case .invited(let invitation):
            invitedStep(invitation)
        }
    }

    // MARK: - Capture

    private var captureStep: some View {
        VStack(spacing: 16) {
            Text(
                "On the new device, open Join a library and show its code. Scan it here, or paste it below."
            )
            .font(.callout)
            .foregroundStyle(.secondary)
            .multilineTextAlignment(.center)

            Button {
                error = nil
                CameraPermission.requestThenScan(
                    present: { showScanner = true },
                    onError: { error = $0 }
                )
            } label: {
                Label("Scan code", systemImage: "qrcode.viewfinder")
                    .frame(maxWidth: 240)
            }
            .buttonStyle(.borderedProminent)

            VStack(alignment: .leading, spacing: 8) {
                Text("Or paste the device's code")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                TextField(
                    "Paste the device's code",
                    text: $pasteInput,
                    axis: .vertical
                )
                .textFieldStyle(.roundedBorder)
                .font(.body.monospaced())
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .lineLimit(3, reservesSpace: true)
                Button("Decode") { decode(pasteInput) }
                    .buttonStyle(.bordered)
                    .disabled(
                        pasteInput.trimmingCharacters(in: .whitespacesAndNewlines)
                            .isEmpty
                    )
            }

            if let error {
                Text(error)
                    .foregroundStyle(.red)
                    .font(.callout)
                    .multilineTextAlignment(.center)
            }
            Spacer()
        }
    }

    // MARK: - Confirm

    private func confirmStep(_ request: JoinRequestSelection) -> some View {
        VStack(spacing: 16) {
            Spacer()
            Image(systemName: "iphone.and.arrow.forward")
                .font(.largeTitle)
                .foregroundStyle(.secondary)
            VStack(spacing: 4) {
                Text("Approve this device?")
                    .font(.headline)
                Text(request.info.fingerprint)
                    .font(.system(.body, design: .monospaced))
                if let email = request.info.email {
                    Text(email)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            Text(
                "It will be added to your library and able to sync. You'll get a code to enter on it."
            )
            .font(.caption)
            .foregroundStyle(.secondary)
            .multilineTextAlignment(.center)

            if let error {
                Text(error)
                    .foregroundStyle(.red)
                    .font(.caption)
            }

            HStack(spacing: 12) {
                Button("Back") {
                    error = nil
                    step = .capture
                }
                .buttonStyle(.bordered)
                Button("Approve") { approve(request) }
                    .buttonStyle(.borderedProminent)
            }
            Spacer()
        }
    }

    // MARK: - Invited

    private func invitedStep(_ invitation: DeviceInvitation) -> some View {
        VStack(spacing: 16) {
            Spacer()
            Text("Enter this code on the new device.")
                .font(.callout)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)

            CodeShareBlock(
                code: invitation.code,
                contentDescription: "Invite code"
            )
            if let error {
                Text(error)
                    .foregroundStyle(.red)
                    .font(.caption)
                Button("Retry") { retry(invitation) }
            }
            Spacer()
        }
    }

    // MARK: - Scanner

    private var scannerSheet: some View {
        ZStack(alignment: .topTrailing) {
            QRScannerView(
                onScanned: { code in
                    showScanner = false
                    decode(code)
                },
                onError: { message in
                    showScanner = false
                    error = message
                }
            )
            .ignoresSafeArea()
            Button {
                showScanner = false
            } label: {
                Image(systemName: "xmark.circle.fill")
                    .font(.title)
                    .foregroundStyle(.white)
                    .padding()
            }
        }
    }
}

// MARK: - Actions

private extension ApproveDeviceView {
    private func decode(_ raw: String) {
        let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else {
            logger.warning("scanned join-request code was empty after trimming")
            return
        }
        do {
            let info = try decodeJoinRequest(code: trimmed)
            error = nil
            step = .confirm(JoinRequestSelection(code: trimmed, info: info))
        }
        catch {
            logger.error(
                "Failed to decode join request: \(error.localizedDescription)"
            )
            self.error = error.displayLine
        }
    }

    private func approve(_ request: JoinRequestSelection) {
        error = nil
        step = .inviting(request)
        inviteTask?.cancel()
        inviteTask = Task { @MainActor in
            do {
                let bytes = try await sync.beginDeviceInvite(request.code)
                let invitation = DeviceInvitation(
                    bytes: bytes,
                    code: bytes.base64EncodedString()
                )
                guard isPresented else {
                    try await sync.cancelDeviceInvite(bytes)
                    return
                }
                step = .invited(invitation)
                await drive(invitation)
            }
            catch is CancellationError {
                logger.debug("device approval cancelled")
            }
            catch {
                logger.error(
                    "Failed to approve device: \(error.localizedDescription)"
                )
                if isPresented {
                    self.error = error.displayLine
                    step = .confirm(request)
                }
            }
        }
    }

    private func retry(_ invitation: DeviceInvitation) {
        error = nil
        inviteTask?.cancel()
        inviteTask = Task { @MainActor in await drive(invitation) }
    }

    private func drive(_ invitation: DeviceInvitation) async {
        do {
            try await sync.driveDeviceJoin(invitation.bytes)
            try Task.checkCancellation()
            guard isPresented else { return }
            step = .capture
            onApproved()
            onDismiss()
        }
        catch is CancellationError {
            logger.debug("device join driver cancelled")
        }
        catch {
            logger.error(
                "Failed to finish device join: \(error.localizedDescription)"
            )
            if isPresented {
                self.error = error.displayLine
                step = .invited(invitation)
            }
        }
    }

    private func withdrawInvitation() {
        isPresented = false
        guard case .invited(let invitation) = step else { return }
        inviteTask?.cancel()
        Task {
            do {
                try await sync.cancelDeviceInvite(invitation.bytes)
            }
            catch {
                logger.error(
                    "Failed to withdraw device invitation: \(error.localizedDescription)"
                )
            }
        }
    }
}

#if DEBUG
#Preview {
    ApproveDeviceView(
        sync: .stub(),
        onDismiss: {},
        onApproved: {}
    )
}
#endif
