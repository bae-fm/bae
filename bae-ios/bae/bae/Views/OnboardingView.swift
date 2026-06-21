import AuthenticationServices
import SwiftUI
import UIKit
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
    /// This device's join-request code and a short fingerprint of its public
    /// key, generated on appear and shown for an existing member to scan or
    /// paste. `nil` while generating; `.failure` if generation fails.
    @State
    private var joinRequest: Result<GeneratedJoinCode, Error>?
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
                linkingView
            }
            else {
                switch mode {
                case .entry:
                    entryView
                case .join:
                    joinView
                }
            }
        }
        .fullScreenCover(isPresented: $showScanner) {
            scannerSheet(onScanned: { link(code: $0) })
        }
        .fullScreenCover(isPresented: $showInviteScanner) {
            scannerSheet(onScanned: { inviteCodeInput = $0 })
        }
        .sheet(isPresented: $showPasteSheet) {
            pasteSheet
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

    private var entryView: some View {
        onboardingScreen {
            Image(systemName: "music.note.house.fill")
                .font(.system(size: 72))
                .foregroundStyle(Theme.accent)
            Text("bae")
                .font(.system(size: 48, weight: .bold))
            secondaryText(
                "Add this device to a library you already have on another device."
            )

            VStack(spacing: 12) {
                Button {
                    error = nil
                    mode = .join
                } label: {
                    Text("Join a library")
                        .frame(maxWidth: 240)
                }
                .buttonStyle(.borderedProminent)

                Button {
                    error = nil
                    CameraPermission.requestThenScan(
                        present: { showScanner = true },
                        onError: { error = $0 }
                    )
                } label: {
                    Text("Scan recovery code")
                        .frame(maxWidth: 240)
                }
                .buttonStyle(.bordered)

                Button {
                    error = nil
                    pasteInput = ""
                    showPasteSheet = true
                } label: {
                    Text("Paste recovery code")
                        .frame(maxWidth: 240)
                }
                .buttonStyle(.bordered)
            }
            .padding(.top, 16)

            if let error {
                Text(error)
                    .font(.callout)
                    .foregroundStyle(.red)
                    .multilineTextAlignment(.center)
                    .frame(maxWidth: 320)
            }
        }
    }

    private var linkingView: some View {
        onboardingScreen {
            ProgressView()
                .controlSize(.large)
            Text("Connecting to your library")
                .font(.headline)
                .multilineTextAlignment(.center)
            secondaryText("bae is setting up the library on this device.")
            Button("Cancel") {
                cancelLink()
            }
            .buttonStyle(.bordered)
            .padding(.top, 8)
        }
    }

    private func onboardingScreen<Content: View>(
        @ViewBuilder content: () -> Content
    ) -> some View {
        VStack(spacing: 16) {
            Spacer()
            content()
            Spacer()
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .padding(32)
    }

    private func secondaryText(_ text: LocalizedStringKey) -> some View {
        Text(text)
            .font(.body)
            .foregroundStyle(.secondary)
            .multilineTextAlignment(.center)
            .frame(maxWidth: 320)
    }

    private func scannerSheet(
        onScanned: @escaping (String) -> Void
    ) -> some View {
        ZStack(alignment: .topTrailing) {
            QRScannerView(
                onScanned: { code in
                    showScanner = false
                    showInviteScanner = false
                    onScanned(code)
                },
                onError: { message in
                    showScanner = false
                    showInviteScanner = false
                    error = message
                }
            )
            .ignoresSafeArea()
            Button {
                showScanner = false
                showInviteScanner = false
            } label: {
                Image(systemName: "xmark.circle.fill")
                    .font(.title)
                    .foregroundStyle(.white)
                    .padding()
            }
        }
    }

    private var pasteSheet: some View {
        NavigationStack {
            VStack(alignment: .leading, spacing: 16) {
                Text(
                    "Paste your recovery code. Use this only when you have no other device available to approve this one."
                )
                .font(.callout)
                .foregroundStyle(.secondary)
                TextField(
                    "Paste your recovery code",
                    text: $pasteInput,
                    axis: .vertical
                )
                .textFieldStyle(.roundedBorder)
                .font(.body.monospaced())
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .lineLimit(3, reservesSpace: true)
                Spacer()
            }
            .padding()
            .navigationTitle("Paste recovery code")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { showPasteSheet = false }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Connect") {
                        let code = pasteInput.trimmingCharacters(
                            in: .whitespacesAndNewlines
                        )
                        showPasteSheet = false
                        link(code: code)
                    }
                    .disabled(
                        pasteInput.trimmingCharacters(
                            in: .whitespacesAndNewlines
                        )
                        .isEmpty
                    )
                }
            }
        }
    }

}

// MARK: - Join flow

extension OnboardingView {
    /// This device's generated join code plus a short fingerprint of its public
    /// key, shown so the approving device can confirm it matches.
    fileprivate struct GeneratedJoinCode {
        let code: String
        let fingerprint: String
    }

