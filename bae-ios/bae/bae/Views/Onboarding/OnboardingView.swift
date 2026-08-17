import AuthenticationServices
import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("OnboardingView")

/// The in-flight bridge linking work (restore-from-code or join-from-code),
/// owned so a superseding attempt and the view's disappear can cancel it through
/// the operation's own token — the blocking bridge call observes the token and
/// stops, rather than running to completion on a now-stale attempt.
@MainActor
private final class LinkFlow {
    /// Set once the bridge operation exists; cancellation goes through it so the
    /// blocking restore/join call is interrupted, not just the Swift wrapper.
    var bridgeCancel: (@Sendable () -> Void)?
    private let makeTask: (LinkFlow) -> Task<Void, Never>
    private lazy var task: Task<Void, Never> = makeTask(self)

    init(makeTask: @escaping (LinkFlow) -> Task<Void, Never>) {
        self.makeTask = makeTask
    }

    func start() {
        _ = task
    }

    func cancel() {
        bridgeCancel?()
        task.cancel()
    }

    func cancelAndWait() async {
        cancel()
        await task.value
    }
}

/// First-run onboarding. Two ways to put a library on this device: join a
/// library you already have on another device (the other device approves this
/// one), or restore from a recovery code when no other device is available to
/// approve. On submit each decodes its code, runs cloud sign-in when the
/// provider needs it, then runs the bridge restore/join.
struct OnboardingView: View {
    // The host's OAuth client config. Present only in a full build; baeium
    // (S3-only) compiles out the OAuth branch of the link flow.
    #if BAE_OAUTH_PROVIDERS
    let oauthLinking: OAuthLinking?
    let oauthLinkingError: String?
    #endif
    let onLinked: (BridgeLibrary) -> Void

    /// Which screen onboarding is showing. `entry` is the chooser; `join` shows
    /// the pairing-code entry; while a `LinkFlow` is in
    /// flight the linking screen replaces both.
    private enum Mode {
        case entry
        case join
    }

    @State
    private var mode: Mode = .entry

    // Recovery-code restore
    @State
    private var showScanner = false
    @State
    private var showPasteSheet = false
    @State
    private var pasteInput = ""

    // Join-a-library flow
    @State
    private var pairingCodeInput = ""
    @State
    private var joinTokenJson: String?
    @State
    private var isAuthorizing = false
    @State
    private var decodedPairingOffer: Result<BridgeDevicePairingOffer, Error>?
    @State
    private var authorizationTask: Task<Void, Never>?
    @State
    private var showPairingScanner = false

    @State
    private var error: String?
    @State
    private var linkFlow: LinkFlow?
    @State
    private var linkingContext = OnboardingLinkingScreen.Context.librarySetup
    @State
    private var joinProgress: BridgeJoiningDeviceJoinProgress?
    @State
    private var pendingPairing: BridgePendingDevicePairingJoin?
    #if BAE_OAUTH_PROVIDERS
    @State
    private var presentationAnchor: ASPresentationAnchor?
    #endif

