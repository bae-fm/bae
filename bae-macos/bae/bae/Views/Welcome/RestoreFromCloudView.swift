import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("RestoreFromCloudView")

/// The restore-from-cloud screen: paste a restore code, or open the disclosure
/// and enter the library's details by hand. Owns the code decode, the manual
/// draft, the OAuth state, and the restore itself.
struct RestoreFromCloudView: View {
    let onLibraryReady: (BridgeLibrary) -> Void
    let onBack: () -> Void

    @Environment(LibrarySetup.self)
    private var setup

    @State
    private var restoreCodeInput = ""
    /// The decode of the current restore-code input: `nil` when the input is
    /// empty (nothing to decode), `.success(info)` for a valid code, or
    /// `.failure(error)` describing why the input couldn't be parsed.
    @State
    private var decodedRestore: Result<BridgeRestoreCodeInfo, Error>?
    @State
    private var isRestoring = false
    /// The in-flight restore (from code or manual), owned so a superseding
    /// restore and the view's disappear can cancel it.
    @State
    private var restoreTask: Task<Void, Never>?
    @State
    private var oauthTokenJson: String?
    @State
    private var isAuthorizing = false
    @State
    private var showManualForm = false
    @State
    private var draft = ManualRestoreDraft()
    @State
    private var error: String?

    var body: some View {
        VStack(spacing: 0) {
            Text("Restore from cloud")
                .font(.title2.bold())
                .padding(.top, 24)
                .padding(.bottom, 4)
            Text("Paste your restore code, or enter details manually.")
                .font(.callout)
                .foregroundStyle(.secondary)
                .padding(.bottom, 16)
            Form {
                Section("Restore code") {
                    TextField("Paste restore code", text: $restoreCodeInput)
                        .font(.system(.body, design: .monospaced))
                        .onChange(of: restoreCodeInput) { _, newInput in
                            oauthTokenJson = nil
                            let trimmed = newInput.trimmingCharacters(
                                in: .whitespaces
                            )
                            decodedRestore =
                                trimmed.isEmpty
                                ? nil
                                : decode(restoreCode: newInput)
                        }
                    if case .success(let info) = decodedRestore {
                        LabeledContent(
                            "Provider",
                            value: info.cloudProvider.displayName
                        )
                        LabeledContent("Library", value: info.libraryName)
                        #if BAE_OAUTH_PROVIDERS
                            if info.needsOauth {
                                OauthConnectRow(
                                    provider: info.cloudProvider,
                                    isConnected: oauthTokenJson != nil,
                                    isAuthorizing: isAuthorizing,
                                    onConnect: {
                                        doOAuthAuthorize(
                                            provider: info.cloudProvider
                                        )
                                    },
                                    onCancelAuth: { isAuthorizing = false },
                                )
                            }
                        #endif
                    }
                    else if case .failure(let decodeError) = decodedRestore {
                        Text(decodeError.displayLine ?? "")
                            .foregroundStyle(.red)
                            .font(.callout)
                    }
                }
                DisclosureGroup(
                    "Enter details manually",
                    isExpanded: $showManualForm
                ) {
                    ManualRestoreForm(
                        draft: $draft,
                        oauthConnected: oauthTokenJson != nil,
                        isAuthorizing: isAuthorizing,
                        onConnect: { doOAuthAuthorize(provider: $0) },
                        onCancelAuth: { isAuthorizing = false },
                    )
                }
            }
            .formStyle(.grouped)
            .scrollDisabled(true)
            if let error {
                Text(error)
                    .foregroundStyle(.red)
                    .font(.callout)
                    .padding(.horizontal)
                    .padding(.bottom, 8)
            }
            if isRestoring {
                HStack(spacing: 8) {
                    ProgressView()
                        .controlSize(.small)
                    Text("Restoring library...")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                }
                .padding(.bottom, 12)
            }
            HStack(spacing: 12) {
                Button("Back") {
                    onBack()
                }
                .buttonStyle(.bordered)
                .disabled(isRestoring)
                Button("Restore") {
                    if case .success = decodedRestore {
                        doRestoreFromCode()
                    }
                    else {
                        doRestoreManual()
                    }
                }
                .buttonStyle(.borderedProminent)
                .disabled(isRestoring || !restoreReady)
                .keyboardShortcut(.defaultAction)
            }
            .padding(.bottom, 24)
        }
        .padding(.horizontal)
        // Switching the manual provider drops any token connected for the old
        // one, so a stale token can't ride into a restore for a different
        // provider.
        .onChange(of: draft.provider) {
            oauthTokenJson = nil
        }
        .onDisappear { restoreTask?.cancel() }
    }

