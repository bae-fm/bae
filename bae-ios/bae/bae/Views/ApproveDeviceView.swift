import BaeKit
import SwiftUI
import UIKit
import os.log

private let logger = Logger.bae("ApproveDevice")

/// Owner-side flow for adding a device: read the joining device's join-request
/// code (camera scan or pasted text), preview its public key, approve it, then
/// show the invite code to carry back to that device.
///
/// `invite` is `Sync.inviteMember`: it takes the joining device's public key
/// plus any provider account email and returns the invite code. The sheet
/// drives a small step machine so each stage renders only what that stage
/// needs. Presented as a sheet from `MembersView`.
struct ApproveDeviceView: View {
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
    @State
    private var showScanner = false

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
        .onDisappear { inviteTask?.cancel() }
        .fullScreenCover(isPresented: $showScanner) {
            scannerSheet
        }
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
                Text("Approving device\u{2026}")
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }
        case .invited(let code):
            invitedStep(code)
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

    private func confirmStep(_ info: BridgeJoinRequestInfo) -> some View {
        VStack(spacing: 16) {
            Spacer()
            Image(systemName: "iphone.and.arrow.forward")
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
            }
            Spacer()
        }
    }

    // MARK: - Invited

    private func invitedStep(_ code: String) -> some View {
        VStack(spacing: 16) {
            Spacer()
            Text("Enter this code on the new device.")
                .font(.callout)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)

            CodeShareBlock(
                code: code,
                contentDescription: "Invite code"
            )
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

    // MARK: - Actions

    private func decode(_ raw: String) {
        let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else {
            logger.warning("scanned join-request code was empty after trimming")
            return
        }
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
