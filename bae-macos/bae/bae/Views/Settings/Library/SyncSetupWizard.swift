// A modal wizard, so sync is either fully configured or not at all.

import BaeKit
import SwiftUI

// MARK: - Wizard Steps

private enum WizardStep: Equatable {
    case selectProvider
    case configure(BridgeCloudProvider)
}

// MARK: - Provider Row

/// The providers this build offers, in the bridge's display order.
private let providerOptions: [BridgeCloudProvider] = availableCloudProviders()

/// A provider in the wizard's list: its icon, name and blurb.
private struct ProviderRow: View {
    let provider: BridgeCloudProvider

    var body: some View {
        HStack(spacing: ThemeSpace.group) {
            Image(systemName: provider.setupSymbol)
                .frame(width: 24)
                .foregroundStyle(.secondary)

            VStack(alignment: .leading, spacing: ThemeSpace.line) {
                Text(provider.displayName)
                    .themeText(.rowTitle)
                Text(provider.setupSummary)
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
            }

            Spacer()

            Image(systemName: "chevron.right")
                .themeIcon(.small)
                .foregroundStyle(.tertiary)
        }
        .contentShape(Rectangle())
        .padding(.horizontal, ThemeSpace.edge)
        .padding(.vertical, ThemeSpace.related)
    }
}

// MARK: - SyncSetupWizard

/// Sets up the library's storage: pick a provider, then its short form or
/// sign-in and folder choice. Success records the location coven connected.
struct SyncSetupWizard: View {
    let onDone: () -> Void

    @Environment(ConfigStore.self)
    private var configStore
    @Environment(Sync.self)
    private var sync
    #if BAE_OAUTH_PROVIDERS
        @Environment(CloudSyncSetup.self)
        private var cloudSyncSetup
    #endif

    @State
    private var step: WizardStep = .selectProvider
    /// The setup a step runs; a cancelled one closes the wizard once its
    /// call ends.
    @State
    private var run: StorageSetupRun
    #if DEBUG
        /// The S3 form's fields, when a preview or a shot stages them.
        private var stagedS3: S3SetupFields?
    #endif

    init(onDone: @escaping () -> Void) {
        self.onDone = onDone
        _run = State(initialValue: StorageSetupRun(onCancelled: onDone))
    }