    var body: some View {
        Group {
            if linkFlow != nil {
                OnboardingLinkingScreen(
                    context: linkingContext,
                    joinProgress: joinProgress,
                    onCancel: {
                        if case .devicePairing = linkingContext {
                            abandonPairingAndReturnToEntry()
                        }
                        else {
                            cancelLink()
                        }
                    }
                )
            }
            else {
                switch mode {
                case .entry:
                    OnboardingEntryScreen(
                        error: error,
                        onJoin: {
                            error = nil
                            mode = .join
                        },
                        onScanRecovery: {
                            error = nil
                            CameraPermission.requestThenScan(
                                present: { showScanner = true },
                                onError: { error = $0 }
                            )
                        },
                        onPasteRecovery: {
                            error = nil
                            pasteInput = ""
                            showPasteSheet = true
                        }
                    )
                case .join:
                    joinView
                }
            }
        }
        .fullScreenCover(isPresented: $showScanner) {
            ScannerSheet(
                onScanned: { code in
                    showScanner = false
                    showPairingScanner = false
                    link(code: code)
                },
                onError: { message in
                    showScanner = false
                    error = message
                },
                onClose: {
                    showScanner = false
                }
            )
        }
        .fullScreenCover(isPresented: $showPairingScanner) {
            ScannerSheet(
                onScanned: { code in
                    showScanner = false
                    showPairingScanner = false
                    pairingCodeInput = code
                },
                onError: { message in
                    showScanner = false
                    showPairingScanner = false
                    error = message
                },
                onClose: {
                    showScanner = false
                    showPairingScanner = false
                }
            )
        }
        .sheet(isPresented: $showPasteSheet) {
            PasteRecoveryCodeSheet(
                input: $pasteInput,
                onCancel: { showPasteSheet = false },
                onConnect: { code in
                    showPasteSheet = false
                    link(code: code)
                }
            )
        }
        .onDisappear {
            linkFlow?.cancel()
            authorizationTask?.cancel()
        }
        .task {
            await discoverPendingPairing()
        }
        #if BAE_OAUTH_PROVIDERS
        // Captures the host window so the OAuth web-auth session has a
        // presentation anchor. Only the OAuth link branch needs it.
        .background(
            PresentationAnchorReader(presentationAnchor: $presentationAnchor)
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
        )
        #endif
    }
}

// MARK: - Join flow

