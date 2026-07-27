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
    /// this device's code and accepts an invite code; while a `LinkFlow` is in
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
    /// The cloud provider the joiner picked for the library it's adding this
    /// device to. `nil` until picked — while `nil` the join flow shows the
    /// provider picker and no code is generated yet. Picking an OAuth provider
    /// authenticates up front so the account email lands in the join-request;
    /// picking S3/iCloud generates the code with no email.
    @State
    private var joinProvider: BridgeCloudProvider?
    /// The OAuth token captured up front for the picked provider, held so the
    /// eventual join reuses it instead of authenticating a second time. `nil`
    /// for S3/iCloud.
    @State
    private var joinTokenJson: String?
    /// The account email fetched from the picked OAuth provider, baked into the
    /// join-request so the approver shares the OAuth folder to it. `nil` for
    /// S3/iCloud. Held so a code retry reuses it without re-authenticating.
    @State
    private var joinEmail: String?
    /// True while authenticating with the picked OAuth provider before the code
    /// is generated.
    @State
    private var isAuthorizing = false
    /// This device's join-request code and a short fingerprint of its public
    /// key, generated once a provider is picked (and, for OAuth, authenticated)
    /// and shown for an existing member to scan or paste. `nil` while generating;
    /// `.failure` if generation fails.
    @State
    private var joinRequest: Result<BridgeJoinRequest, Error>?
    @State
    private var genTask: Task<Void, Never>?
    @State
    private var inviteCodeInput = ""
    /// The decode of the current invite-code input: `nil` when empty,
    /// `.success(info)` for a valid code, `.failure(error)` for an unparseable
    /// one.
    @State
    private var decodedInvite: Result<BridgeInviteCodeInfo, Error>?
    @State
    private var showInviteScanner = false

    @State
    private var error: String?
    @State
    private var linkFlow: LinkFlow?
    #if BAE_OAUTH_PROVIDERS
    @State
    private var presentationAnchor: ASPresentationAnchor?
    #endif

    var body: some View {
        Group {
            if linkFlow != nil {
                OnboardingLinkingScreen(onCancel: { cancelLink() })
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
                    showInviteScanner = false
                    link(code: code)
                },
                onError: { message in
                    showScanner = false
                    showInviteScanner = false
                    error = message
                },
                onClose: {
                    showScanner = false
                    showInviteScanner = false
                }
            )
        }
        .fullScreenCover(isPresented: $showInviteScanner) {
            ScannerSheet(
                onScanned: { code in
                    showScanner = false
                    showInviteScanner = false
                    inviteCodeInput = code
                },
                onError: { message in
                    showScanner = false
                    showInviteScanner = false
                    error = message
                },
                onClose: {
                    showScanner = false
                    showInviteScanner = false
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
            genTask?.cancel()
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
            Group {
                if joinProvider == nil {
                    JoinProviderPicker(
                        providers: availableCloudProviders(),
                        isAuthorizing: isAuthorizing,
                        error: error,
                        onSelect: { selectJoinProvider($0) }
                    )
                }
                else {
                    JoinCodeExchange(
                        joinRequest: joinRequest,
                        inviteCode: $inviteCodeInput,
                        decodedInvite: decodedInvite,
                        joinTokenJson: joinTokenJson,
                        error: error,
                        onRetryGenerate: {
                            genTask?.cancel()
                            genTask = Task { await generateJoinCode() }
                        },
                        onScanInvite: {
                            error = nil
                            CameraPermission.requestThenScan(
                                present: { showInviteScanner = true },
                                onError: { error = $0 }
                            )
                        },
                        onInviteChanged: { newInput in
                            let trimmed = newInput.trimmingCharacters(
                                in: .whitespaces
                            )
                            decodedInvite =
                                trimmed.isEmpty
                                ? nil : Result { try decodeInvite(pasted: trimmed) }
                        }
                    )
                }
            }
            .navigationTitle("Join a library")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Back") {
                        if joinProvider == nil {
                            mode = .entry
                        }
                        else {
                            // Return to the provider picker; drop the generated
                            // code and any OAuth token so re-picking starts clean.
                            joinProvider = nil
                            joinRequest = nil
                            joinTokenJson = nil
                            joinEmail = nil
                            inviteCodeInput = ""
                            decodedInvite = nil
                        }
                        error = nil
                    }
                    .disabled(isAuthorizing)
                }
                if joinProvider != nil {
                    ToolbarItem(placement: .confirmationAction) {
                        Button("Join") { join() }
                            .disabled(!joinReady)
                    }
                }
            }
        }
        // Entering the join flow fresh: clear any prior provider/token/decode so
        // nothing leaks across attempts. The code is generated only once the
        // joiner picks a provider.
        .task {
            joinProvider = nil
            joinTokenJson = nil
            joinEmail = nil
            joinRequest = nil
            error = nil
        }
    }

    /// Whether the Join button should be enabled: a valid invite code, plus the
    /// up-front OAuth token held when the picked provider needs it.
    fileprivate var joinReady: Bool {
        guard case .success(let info) = decodedInvite else {
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

    /// Pick the provider the target library uses. For an OAuth provider this
    /// authenticates and fetches the account email up front (baked into the
    /// generated code and reused for the eventual join); for S3/iCloud it goes
    /// straight to generating a code with no email.
    private func selectJoinProvider(_ provider: BridgeCloudProvider) {
        genTask?.cancel()
        error = nil
        joinTokenJson = nil
        joinEmail = nil
        genTask = Task { await prepareJoin(provider: provider) }
    }

    private func prepareJoin(provider: BridgeCloudProvider) async {
        #if BAE_OAUTH_PROVIDERS
        switch provider {
        case .googleDrive, .dropbox, .oneDrive:
            isAuthorizing = true
            do {
                guard let token = try await oauthToken(provider: provider)
                else {
                    // Cancelled, or unavailable with `error` already set.
                    isAuthorizing = false
                    return
                }
                let email = try await fetchAccountEmailDetached(
                    provider: provider,
                    tokenJson: token
                )
                joinTokenJson = token
                joinEmail = email
            }
            catch {
                isAuthorizing = false
                if !isLinkCancellation(error) {
                    self.error = error.displayLine
                }
                return
            }
            isAuthorizing = false
        default:
            break
        }
        #endif
        joinProvider = provider
        await generateJoinCode()
    }

    #if BAE_OAUTH_PROVIDERS
    /// Turn the OAuth token into the account email off the main thread — the
    /// bridge call is synchronous and hits the network.
    private func fetchAccountEmailDetached(
        provider: BridgeCloudProvider,
        tokenJson: String
    ) async throws -> String {
        try await Task.detached {
            try fetchAccountEmail(
                provider: provider,
                oauthTokenJson: tokenJson
            )
        }
        .value
    }
    #endif

    private func generateJoinCode() async {
        joinRequest = nil
        let email = joinEmail
        do {
            let generated = try await withTaskCancellationHandler {
                let detached = Task.detached { () throws -> BridgeJoinRequest in
                    try generateJoinRequest(email: email)
                }
                return try await detached.value
            } onCancel: {
                // generateJoinRequest is a synchronous bridge call with no
                // cancellation token; the detached worker runs to completion and
                // its result is dropped.
            }
            try Task.checkCancellation()
            joinRequest = .success(generated)
        }
        catch is CancellationError {
            logger.debug("join request generation cancelled")
        }
        catch {
            logger.error(
                "Failed to generate join request: \(error.localizedDescription)"
            )
            joinRequest = .failure(error)
        }
    }

    /// Join the library from the current invite code: decode it, run cloud
    /// sign-in when the provider needs it, then run the cancellable join
    /// operation. A superseding join (or the view disappearing) cancels the
    /// in-flight one through the operation's own token, so the bridge stops its
    /// blocking work rather than running to completion on a stale library.
    func join() {
        guard case .success = decodedInvite else {
            logger.warning("join tapped without a decoded invite code")
            return
        }
        // The code minted by this device's own generateJoinRequest — coven
        // needs it back to promote that pending identity into this store's
        // custody.
        guard case .success(let request) = joinRequest else {
            logger.warning("join tapped before the join-request code was ready")
            return
        }
        let code = inviteCodeInput.trimmingCharacters(
            in: .whitespacesAndNewlines
        )
        let joinRequestCode = request.code
        // Reuse the token captured when the provider was picked — no second
        // OAuth at join time.
        let tokenJson = joinTokenJson
        error = nil
        cancelLink()
        let flow = LinkFlow { flow in
            Task {
                defer {
                    if linkFlow === flow {
                        linkFlow = nil
                    }
                }
                await performJoin(
                    code: code,
                    joinRequestCode: joinRequestCode,
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
        joinRequestCode: String,
        tokenJson: String?,
        flow: LinkFlow
    ) async {
        do {
            try await runJoin(
                code: code,
                joinRequestCode: joinRequestCode,
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

    /// The invite bundle behind what the joiner pasted or scanned.
    ///
    /// The invite is a byte payload — the owner's signed join offer, not a
    /// human-typed code — so it travels as base64 wherever it has to be text.
    /// A QR scan hands back that same text, so both entry paths decode here.
    private func inviteBytes(pasted: String) throws -> Data {
        guard
            let bytes = Data(
                base64Encoded: pasted.trimmingCharacters(in: .whitespacesAndNewlines)
            )
        else {
            throw BridgeError.Diagnostic(
                category: .config,
                detail: "the pasted invite is not base64 an invite bundle could be read from"
            )
        }
        return bytes
    }

    private func decodeInvite(pasted: String) throws -> BridgeInviteCodeInfo {
        try decodeScannedInvite(scanned: try inviteBytes(pasted: pasted))
    }

    private func runJoin(
        code: String,
        joinRequestCode: String,
        tokenJson: String?,
        flow: LinkFlow
    ) async throws {
        let operation = try joinFromScannedInviteOperation(
            scanned: try inviteBytes(pasted: code),
            joinRequestCode: joinRequestCode,
            oauthTokenJson: tokenJson
        )
        flow.bridgeCancel = { operation.cancel() }
        let libraryInfo = try await withTaskCancellationHandler {
            try Task.checkCancellation()
            return
                try await Task.detached {
                    try operation.join()
                }
                .value
        } onCancel: {
            operation.cancel()
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
    }

    /// Decode the recovery code, run cloud sign-in when required, inject the
    /// CloudKit driver before restore when the library syncs through CloudKit,
    /// and restore. A recovery code points at the owner's own private CloudKit
    /// zone — every device is the one owner.
    func link(code: String) {
        error = nil
        cancelLink()
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
