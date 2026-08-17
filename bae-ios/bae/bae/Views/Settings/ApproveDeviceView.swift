import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("ApproveDevice")

struct ApproveDeviceView: View {
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
    private var activeSession: BridgeDevicePairingSession?
    @State
    private var pairingTask: Task<Void, Never>?
    @State
    private var error: String?
    @State
    private var completed = false

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
                            .disabled(isApproving)
                    }
                }
        }
        .task { await startPairing() }
        .onDisappear { cancelPairing() }
        .interactiveDismissDisabled(isApproving)
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

    private var isApproving: Bool {
        if case .approving = step { return true }
        return false
    }

    private func waitingStep(_ session: BridgeDevicePairingSession) -> some View {
        VStack(spacing: 16) {
            Text("Scan this code on the device joining your library.")
                .font(.callout)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)

            CodeShareBlock(
                code: session.code(),
                contentDescription: "Device pairing code",
                qrSize: 240
            )

            HStack(spacing: 8) {
                ProgressView()
                Text("Waiting for the device...")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            if let error {
                Text(error)
                    .foregroundStyle(.red)
                    .font(.callout)
                Button("Try again") {
                    self.error = nil
                    pairingTask?.cancel()
                    pairingTask = Task { await waitForDevice(session) }
                }
            }
        }
    }

    private func confirmStep(
        _ session: BridgeDevicePairingSession,
        _ device: BridgePairingDevice
    ) -> some View {
        VStack(spacing: 16) {
            Spacer()
            Image(systemName: "iphone.and.arrow.forward")
                .font(.largeTitle)
                .foregroundStyle(.secondary)
            Text("Approve this device?")
                .font(.headline)
            Text(device.fingerprint)
                .font(.body.monospaced())
            if let email = device.email {
                Text(email)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Text("Check that this matches the fingerprint shown on the new device before approving.")
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
            }
            Spacer()
        }
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
            logger.error("Failed to start device pairing: \(error.localizedDescription)")
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
            logger.error("Failed while waiting for pairing device: \(error.localizedDescription)")
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
                completed = true
                activeSession = nil
                onApproved()
                onDismiss()
            }
            catch {
                logger.error("Failed to approve paired device: \(error.localizedDescription)")
                self.error = error.displayLine
                step = .confirm(session, device)
            }
        }
    }

    private func cancelPairing() {
        pairingTask?.cancel()
        guard !completed, let session = activeSession else { return }
        do {
            try session.cancel()
        }
        catch {
            logger.error("Failed to cancel device pairing: \(error.localizedDescription)")
        }
    }
}

#if DEBUG
#Preview {
    ApproveDeviceView(sync: .stub(), onDismiss: {}, onApproved: {})
}
#endif