extension OnboardingView {
    fileprivate var joinView: some View {
        NavigationStack {
            JoinPairingOffer(
                pairingCode: $pairingCodeInput,
                decodedOffer: decodedPairingOffer,
                isAuthorizing: isAuthorizing,
                error: error,
                onScan: {
                    error = nil
                    CameraPermission.requestThenScan(
                        present: { showPairingScanner = true },
                        onError: { error = $0 }
                    )
                },
                onCodeChanged: { decodePairingOffer($0) }
            )
            .navigationTitle("Join a library")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button(isAuthorizing || linkFlow != nil ? "Cancel" : "Back") {
                        abandonPairingAndReturnToEntry()
                    }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Join") { join() }
                        .disabled(!joinReady)
                }
            }
        }
        // A durable attempt resumes from its retained code and phase. A newly
        // opened join screen clears every value from the preceding attempt.
        .task(id: pendingPairing?.fingerprint) {
            if let pendingPairing {
                resume(pendingPairing)
            }
            else {
                pairingCodeInput = ""
                joinTokenJson = nil
                decodedPairingOffer = nil
                error = nil
            }
        }
    }

    /// Whether the Join button should be enabled: a valid pairing code, plus the
    /// up-front OAuth token held when the picked provider needs it.
    fileprivate var joinReady: Bool {
        guard case .success(let info) = decodedPairingOffer else {
            return false
        }
        #if BAE_OAUTH_PROVIDERS
        if info.needsOauth {
            return joinTokenJson != nil
        }
        #else
        if info.needsOauth {
            return false
        }
        #endif
        return true
    }

    private func decodePairingOffer(_ input: String) {
        authorizationTask?.cancel()
        error = nil
        joinTokenJson = nil
        isAuthorizing = false
        let code = input.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !code.isEmpty else {
            decodedPairingOffer = nil
            return
        }
        let decoded = Result { try decodeDevicePairingOffer(code: code) }
        decodedPairingOffer = decoded
        guard case .success(let offer) = decoded, offer.needsOauth else { return }
        authorizeJoiningProvider(offer, joinAfterAuthorization: false)
    }

    private func authorizeJoiningProvider(
        _ offer: BridgeDevicePairingOffer,
        joinAfterAuthorization: Bool
    ) {
        isAuthorizing = true
        authorizationTask = Task {
            do {
                guard let token = try await oauthToken(provider: offer.cloudProvider) else {
                    isAuthorizing = false
                    return
                }
                try Task.checkCancellation()
                joinTokenJson = token
                isAuthorizing = false
                if joinAfterAuthorization {
                    join()
                }
            }
            catch {
                isAuthorizing = false
                if !isLinkCancellation(error) {
                    self.error = error.displayLine
                }
            }
        }
    }

    private func discoverPendingPairing() async {
        do {
            guard
                let pending = try await DetachedWork.run({
                    try pendingDevicePairingJoin()
                })
            else { return }
            pendingPairing = pending
            mode = .join
        }
        catch {
            self.error = error.displayLine
        }
    }

    private func resume(_ pending: BridgePendingDevicePairingJoin) {
        pairingCodeInput = pending.pairingCode
        decodedPairingOffer = .success(pending.offer)
        error = nil
        if pending.offer.needsOauth,
            pending.phase != .libraryInstallationPending
        {
            authorizeJoiningProvider(
                pending.offer,
                joinAfterAuthorization: true
            )
        }
        else {
            join()
        }
    }

    private func abandonPairingAndReturnToEntry() {
        let activeFlow = linkFlow
        let activeAuthorization = authorizationTask
        activeAuthorization?.cancel()
        Task {
            await activeAuthorization?.value
            await activeFlow?.cancelAndWait()
            do {
                try await DetachedWork.run {
                    try abandonPendingDevicePairingJoin()
                }
                linkFlow = nil
                joinProgress = nil
                pendingPairing = nil
                pairingCodeInput = ""
                decodedPairingOffer = nil
                mode = .entry
                error = nil
            }
            catch {
                self.error = error.displayLine
            }
        }
    }

    /// Join the library from the current pairing code: decode it, run cloud
    /// sign-in when the provider needs it, then run the cancellable join
    /// operation. A superseding join (or the view disappearing) cancels the
    /// in-flight one through the operation's own token, so the bridge stops its
    /// blocking work rather than running to completion on a stale library.
    func join() {
        guard case .success = decodedPairingOffer else {
            logger.warning("join tapped without a decoded pairing code")
            return
        }
        let code = pairingCodeInput.trimmingCharacters(
            in: .whitespacesAndNewlines
        )
        let tokenJson = joinTokenJson
        error = nil
        cancelLink()
        linkingContext = .devicePairing(fingerprint: nil)
        joinProgress = nil
        let flow = LinkFlow { flow in
            Task {
                defer {
                    if linkFlow === flow {
                        linkFlow = nil
                    }
                }
                await performJoin(
                    code: code,
                    tokenJson: tokenJson,
                    flow: flow
                )
            }
        }
        linkFlow = flow
        flow.start()
    }

    private func performJoin(
        code: String,
        tokenJson: String?,
        flow: LinkFlow
    ) async {
        do {
            try await runJoin(
                code: code,
                tokenJson: tokenJson,
                flow: flow
            )
        }
        catch {
            if isLinkCancellation(error) {
                logger.debug("join flow cancelled")
            }
            else {
                self.error = error.displayLine
            }
        }
    }

    private func runJoin(
        code: String,
        tokenJson: String?,
        flow: LinkFlow
    ) async throws {
        let operation = try await joinDevicePairingOperation(
            pairingCode: code,
            oauthTokenJson: tokenJson
        )
        linkingContext = .devicePairing(fingerprint: operation.fingerprint())
        joinProgress = .waitingForApproval
        flow.bridgeCancel = {
            do {
                try operation.cancel()
            }
            catch {
                logger.error(
                    "Failed to cancel device pairing: \(error.localizedDescription)"
                )
            }
        }
        let libraryInfo = try await withTaskCancellationHandler {
            try Task.checkCancellation()
            let progress = JoiningDeviceJoinProgressSink {
                joinProgress = $0
            }
            return
                try await Task.detached {
                    try operation.join(progress: progress)
                }
                .value
        } onCancel: {
            do {
                try operation.cancel()
            }
            catch {
                logger.error(
                    "Failed to cancel device pairing: \(error.localizedDescription)"
                )
            }
        }
        try Task.checkCancellation()
        onLinked(libraryInfo)
    }
}