    var body: some View {
        VStack(spacing: 0) {
            header

            Divider()

            Group {
                switch step {
                case .selectProvider:
                    providerList
                case .configure(let provider):
                    configureStep(for: provider)
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .frame(width: 460, height: 560)
        .environment(run)
    }

    // MARK: - Header

    private var header: some View {
        HStack {
            if case .configure = step, !run.isRunning {
                Button {
                    withAnimation(.easeInOut(duration: 0.15)) {
                        step = .selectProvider
                    }
                } label: {
                    Image(systemName: "chevron.left")
                }
                .buttonStyle(.borderless)
            }

            Text(headerTitle)
                .themeText(.heading)

            Spacer()

            if !run.isRunning || run.cancelOffered {
                Button("Cancel") {
                    cancel()
                }
                .buttonStyle(.borderless)
            }
        }
        .padding()
    }

    /// Close the wizard. A running setup is cancelled first, and the wizard
    /// closes once coven has undone it.
    private func cancel() {
        if run.isRunning {
            run.cancel(sync)
        }
        else {
            onDone()
        }
    }

    private var headerTitle: String {
        switch step {
        case .selectProvider:
            String(localized: "Set up sync")
        case .configure(let provider):
            provider.displayName
        }
    }

    // MARK: - Provider List

    private var providerList: some View {
        ScrollView {
            VStack(spacing: ThemeSpace.hairline) {
                ForEach(providerOptions, id: \.self) { provider in
                    Button {
                        withAnimation(.easeInOut(duration: 0.15)) {
                            step = .configure(provider)
                        }
                    } label: {
                        ProviderRow(provider: provider)
                    }
                    .buttonStyle(.plain)
                }
            }
            .padding(.vertical, ThemeSpace.related)
        }
    }

    // MARK: - Configure Step

    /// A provider compiled out of this build can't be selected.
    @ViewBuilder
    private func configureStep(for provider: BridgeCloudProvider) -> some View {
        switch provider {
        case .s3:
            s3Form
        case .dropbox:
            #if BAE_OAUTH_PROVIDERS
                OAuthFolderSetup(
                    browser: cloudSyncSetup.folderBrowser(.dropbox),
                    onConnected: connected
                )
            #else
                EmptyView()
            #endif
        case .oneDrive:
            #if BAE_OAUTH_PROVIDERS
                OAuthFolderSetup(
                    browser: cloudSyncSetup.folderBrowser(.oneDrive),
                    onConnected: connected
                )
            #else
                EmptyView()
            #endif
        case .cloudKit:
            #if BAE_CLOUDKIT
                CloudKitSetup(onConnected: connected)
            #else
                EmptyView()
            #endif
        }
    }

    private var s3Form: S3SetupForm {
        #if DEBUG
            if let stagedS3 {
                return S3SetupForm(staged: stagedS3, onConnected: connected)
            }
        #endif
        return S3SetupForm(onConnected: connected)
    }

    private func connected(_ setup: BridgeStorageSetup) {
        configStore.applyStorage(setup.location)
        onDone()
    }
}

#if BAE_CLOUDKIT
    /// Uses the Apple Account this Mac is signed in to.
    private struct CloudKitSetup: View {
        let onConnected: (BridgeStorageSetup) -> Void

        @Environment(CloudSyncSetup.self)
        private var cloudSyncSetup

        var body: some View {
            SetupStep(
                actionTitle: "Use iCloud",
                ready: true,
                operation: { try await cloudSyncSetup.useCloudKit($0) },
                onConnected: onConnected,
                fields: {
                    Section {
                        Text(
                            "Uses iCloud for sync. Requires iCloud to be enabled in System Settings."
                        )
                        .themeText(.body)
                        .foregroundStyle(.secondary)
                        .frame(maxWidth: .infinity, alignment: .leading)
                    }
                }
            )
        }
    }
#endif

/// A setup step's fields above its one action: the error and the button stay
/// below the fields while they scroll. While the connection is set up, the
/// step it runs shows over its bar above the button, and a cancel under way
/// shows there in its place.
struct SetupStep<Fields: View>: View {
    let actionTitle: LocalizedStringKey
    let ready: Bool
    /// Set the storage up, writing each step into the setup's stream.
    let operation: (BridgeLibrarySetup) async throws -> BridgeStorageSetup
    let onConnected: (BridgeStorageSetup) -> Void
    /// Offered a failed setup before the step shows it at the top: true when
    /// a field shows it instead.
    var claim: (any Error) -> Bool = { _ in false }
    @ViewBuilder
    let fields: Fields

    @Environment(Sync.self)
    private var sync
    @Environment(StorageSetupRun.self)
    private var run
    @State
    private var error: DisplayError?

    var body: some View {
        VStack(spacing: 0) {
            Form {
                fields
            }
            .formStyle(.grouped)
            .disabled(run.isRunning)

            if let error {
                ErrorDetailDisclosure(error: error)
                    .padding(.horizontal)
                    .padding(.bottom, ThemeSpace.related)
            }

            if run.isRunning {
                StorageSetupRunLine(run: run)
                    .padding(.horizontal)
                    .padding(.bottom, ThemeSpace.related)
            }
            MacSheetFooter {
                Button(actionLabel) {
                    connect()
                }
                .buttonStyle(PrimaryButtonStyle())
                .keyboardShortcut(.defaultAction)
                .disabled(run.isRunning || !ready)
            }
            .padding(.horizontal)
            .padding(.bottom, ThemeSpace.section)
        }
        .onDisappear { run.leave() }
    }

    /// The action's words: its title, then what the running setup does,
    /// connecting or, once the person cancelled it, cancelling.
    private var actionLabel: LocalizedStringKey {
        switch run.state {
        case .idle: actionTitle
        case .running: "Connecting…"
        case .cancelling: "Cancelling…"
        }
    }

    private func connect() {
        error = nil
        run.start(
            sync.followStorageSetup(),
            operation: operation,
            onConnected: onConnected,
            onFailed: { failure in
                if !claim(failure) { error = DisplayError(failure) }
            }
        )
    }
}

#if DEBUG
    // MARK: - Previews

    extension SyncSetupWizard {
        /// Where a preview or a shot stages the wizard's S3 setup.
        enum SetupStaging {
            /// Running `step`, before its point of no return.
            case running(BridgeStorageSetupStep)
            /// Running `step`, past its point of no return.
            case finishing(BridgeStorageSetupStep)
            /// The person cancelled it while it connected.
            case cancelling
            /// The setup failed with `failure`, the form as typed.
            case failed(BridgeStorageFailure)
        }

        /// The wizard on the S3 form, its setup staged at `staging` in
        /// place of core's.
        static func staged(_ staging: SetupStaging) -> some View {
            let sync = Sync(cancelStorageSetup: { .undoing })
            let (step, finishing): (BridgeStorageSetupStep, Bool) =
                switch staging {
                case .running(let step): (step, false)
                case .finishing(let step): (step, true)
                case .cancelling, .failed: (.connecting, false)
                }
            let run = StorageSetupRun(onCancelled: {})
            if case .failed = staging {
            }
            else {
                run.start(
                    StorageSetupFollowing(
                        staging: BridgeStorageSetupProgress(
                            step: step,
                            done: 0,
                            total: 1
                        ),
                        finishing: finishing
                    ),
                    operation: { _ in try await Task.neverAnswers() },
                    onConnected: { _ in },
                    onFailed: { _ in }
                )
            }
            if case .cancelling = staging {
                run.cancel(sync)
            }
            var wizard = SyncSetupWizard(provider: .s3, run: run, onDone: {})
            if case .failed(let failure) = staging {
                let fields = S3SetupFields()
                fields.endpoint = "https://s3.home.lan"
                fields.region = "us-east-1"
                fields.bucket = "my-bucket"
                fields.accessKey = "a1b2c3d4e5f6"
                fields.secretKey = "abcdefghijkl"
                _ = fields.place(
                    BridgeError.Diagnostic(
                        category: .storage(failure: failure),
                        detail: ""
                    )
                )
                wizard.stagedS3 = fields
            }
            return
                wizard
                .environment(PreviewData.configStore())
                .environment(sync)
                #if BAE_OAUTH_PROVIDERS
                    .environment(CloudSyncSetup())
                #endif
        }

        private init(
            provider: BridgeCloudProvider,
            run: StorageSetupRun,
            onDone: @escaping () -> Void
        ) {
            self.onDone = onDone
            _step = State(initialValue: .configure(provider))
            _run = State(initialValue: run)
        }
    }

    #Preview("Provider Selection") {
        SyncSetupWizard(onDone: {})
            .environment(PreviewData.configStore())
            .environment(PreviewData.previewSync(storage: nil))
            #if BAE_OAUTH_PROVIDERS
                .environment(CloudSyncSetup())
            #endif
    }

    #Preview("Connecting") {
        SyncSetupWizard.staged(.running(.connecting))
    }

    #Preview("Creating the library") {
        SyncSetupWizard.staged(.finishing(.creatingStore))
    }

    #Preview("Cancelling") {
        SyncSetupWizard.staged(.cancelling)
    }
#endif
