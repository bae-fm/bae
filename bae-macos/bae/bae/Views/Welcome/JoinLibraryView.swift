import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("JoinLibraryView")

/// The join-a-library screen: add this device to a library that already exists
/// on another of the owner's devices. Two steps — pick the cloud provider, then
/// exchange this device's join code for the invite code the approving device
/// hands back. Owns the provider selection, the generated code, and the join.
struct JoinLibraryView: View {
    let onLibraryReady: (BridgeLibrary) -> Void
    let onBack: () -> Void

    @Environment(LibrarySetup.self)
    private var setup

    /// The cloud provider the joiner picked for the library they're adding this
    /// device to. `nil` until picked — while `nil` the join flow shows the
    /// provider picker and no code is generated yet. Picking an OAuth provider
    /// authenticates up front so the account email lands in the join-request;
    /// picking S3/iCloud generates the code with no email.
    @State
    private var joinProvider: BridgeCloudProvider?
    /// This device's join-request code and a short fingerprint of its public
    /// key, generated once a provider is picked (and, for OAuth, authenticated)
    /// and shown for an existing member to scan or paste. `nil` while generating;
    /// `.failure` if generation fails.
    @State
    private var joinRequest: Result<BridgeJoinRequest, Error>?
    /// The account email fetched from the picked OAuth provider, baked into the
    /// join-request so the approver shares the OAuth folder to it. `nil` for
    /// S3/iCloud, which share no folder. Held so a code retry reuses it without
    /// re-authenticating.
    @State
    private var joinEmail: String?
    /// The in-flight provider-prepare / (re)generation of this device's join
    /// code, owned so a retry supersedes the previous attempt and the view's
    /// disappear cancels it.
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
    private var isJoining = false
    /// The in-flight join, owned so a superseding join and the view's disappear
    /// can cancel it.
    @State
    private var joinTask: Task<Void, Never>?
    @State
    private var showInviteScanner = false
    @State
    private var isAuthorizing = false
    @State
    private var oauthTokenJson: String?
    @State
    private var error: String?

    var body: some View {
        VStack(spacing: 0) {
            Text("Join a library")
                .font(.title2.bold())
                .padding(.top, 24)
                .padding(.bottom, 4)
            Text(
                "Add this device to a library you already have on another device."
            )
            .font(.callout)
            .foregroundStyle(.secondary)
            .padding(.bottom, 16)

            if joinProvider == nil {
                JoinProviderPicker(
                    providers: availableCloudProviders(),
                    isAuthorizing: isAuthorizing,
                    error: error,
                    onSelect: { selectJoinProvider($0) },
                    onCancel: {
                        #if BAE_OAUTH_PROVIDERS
                            setup.oauthCancel()
                        #endif
                        genTask?.cancel()
                        isAuthorizing = false
                    },
                    onBack: onBack,
                )
            }
            else {
                JoinCodeExchange(
                    joinRequest: joinRequest,
                    inviteCodeInput: $inviteCodeInput,
                    decodedInvite: decodedInvite,
                    oauthConnected: oauthTokenJson != nil,
                    isJoining: isJoining,
                    error: error,
                    joinReady: joinReady,
                    onRetryGenerate: {
                        genTask?.cancel()
                        genTask = Task { await generateJoinCode() }
                    },
                    onScan: { showInviteScanner = true },
                    onJoin: { doJoin() },
                    onBack: {
                        // Return to the provider picker; drop the generated code
                        // and any OAuth token so re-picking starts clean.
                        joinProvider = nil
                        joinRequest = nil
                        oauthTokenJson = nil
                        inviteCodeInput = ""
                        decodedInvite = nil
                        error = nil
                    },
                )
            }
        }
        .padding(.horizontal)
        .onChange(of: inviteCodeInput) { _, newInput in
            let trimmed = newInput.trimmingCharacters(in: .whitespaces)
            decodedInvite =
                trimmed.isEmpty
                ? nil
                : Result { try setup.decodeInviteCode(newInput) }
        }
        .onDisappear {
            genTask?.cancel()
            joinTask?.cancel()
        }
        .sheet(isPresented: $showInviteScanner) {
            InviteScannerSheet(
                onScan: { code in
                    inviteCodeInput = code
                    showInviteScanner = false
                },
                onDismiss: { showInviteScanner = false },
            )
        }
    }

