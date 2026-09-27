import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("RestoreFromCloudView")

/// The restore-from-cloud screen, where a pasted restore code carries
/// everything the restore needs.
struct RestoreFromCloudView: View {
    let onLibraryReady: (BridgeLibrary) -> Void
    let onBack: () -> Void

    @Environment(LibrarySetup.self)
    private var setup

    @State
    private var restoreCodeInput = ""
    /// The decoded restore code, or `nil` when the input is empty.
    @State
    private var decodedRestore: Result<BridgeRestoreCodeInfo, Error>?
    @State
    private var isRestoring = false
    /// The in-flight restore, cancelled by a newer restore or on disappear.
    @State
    private var restoreTask: Task<Void, Never>?
    @State
    private var oauthTokenJson: String?
    @State
    private var isAuthorizing = false
    @State
    private var error: String?

    var body: some View {
        VStack(spacing: 0) {
            Text("Restore from cloud")
                .themeText(.title)
                .padding(.top, ThemeSpace.section)
                .padding(.bottom, ThemeSpace.line)
            Text("Paste your restore code.")
                .themeText(.body)
                .foregroundStyle(.secondary)
                .padding(.bottom, ThemeSpace.edge)
            Form {
                Section("Restore code") {
                    TextField("Paste restore code", text: $restoreCodeInput)
                        .themeText(.mono)
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
                    // A decode is never cancelled, so it always has a line.
                    else if case .failure(let decodeError) = decodedRestore,
                        let line = decodeError.displayLine
                    {
                        Text(line)
                            .foregroundStyle(Theme.danger)
                            .themeText(.body)
                    }
                }
            }
            .formStyle(.grouped)
            .scrollDisabled(true)
            if let error {
                Text(error)
                    .foregroundStyle(Theme.danger)
                    .themeText(.body)
                    .padding(.horizontal)
                    .padding(.bottom, ThemeSpace.related)
            }
            if isRestoring {
                HStack(spacing: ThemeSpace.related) {
                    ProgressView()
                        .controlSize(.small)
                    Text("Restoring library...")
                        .themeText(.body)
                        .foregroundStyle(.secondary)
                }
                .padding(.bottom, ThemeSpace.group)
            }
            HStack(spacing: ThemeSpace.group) {
                Button("Back") {
                    onBack()
                }
                .buttonStyle(.bordered)
                .disabled(isRestoring)
                Button("Restore") {
                    doRestoreFromCode()
                }
                .buttonStyle(PrimaryButtonStyle())
                .disabled(isRestoring || !restoreReady)
                .keyboardShortcut(.defaultAction)
            }
            .padding(.bottom, ThemeSpace.section)
        }
        .padding(.horizontal)
        .onDisappear { restoreTask?.cancel() }
    }

    /// Decodes a non-empty restore code.
    private func decode(
        restoreCode raw: String
    ) -> Result<BridgeRestoreCodeInfo, Error> {
        Result { try setup.decodeRestoreCode(raw) }
    }

    // MARK: - Validation

    /// A decoded code, with its OAuth connection made when the provider needs
    /// one.
    private var restoreReady: Bool {
        guard case .success(let info) = decodedRestore else { return false }
        return info.needsOauth ? oauthTokenJson != nil : true
    }

    // MARK: - Actions

    /// Restores the library from the current input, which the bridge decodes
    /// again.
    private func doRestoreFromCode() {
        let code = restoreCodeInput
        let token = oauthTokenJson
        let restore = setup.restoreFromCode
        runRestore {
            try restore(code, token)
        }
    }

    /// Runs a restore off the main thread, cancelling any earlier one; a
    /// cancelled restore neither opens its library nor clears `isRestoring`.
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
                // The newer restore owns `isRestoring` now.
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
        WelcomeWindowChrome {
            RestoreFromCloudView(
                onLibraryReady: { _ in },
                onBack: {},
            )
        }
        .environment(LibrarySetup.stub())
    }
#endif