    fileprivate var joinView: some View {
        NavigationStack {
            List {
                Section("This device's code") {
                    joinRequestRow
                }
                Section("Invite code") {
                    inviteCodeRow
                }
                if let error {
                    Section {
                        Text(error)
                            .foregroundStyle(.red)
                            .font(.callout)
                    }
                }
            }
            .navigationTitle("Join a library")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Back") {
                        mode = .entry
                        error = nil
                    }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Join") { join() }
                        .disabled(!joinReady)
                }
            }
        }
        // Entering the join flow: clear any prior decode/error, then generate
        // this device's code.
        .task {
            error = nil
            await generateJoinCode()
        }
    }

    @ViewBuilder
    fileprivate var joinRequestRow: some View {
        switch joinRequest {
        case nil:
            ProgressView()
                .frame(maxWidth: .infinity)
        case .success(let generated):
            VStack(spacing: 12) {
                Text(
                    "On a device already in your library, open Settings \u{2192} Members \u{2192} Add a device and scan or paste this."
                )
                .font(.caption)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)

                CodeShareBlock(
                    code: generated.code,
                    contentDescription: "This device's join code",
                    qrSize: 180
                )

                // The approving device shows this same fingerprint; matching
                // them confirms the right device is being added.
                Text("This device: \(generated.fingerprint)")
                    .font(.caption.monospaced())
                    .foregroundStyle(.secondary)
            }
            .frame(maxWidth: .infinity)
            .padding(.vertical, 4)
        case .failure(let genError):
            VStack(spacing: 8) {
                Text(genError.localizedDescription)
                    .foregroundStyle(.red)
                    .font(.callout)
                Button("Try again") {
                    genTask?.cancel()
                    genTask = Task { await generateJoinCode() }
                }
            }
            .frame(maxWidth: .infinity)
        }
    }

    @ViewBuilder
    fileprivate var inviteCodeRow: some View {
        Text(
            "Once that device approves this one, enter the invite code it shows."
        )
        .font(.caption)
        .foregroundStyle(.secondary)

        HStack(spacing: 8) {
            TextField("Paste invite code", text: $inviteCodeInput)
                .font(.body.monospaced())
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
            Button("Scan") {
                error = nil
                CameraPermission.requestThenScan(
                    present: { showInviteScanner = true },
                    onError: { error = $0 }
                )
            }
        }
        .onChange(of: inviteCodeInput) { _, newInput in
            let trimmed = newInput.trimmingCharacters(in: .whitespaces)
            decodedInvite =
                trimmed.isEmpty
                ? nil
                : Result { try decodeInviteCode(code: trimmed) }
        }

        if case .success(let info) = decodedInvite {
            LabeledContent("Library", value: info.libraryName)
            LabeledContent(
                "Provider",
                value: info.cloudProvider.displayName
            )
            LabeledContent(
                "Owner",
                value: MemberFormat.fingerprint(info.ownerPubkey)
            )
            #if !BAE_OAUTH_PROVIDERS
            if info.needsOauth {
                Text(
                    "This library uses a provider this build can't connect to."
                )
                .foregroundStyle(.red)
                .font(.callout)
            }
            #endif
        }
        else if case .failure(let decodeError) = decodedInvite {
            Text(decodeError.localizedDescription)
                .foregroundStyle(.red)
                .font(.callout)
        }
    }

    /// Whether the Join button should be enabled: a valid invite code that this
    /// build can connect to. OAuth, when needed, runs as part of the join.
    fileprivate var joinReady: Bool {
        guard case .success(let info) = decodedInvite else {
            return false
        }
        #if !BAE_OAUTH_PROVIDERS
        if info.needsOauth {
            return false
        }
        #endif
        return true
    }

    private func generateJoinCode() async {
        joinRequest = nil
        do {
            let generated = try await withTaskCancellationHandler {
                let detached = Task.detached { () throws -> GeneratedJoinCode in
                    let code = try generateJoinRequest()
                    let pubkey = try decodeJoinRequest(code: code).pubkey
                    return GeneratedJoinCode(
                        code: code,
                        fingerprint: MemberFormat.fingerprint(pubkey)
                    )
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
        guard case .success(let info) = decodedInvite else {
            logger.warning("join tapped without a decoded invite code")
            return
        }
        let code = inviteCodeInput.trimmingCharacters(
            in: .whitespacesAndNewlines
        )
        error = nil
        cancelLink()
        let flow = LinkFlow { flow in
            Task {
                defer {
                    if linkFlow === flow {
                        linkFlow = nil
                    }
                }
                await performJoin(code: code, info: info, flow: flow)
            }
        }
        linkFlow = flow
        flow.start()
    }

    private func performJoin(
        code: String,
        info: BridgeInviteCodeInfo,
        flow: LinkFlow
    ) async {
        do {
            let tokenJson: String?
            if info.needsOauth {
                guard
                    let token = try await oauthToken(
                        provider: info.cloudProvider
                    )
                else {
                    logger.debug("join cancelled at cloud sign-in")
                    return
                }
                tokenJson = token
            }
            else {
                tokenJson = nil
            }
            try await runJoin(code: code, tokenJson: tokenJson, flow: flow)
        }
        catch {
            if isLinkCancellation(error) {
                logger.debug("join flow cancelled")
            }
            else {
                self.error = error.localizedDescription
            }
        }
    }

    private func runJoin(
        code: String,
        tokenJson: String?,
        flow: LinkFlow
    ) async throws {
        let operation = try joinFromCodeOperation(
            code: code,
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
                self.error = error.localizedDescription
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

#if BAE_OAUTH_PROVIDERS
private struct PresentationAnchorReader: UIViewRepresentable {
    @Binding
    var presentationAnchor: ASPresentationAnchor?

    func makeUIView(context: Context) -> UIView {
        UIView(frame: .zero)
    }

    func updateUIView(_ uiView: UIView, context: Context) {
        let window = uiView.window
        DispatchQueue.main.async {
            presentationAnchor = window
        }
    }
}
#endif
