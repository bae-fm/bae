import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("ApproveDevice")

/// Owner-side flow for adding a device: read the joining device's join-request
/// code (camera scan or pasted text), preview its public key, approve it, then
/// show the invite code to carry back to that device.
///
/// `invite` is `Sync.inviteMember`: it takes the joining device's public key
/// plus any provider account email and returns the invite code. The sheet drives
/// a small step machine so each stage renders only what that stage needs.
struct ApproveDeviceSheet: View {
    let invite:
        @Sendable (_ publicKeyHex: String, _ providerAccountEmail: String?)
            async throws -> String
    let onDismiss: () -> Void
    /// Called once a device has been approved, so the caller can refresh its
    /// member list.
    let onApproved: () -> Void

    /// Where the flow is. `capture` reads the join-request code; `confirm`
    /// previews the decoded device; `inviting` runs the approval; `invited`
    /// shows the resulting invite code.
    private enum Step {
        case capture
        case confirm(BridgeJoinRequestInfo)
        case inviting(BridgeJoinRequestInfo)
        case invited(code: String)
    }

    @State
    private var step: Step = .capture
    @State
    private var pasteInput = ""
    @State
    private var providerAccountEmail = ""
    @State
    private var error: String?
    @State
    private var inviteTask: Task<Void, Never>?

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text("Add a device")
                    .font(.headline)
                Spacer()
                Button("Done") { onDismiss() }
                    .buttonStyle(.borderless)
            }
            .padding()

            Divider()

            content
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .frame(width: 400, height: 480)
        .onDisappear { inviteTask?.cancel() }
    }

    @ViewBuilder
    private var content: some View {
        switch step {
        case .capture:
            captureStep
        case .confirm(let info):
            confirmStep(info)
        case .inviting:
            VStack(spacing: 12) {
                ProgressView()
                Text("Approving device...")
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }
        case .invited(let code):
            invitedStep(code)
        }
    }

    // MARK: - Capture

    private var captureStep: some View {
        VStack(spacing: 12) {
            Text(
                "On the new device, open Join a library and show its code. Scan it here, or paste it below."
            )
            .font(.callout)
            .foregroundStyle(.secondary)
            .multilineTextAlignment(.center)
            .padding(.horizontal)

            QRScannerView(onScan: { decode($0) })
                .frame(height: 200)
                .clipShape(RoundedRectangle(cornerRadius: 8))

            HStack(spacing: 8) {
                TextField("Paste the device's code", text: $pasteInput)
                    .textFieldStyle(.roundedBorder)
                    .font(.system(.caption, design: .monospaced))
                    .onSubmit { decode(pasteInput) }
                Button("Decode") { decode(pasteInput) }
                    .disabled(
                        pasteInput.trimmingCharacters(in: .whitespaces).isEmpty
                    )
            }

            if let error {
                Text(error)
                    .foregroundStyle(.red)
                    .font(.caption)
            }
        }
        .padding()
    }

    // MARK: - Confirm

    private func confirmStep(_ info: BridgeJoinRequestInfo) -> some View {
        VStack(spacing: 16) {
            Spacer()
            Image(systemName: "laptopcomputer.and.iphone")
                .font(.largeTitle)
                .foregroundStyle(.secondary)
            VStack(spacing: 4) {
                Text("Approve this device?")
                    .font(.headline)
                Text(info.fingerprint)
                    .font(.system(.body, design: .monospaced))
                if let email = info.email {
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
            .padding(.horizontal)

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
                Button("Approve") { approve(info) }
                    .buttonStyle(.borderedProminent)
                    .keyboardShortcut(.defaultAction)
            }
            Spacer()
        }
        .padding()
    }

    // MARK: - Invited

    private func invitedStep(_ code: String) -> some View {
        VStack(spacing: 16) {
            Spacer()
            Text("Enter this code on the new device.")
                .font(.callout)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)

            CodeDisplay(code: code, qrSize: 180)
            Spacer()
        }
        .padding()
    }

    // MARK: - Actions

    private func decode(_ raw: String) {
        let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        do {
            let info = try decodeJoinRequest(code: trimmed)
            providerAccountEmail = info.email ?? ""
            error = nil
            step = .confirm(info)
        }
        catch {
            logger.error(
                "Failed to decode join request: \(error.localizedDescription)"
            )
            self.error = error.localizedDescription
        }
    }

    private func approve(_ info: BridgeJoinRequestInfo) {
        let pubkey = info.pubkey
        let email = providerAccountEmail.trimmingCharacters(
            in: .whitespacesAndNewlines
        )
        error = nil
        step = .inviting(info)
        inviteTask?.cancel()
        inviteTask = Task { @MainActor in
            do {
                let code = try await invite(pubkey, email.isEmpty ? nil : email)
                step = .invited(code: code)
                onApproved()
            }
            catch is CancellationError {
                logger.debug("device approval cancelled")
            }
            catch {
                logger.error(
                    "Failed to approve device: \(error.localizedDescription)"
                )
                self.error = error.localizedDescription
                step = .confirm(info)
            }
        }
    }
}
