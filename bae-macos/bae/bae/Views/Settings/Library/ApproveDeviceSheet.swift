import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("ApproveDevice")

/// Owner-side flow for adding a device. It displays the only pairing code,
/// waits for the joining device's signed identity, and admits that exact device
/// after the owner reviews it.
struct ApproveDeviceSheet: View {
    let sync: Sync
    let onDismiss: () -> Void
    let onApproved: () -> Void

    private enum Step {
        case starting
        case waiting(BridgeDevicePairingSession)
        case confirm(BridgeDevicePairingSession, BridgePairingDevice)
        case approving(BridgeDevicePairingSession, BridgePairingDevice)
    }

    @State
    private var step: Step = .starting
    @State
    private var error: String?
    @State
    private var pairingTask: Task<Void, Never>?
    @State
    private var completed = false
    @State
    private var activeSession: BridgeDevicePairingSession?

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text("Add a device")
                    .font(.headline)
                Spacer()
                Button("Done") { onDismiss() }
                    .buttonStyle(.borderless)
                    .disabled(isApproving)
            }
            .padding()

            Divider()

            content
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .frame(width: 400, height: 480)
        .task { await startPairing() }
        .onDisappear { cancelPairing() }
        .interactiveDismissDisabled(isApproving)
    }

    private var isApproving: Bool {
        if case .approving = step { return true }
        return false
    }

    @ViewBuilder
    private var content: some View {
        switch step {
        case .starting:
            ProgressView("Starting pairing...")
        case .waiting(let session):
            waitingStep(session)
        case .confirm(let session, let device):
            confirmStep(session, device)
        case .approving:
            ProgressView("Approving device...")
        }
    }

    private func waitingStep(_ session: BridgeDevicePairingSession) -> some View
    {
        VStack(spacing: 16) {
            Text("Scan this code on the device joining your library.")
                .font(.callout)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)

            CodeDisplay(code: session.code(), qrSize: 220)

            HStack(spacing: 8) {
                ProgressView()
                    .controlSize(.small)
                Text("Waiting for the device...")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            if let error {
                Text(error)
                    .foregroundStyle(.red)
                    .font(.caption)
                Button("Try again") {
                    self.error = nil
                    pairingTask?.cancel()
                    pairingTask = Task { await waitForDevice(session) }
                }
            }
        }
        .padding()
    }

    private func confirmStep(
        _ session: BridgeDevicePairingSession,
        _ device: BridgePairingDevice
    ) -> some View {
        VStack(spacing: 16) {
            Spacer()
            Image(systemName: "laptopcomputer.and.iphone")
                .font(.largeTitle)
                .foregroundStyle(.secondary)
            Text("Approve this device?")
                .font(.headline)
            Text(device.fingerprint)
                .font(.system(.body, design: .monospaced))
            if let email = device.email {
                Text(email)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Text(
                "Check that this matches the fingerprint shown on the new device before approving."
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
                Button("Cancel") { onDismiss() }
                    .buttonStyle(.bordered)
                Button("Approve") { approve(session, device) }
                    .buttonStyle(.borderedProminent)
                    .keyboardShortcut(.defaultAction)
            }
            Spacer()
        }
        .padding()
    }

    private func startPairing() async {
        do {
            let session = try await sync.startDevicePairing()
            try Task.checkCancellation()
            activeSession = session
            step = .waiting(session)
            pairingTask = Task { await waitForDevice(session) }
        }
        catch is CancellationError {
            logger.debug("device pairing start cancelled")
        }
        catch {
            logger.error(
                "Failed to start device pairing: \(error.localizedDescription)"
            )
            self.error = error.displayLine
        }
    }

    private func waitForDevice(_ session: BridgeDevicePairingSession) async {
        do {
            let device = try await session.waitForDevice()
            try Task.checkCancellation()
            step = .confirm(session, device)
        }
        catch is CancellationError {
            logger.debug("device pairing wait cancelled")
        }
        catch {
            logger.error(
                "Failed while waiting for pairing device: \(error.localizedDescription)"
            )
            self.error = error.displayLine
        }
    }

    private func approve(
        _ session: BridgeDevicePairingSession,
        _ device: BridgePairingDevice
    ) {
        error = nil
        step = .approving(session, device)
        pairingTask?.cancel()
        pairingTask = Task {
            do {
                try await session.approve()
                try Task.checkCancellation()
                completed = true
                activeSession = nil
                onApproved()
                onDismiss()
            }
            catch is CancellationError {
                logger.debug("device pairing approval cancelled")
            }
            catch {
                logger.error(
                    "Failed to approve paired device: \(error.localizedDescription)"
                )
                self.error = error.displayLine
                step = .confirm(session, device)
            }
        }
    }

    private func cancelPairing() {
        pairingTask?.cancel()
        guard !completed else { return }
        guard let session = activeSession else { return }
        do {
            try session.cancel()
        }
        catch {
            logger.error(
                "Failed to cancel device pairing: \(error.localizedDescription)"
            )
        }
    }
}

#if DEBUG
    #Preview("Add a device") {
        ApproveDeviceSheet(sync: .stub(), onDismiss: {}, onApproved: {})
    }
#endif