    /// Whether the Join button should be enabled: a valid invite code, plus the
    /// up-front OAuth token held when the picked provider needs it.
    private var joinReady: Bool {
        guard case .success(let info) = decodedInvite else {
            return false
        }
        if info.needsOauth {
            return oauthTokenJson != nil
        }
        return true
    }

    /// Pick the provider the target library uses. For an OAuth provider this
    /// authenticates and fetches the account email up front (baked into the
    /// generated code and reused for the eventual join); for S3/iCloud it goes
    /// straight to generating a code with no email.
    private func selectJoinProvider(_ provider: BridgeCloudProvider) {
        genTask?.cancel()
        error = nil
        oauthTokenJson = nil
        joinEmail = nil
        genTask = Task { await prepareJoin(provider: provider) }
    }

    private func prepareJoin(provider: BridgeCloudProvider) async {
        #if BAE_OAUTH_PROVIDERS
            switch provider {
            case .googleDrive, .dropbox, .oneDrive:
                isAuthorizing = true
                do {
                    let authorize = setup.oauthAuthorize
                    let fetchEmail = setup.fetchAccountEmail
                    let tokenJson = try await DetachedWork.run {
                        try authorize(provider)
                    }
                    let email = try await DetachedWork.run {
                        try fetchEmail(provider, tokenJson)
                    }
                    isAuthorizing = false
                    oauthTokenJson = tokenJson
                    joinEmail = email
                }
                catch is CancellationError {
                    isAuthorizing = false
                    return
                }
                catch {
                    isAuthorizing = false
                    self.error = error.displayLine
                    return
                }
            default:
                break
            }
        #endif
        joinProvider = provider
        await generateJoinCode()
    }

    private func generateJoinCode() async {
        joinRequest = nil
        let email = joinEmail
        let generate = setup.generateJoinRequest
        do {
            let generated = try await DetachedWork.run {
                try generate(email)
            }
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

    /// Join the library from the current invite code. The join runs through a
    /// cancellable operation: a superseding join (or the view disappearing)
    /// cancels the in-flight one through the operation's own token, so the bridge
    /// stops its blocking work rather than running to completion on a stale
    /// library. The post-await cancellation check guards the success path so a
    /// superseded join neither opens its stale library nor clears `isJoining`
    /// out from under the join that replaced it.
    private func doJoin() {
        // The code minted by this device's own generateJoinRequest — coven
        // needs it back to promote that pending identity into this store's
        // custody.
        guard case .success(let request) = joinRequest else { return }
        let code = inviteCodeInput
        let joinRequestCode = request.code
        let token = oauthTokenJson
        joinTask?.cancel()
        isJoining = true
        error = nil
        joinTask = Task {
            let operation: JoinOperation
            do {
                operation = try setup.joinFromCode(
                    code,
                    joinRequestCode,
                    token
                )
            }
            catch {
                isJoining = false
                self.error = error.displayLine
                return
            }
            do {
                let detached = Task.detached { try operation.join() }
                let joined = try await withTaskCancellationHandler {
                    try await detached.value
                } onCancel: {
                    operation.cancel()
                    detached.cancel()
                }
                try Task.checkCancellation()
                isJoining = false
                onLibraryReady(joined)
            }
            catch is CancellationError {
                logger.debug("Join superseded by a newer join; skipping")
            }
            catch {
                isJoining = false
                self.error = error.displayLine
            }
        }
    }
}

#if DEBUG
    #Preview {
        JoinLibraryView(
            onLibraryReady: { _ in },
            onBack: {},
        )
        .environment(LibrarySetup.stub)
    }
#endif