    /// Decode a non-empty restore code into its info, or the error explaining
    /// why it couldn't be parsed. The caller owns the empty-input precondition —
    /// this always attempts a real decode.
    private func decode(
        restoreCode raw: String
    ) -> Result<BridgeRestoreCodeInfo, Error> {
        Result { try setup.decodeRestoreCode(raw) }
    }

    // MARK: - Validation

    /// Whether the restore button should be enabled.
    private var restoreReady: Bool {
        if case .success(let info) = decodedRestore {
            // Restore code flow: valid code + OAuth done if needed
            if info.needsOauth {
                return oauthTokenJson != nil
            }
            return true
        }
        // Manual form flow
        if showManualForm {
            return manualFormValid
        }
        return false
    }

    private var manualFormValid: Bool {
        validateRestoreConfig(
            config: draft.buildRestoreConfig(oauthTokenJson: oauthTokenJson)
        )
    }

    // MARK: - Actions

    /// Restore the library from the current restore-code input. The bridge
    /// re-decodes the code, so callers only need to have confirmed a valid
    /// decode first — there's nothing to pass in.
    private func doRestoreFromCode() {
        let code = restoreCodeInput
        let token = oauthTokenJson
        let restore = setup.restoreFromCode
        runRestore {
            try restore(code, token)
        }
    }

    private func doRestoreManual() {
        let config = draft.buildRestoreConfig(oauthTokenJson: oauthTokenJson)
        let name: String? = draft.libraryName.isEmpty ? nil : draft.libraryName
        let restore = setup.restoreFromCloud
        runRestore {
            try restore(name, config)
        }
    }

    /// Run a restore (from code or manual) off the UI thread, cancelling any
    /// in-flight restore first. The heavy bridge call blocks its worker, so the
    /// owned task is checked for cancellation before it touches `screen`-driving
    /// state: a superseded restore neither opens its (now stale) library nor
    /// clears `isRestoring` out from under the restore that replaced it.
    private func runRestore(
        _ work: @escaping @Sendable () throws -> BridgeLibrary
    ) {
        restoreTask?.cancel()
        isRestoring = true
        error = nil
        restoreTask = Task {
            do {
                let restored = try await DetachedWork.run(work)
                try Task.checkCancellation()
                isRestoring = false
                onLibraryReady(restored)
            }
            catch is CancellationError {
                // Superseded by a newer restore, which set `isRestoring = true`
                // for itself when it cancelled this one — leave the flag alone so
                // its spinner stays up. The superseding restore owns it.
                logger.debug("Restore superseded by a newer restore; skipping")
            }
            catch {
                isRestoring = false
                self.error = error.displayLine
            }
        }
    }

    #if BAE_OAUTH_PROVIDERS
        private func doOAuthAuthorize(provider: BridgeCloudProvider) {
            isAuthorizing = true
            error = nil
            let authorize = setup.oauthAuthorize
            Task.detached {
                do {
                    let tokenJson = try authorize(provider)
                    await MainActor.run {
                        guard isAuthorizing else {
                            return
                        }
                        isAuthorizing = false
                        oauthTokenJson = tokenJson
                    }
                }
                catch {
                    await MainActor.run {
                        isAuthorizing = false
                        self.error = error.displayLine
                    }
                }
            }
        }
    #endif
}

#if DEBUG
    #Preview {
        MainWindowChrome(loadError: nil) {
            RestoreFromCloudView(
                onLibraryReady: { _ in },
                onBack: {},
            )
        }
        .environment(LibrarySetup.stub)
    }
#endif
