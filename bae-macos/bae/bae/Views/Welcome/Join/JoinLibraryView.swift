import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("JoinLibraryView")

/// Add this device by scanning the one pairing code shown on an existing
/// device. The scanned offer selects the provider; OAuth, when required, starts
/// from that authoritative provider before the signed pairing request is sent.
struct JoinLibraryView: View {
    let onLibraryReady: (BridgeLibrary) -> Void
    let onBack: () -> Void

    @Environment(LibrarySetup.self)
    private var setup

    @State
    private var pairingCodeInput = ""
    @State
    private var decodedOffer: Result<BridgeDevicePairingOffer, Error>?
    @State
    private var oauthTokenJson: String?
    @State
    private var authorizationTask: Task<Void, Never>?
    @State
    private var joinTask: Task<Void, Never>?
    @State
    private var showScanner = false
    @State
    private var isAuthorizing = false
    @State
    private var isJoining = false
    @State
    private var joiningFingerprint: String?
    @State
    private var error: String?

    var body: some View {
        VStack(spacing: 0) {
            Text("Join a library")
                .font(.title2.bold())
                .padding(.top, 24)
                .padding(.bottom, 4)
            Text(
                "Scan the pairing code shown on a device already in your library."
            )
            .font(.callout)
            .foregroundStyle(.secondary)
            .padding(.bottom, 16)

            JoinPairingOffer(
                pairingCodeInput: $pairingCodeInput,
                decodedOffer: decodedOffer,
                isAuthorizing: isAuthorizing,
                isJoining: isJoining,
                joiningFingerprint: joiningFingerprint,
                error: error,
                joinReady: joinReady,
                onScan: { showScanner = true },
                onJoin: { join() },
                onBack: onBack
            )
        }
        .padding(.horizontal)
        .onChange(of: pairingCodeInput) { _, input in
            decodePairingOffer(input)
        }
        .onDisappear {
            authorizationTask?.cancel()
            joinTask?.cancel()
            #if BAE_OAUTH_PROVIDERS
                setup.oauthCancel()
            #endif
        }
        .sheet(isPresented: $showScanner) {
            PairingScannerSheet(
                onScan: { code in
                    pairingCodeInput = code
                    showScanner = false
                },
                onDismiss: { showScanner = false }
            )
        }
    }

    private var joinReady: Bool {
        guard case .success(let offer) = decodedOffer else { return false }
        return !offer.needsOauth || oauthTokenJson != nil
    }

    private func decodePairingOffer(_ input: String) {
        authorizationTask?.cancel()
        #if BAE_OAUTH_PROVIDERS
            setup.oauthCancel()
        #endif
        oauthTokenJson = nil
        isAuthorizing = false
        error = nil

        let code = input.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !code.isEmpty else {
            decodedOffer = nil
            return
        }

        let decoded = Result { try setup.decodeDevicePairingOffer(code) }
        decodedOffer = decoded
        guard case .success(let offer) = decoded, offer.needsOauth else {
            return
        }
        authorize(offer.cloudProvider)
    }

    private func authorize(_ provider: BridgeCloudProvider) {
        #if BAE_OAUTH_PROVIDERS
            isAuthorizing = true
            let authorize = setup.oauthAuthorize
            authorizationTask = Task {
                do {
                    let token = try await DetachedWork.run {
                        try authorize(provider)
                    }
                    try Task.checkCancellation()
                    oauthTokenJson = token
                    isAuthorizing = false
                }
                catch is CancellationError {
                    logger.debug("pairing authorization cancelled")
                }
                catch {
                    isAuthorizing = false
                    self.error = error.displayLine
                }
            }
        #endif
    }

    private func join() {
        guard case .success = decodedOffer else { return }
        let code = pairingCodeInput.trimmingCharacters(
            in: .whitespacesAndNewlines
        )
        let token = oauthTokenJson
        joinTask?.cancel()
        isJoining = true
        error = nil
        joinTask = Task {
            let operation: JoinOperation
            do {
                operation = try await setup.joinDevicePairing(code, token)
                try Task.checkCancellation()
                joiningFingerprint = operation.fingerprint
            }
            catch {
                isJoining = false
                self.error = error.displayLine
                return
            }
            do {
                let detached = Task.detached { try operation.join() }
                let library = try await withTaskCancellationHandler {
                    try await detached.value
                } onCancel: {
                    do {
                        try operation.cancel()
                    }
                    catch {
                        logger.error(
                            "Failed to cancel device pairing: \(error.localizedDescription)"
                        )
                    }
                    detached.cancel()
                }
                try Task.checkCancellation()
                isJoining = false
                joiningFingerprint = nil
                onLibraryReady(library)
            }
            catch is CancellationError {
                logger.debug("device pairing join cancelled")
            }
            catch {
                isJoining = false
                joiningFingerprint = nil
                self.error = error.displayLine
            }
        }
    }
}

#if DEBUG
    #Preview {
        WelcomeWindowChrome {
            JoinLibraryView(onLibraryReady: { _ in }, onBack: {})
        }
        .environment(LibrarySetup.stub())
    }
#endif