// MARK: - Recovery-code restore

extension OnboardingView {
    func cancelLink() {
        linkFlow?.cancel()
        linkFlow = nil
        joinProgress = nil
    }

    /// Decode the recovery code, run cloud sign-in when required, inject the
    /// CloudKit driver before restore when the library syncs through CloudKit,
    /// and restore. A recovery code points at the owner's own private CloudKit
    /// zone — every device is the one owner.
    func link(code: String) {
        error = nil
        cancelLink()
        linkingContext = .librarySetup
        joinProgress = nil
        let flow = LinkFlow { flow in
            Task {
                defer {
                    if linkFlow === flow {
                        linkFlow = nil
                    }
                }
                await performLink(code: code, flow: flow)
            }
        }
        linkFlow = flow
        flow.start()
    }

    /// The link work, off the `LinkFlow`'s task: decode, obtain a cloud token
    /// when the provider needs one, then restore. Sets `error` and returns on a
    /// handled failure; logs and ignores cancellation.
    private func performLink(code: String, flow: LinkFlow) async {
        do {
            let info = try decodeRestoreCode(code: code)
            let tokenJson: String?
            if info.needsOauth {
                guard
                    let token = try await oauthToken(
                        provider: info.cloudProvider
                    )
                else {
                    logger.debug("restore cancelled at cloud sign-in")
                    return
                }
                tokenJson = token
            }
            else {
                tokenJson = nil
            }
            try await restore(code: code, tokenJson: tokenJson, flow: flow)
        }
        catch {
            if isLinkCancellation(error) {
                logger.debug("link flow cancelled")
            }
            else {
                self.error = error.displayLine
            }
        }
    }

    /// The cloud token for a library that needs OAuth, or `nil` after setting
    /// `error` when this build / config can't satisfy it. Only called when the
    /// decoded code reports `needsOauth`: a provider like Google Drive runs the
    /// system auth session; CloudKit and S3 need none.
    private func oauthToken(
        provider: BridgeCloudProvider
    ) async throws -> String? {
        #if BAE_OAUTH_PROVIDERS
        if let oauthLinkingError {
            error = oauthLinkingError
            return nil
        }
        guard let linking = oauthLinking else {
            error = String(
                localized:
                    "This library needs cloud sign-in, which isn't configured on this build."
            )
            return nil
        }
        guard let presentationAnchor else {
            throw OAuthLinkingError.noPresentationAnchor
        }
        return try await linking.authorize(
            provider: provider,
            presentationAnchor: presentationAnchor
        )
        #else
        // A baeium (S3-only) build can't sign in to OAuth providers at all, so a
        // library that needs one can't be linked here.
        error = String(
            localized:
                "This library syncs through a cloud provider this build doesn't support."
        )
        return nil
        #endif
    }

    private func restore(
        code: String,
        tokenJson: String?,
        flow: LinkFlow
    ) async throws {
        let bridgeOperation = try restoreFromCodeOperation(
            code: code,
            oauthTokenJson: tokenJson
        )
        flow.bridgeCancel = { bridgeOperation.cancel() }
        let libraryInfo = try await withTaskCancellationHandler {
            try Task.checkCancellation()
            return
                try await Task.detached {
                    try bridgeOperation.restore()
                }
                .value
        } onCancel: {
            bridgeOperation.cancel()
        }
        try Task.checkCancellation()
        onLinked(libraryInfo)
    }

    private func isLinkCancellation(_ error: Error) -> Bool {
        if error is CancellationError {
            return true
        }
        if case BridgeError.Cancelled = error {
            return true
        }
        return false
    }
}

#if DEBUG
#Preview {
    #if BAE_OAUTH_PROVIDERS
    OnboardingView(
        oauthLinking: nil,
        oauthLinkingError: nil,
        onLinked: { _ in }
    )
    #else
    OnboardingView(onLinked: { _ in })
    #endif
}
#endif
