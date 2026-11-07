// A modal wizard, so sync is either fully configured or not at all.

import BaeKit
import SwiftUI

// MARK: - Wizard Steps

private enum WizardStep: Equatable {
    case selectProvider
    case configure(BridgeCloudProvider)
    /// The storage is connected: the last screen shows the restore code.
    case restoreCode(BridgeCloudProvider)
}

// MARK: - Provider Row

/// The providers this build offers, in the bridge's display order.
private let providerOptions: [BridgeCloudProvider] = availableCloudProviders()

/// A provider in the wizard's list: its radio, name and what it is.
private struct ProviderRow: View {
    let provider: BridgeCloudProvider
    let chosen: Bool

    var body: some View {
        HStack(alignment: .top, spacing: ThemeSpace.group) {
            Image(systemName: chosen ? "largecircle.fill.circle" : "circle")
                .foregroundStyle(chosen ? Color.accentColor : .secondary)

            VStack(alignment: .leading, spacing: ThemeSpace.line) {
                Text(provider.displayName)
                    .themeText(.rowTitle)
                Text(provider.setupSummary)
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
            }

            Spacer()
        }
        .contentShape(Rectangle())
    }
}

// MARK: - SyncSetupWizard

/// Sets up the library's storage: choose a provider and Continue, then its
/// short form or sign-in and folder choice, then the restore code, which is
/// where setup ends for every provider. Success records the location coven
/// connected.
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
    /// The restore code the last screen shows, made once setup is done.
    @State
    private var restoreCode = RecoveryCodeReveal()
    /// The provider the list has selected, which Continue opens.
    @State
    private var choice: BridgeCloudProvider = providerOptions.first ?? .s3
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
        if let only = providerOptions.first, providerOptions.count == 1 {
            _step = State(initialValue: .configure(only))
        }
        _run = State(initialValue: StorageSetupRun(onCancelled: {}))
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
                case .restoreCode:
                    RestoreCodeStep(onDone: onDone, reveal: restoreCode)
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
            Text(headerTitle)
                .themeText(.heading)
            Spacer()
        }
        .padding()
    }

    /// One screen back: from a provider's screen to the list of providers, or
    /// out of the sheet when the build offers one provider and has no list.
    private func leave() {
        guard providerOptions.count > 1 else {
            onDone()
            return
        }
        withAnimation(.easeInOut(duration: 0.15)) {
            step = .selectProvider
        }
    }

    private var headerTitle: String {
        switch step {
        case .selectProvider:
            String(localized: "Set up sync")
        case .configure(let provider), .restoreCode(let provider):
            String(
                localized: "Set up sync: \(provider.displayName)",
                comment:
                    "Set up sync's sheet title once a provider is chosen; the argument is the provider's name"
            )
        }
    }

    // MARK: - Provider List

    private var providerList: some View {
        VStack(spacing: 0) {
            Form {
                Section {
                    Text(
                        "Choose where this library is kept. Everything is encrypted on this Mac before it's uploaded."
                    )
                    .themeText(.body)
                    .foregroundStyle(.secondary)
                }
                Section {
                    ForEach(providerOptions, id: \.self) { provider in
                        Button {
                            choice = provider
                        } label: {
                            ProviderRow(
                                provider: provider,
                                chosen: provider == choice
                            )
                        }
                        .buttonStyle(.plain)
                        .accessibilityAddTraits(
                            provider == choice ? .isSelected : []
                        )
                    }
                }
            }
            .formStyle(.grouped)

            MacSheetFooter {
                Button("Cancel") { onDone() }
                Button("Continue") {
                    withAnimation(.easeInOut(duration: 0.15)) {
                        step = .configure(choice)
                    }
                }
                .buttonStyle(PrimaryButtonStyle())
                .keyboardShortcut(.defaultAction)
            }
            .padding(.horizontal)
            .padding(.bottom, ThemeSpace.section)
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
                    leave: leave,
                    onConnected: connected
                )
            #else
                EmptyView()
            #endif
        case .oneDrive:
            #if BAE_OAUTH_PROVIDERS
                OAuthFolderSetup(
                    browser: cloudSyncSetup.folderBrowser(.oneDrive),
                    leave: leave,
                    onConnected: connected
                )
            #else
                EmptyView()
            #endif
        case .cloudKit:
            #if BAE_CLOUDKIT
                CloudKitSetup(leave: leave, onConnected: connected)
            #else
                EmptyView()
            #endif
        }
    }

    private var s3Form: S3SetupForm {
        #if DEBUG
            if let stagedS3 {
                return S3SetupForm(
                    staged: stagedS3,
                    leave: leave,
                    onConnected: connected
                )
            }
        #endif
        return S3SetupForm(leave: leave, onConnected: connected)
    }

    /// The storage is connected: record its location, and show the restore
    /// code before the sheet closes, whether or not the Mac's keychain
    /// carries it anywhere else.
    private func connected(_ setup: BridgeStorageSetup) {
        configStore.applyStorage(setup.location)
        withAnimation(.easeInOut(duration: 0.15)) {
            step = .restoreCode(choice)
        }
    }
}

#if BAE_CLOUDKIT
    /// Uses the Apple Account this Mac is signed in to.
    private struct CloudKitSetup: View {
        let leave: () -> Void
        let onConnected: (BridgeStorageSetup) -> Void

        @Environment(CloudSyncSetup.self)
        private var cloudSyncSetup

        var body: some View {
            SetupStep(
                ready: true,
                operation: { try await cloudSyncSetup.useCloudKit($0) },
                leave: leave,
                onConnected: onConnected,
                fields: {
                    Section {
                        Text(
                            "This library will be kept in bae's iCloud storage, using the Apple Account this Mac is signed in to."
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

/// Setup's last screen, for every provider: the library's restore code, which
/// gets the person back in if they lose every device. bae can't tell whether
/// the Mac's keychain carries it anywhere else, so setup ends here, and Done
/// lands on the Library settings, which hold the code too.
private struct RestoreCodeStep: View {
    let onDone: () -> Void
    let reveal: RecoveryCodeReveal

    var body: some View {
        VStack(spacing: 0) {
            Form {
                RecoveryCodeSection(reveal: reveal)
            }
            .formStyle(.grouped)

            MacSheetFooter {
                Button("Done") { onDone() }
                    .buttonStyle(PrimaryButtonStyle())
                    .keyboardShortcut(.defaultAction)
            }
            .padding(.horizontal)
            .padding(.bottom, ThemeSpace.section)
        }
        .onDisappear { reveal.hide() }
    }
}

/// A setup step's fields above its footer: Cancel, which goes back one screen
/// and, while the connection is set up, stops it, and the one action. The
/// error stays below the fields while they scroll. While the connection is set
/// up, the step it runs shows over its bar in place of the fields, with only
/// Cancel under it, and a cancel under way shows there in its place.
struct SetupStep<Fields: View>: View {
    let ready: Bool
    /// Set the storage up, writing each step into the setup's stream.
    let operation: (BridgeLibrarySetup) async throws -> BridgeStorageSetup
    /// Go back one screen, when no setup runs.
    let leave: () -> Void
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
            if run.isRunning {
                StorageSetupRunLine(run: run)
                    .padding()
                    .frame(maxWidth: .infinity, alignment: .leading)
                Spacer(minLength: 0)
            }
            else {
                Form {
                    fields
                }
                .formStyle(.grouped)

                if let error {
                    ErrorDetailDisclosure(error: error)
                        .padding(.horizontal)
                        .padding(.bottom, ThemeSpace.related)
                }
            }
            MacSheetFooter {
                if let cancel {
                    Button("Cancel") { cancel() }
                        .disabled(run.isCancelling)
                }
                if !run.isRunning {
                    Button("Set up") {
                        connect()
                    }
                    .buttonStyle(PrimaryButtonStyle())
                    .keyboardShortcut(.defaultAction)
                    .disabled(!ready)
                }
            }
            .padding(.horizontal)
            .padding(.bottom, ThemeSpace.section)
        }
        .onDisappear { run.leave() }
    }

    /// What Cancel does: stop the setup that runs, or go back. Nothing while
    /// the setup is past its point of no return.
    private var cancel: (() -> Void)? {
        if !run.isRunning { return leave }
        guard run.cancelOffered || run.isCancelling else { return nil }
        return { run.cancel(sync) }
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

        private init(
            restoreCode: RecoveryCodeReveal,
            provider: BridgeCloudProvider,
            onDone: @escaping () -> Void
        ) {
            self.onDone = onDone
            _step = State(initialValue: .restoreCode(provider))
            _run = State(initialValue: StorageSetupRun(onCancelled: {}))
            _restoreCode = State(initialValue: restoreCode)
        }
    }

    extension SyncSetupWizard {
        #if BAE_OAUTH_PROVIDERS
            /// The wizard on Dropbox's screen while the person signs in in
            /// their browser, which never answers.
            static func signingInToDropbox() -> some View {
                SyncSetupWizard(
                    provider: .dropbox,
                    run: StorageSetupRun(onCancelled: {}),
                    onDone: {}
                )
                .environment(PreviewData.configStore())
                .environment(PreviewData.previewSync(storage: nil))
                .environment(
                    CloudSyncSetup(signIn: { _ in
                        try await Task.neverAnswers()
                    })
                )
            }
        #endif
    }

    extension SyncSetupWizard {
        /// Setup's last screen for `provider`, in `state`: a fixture code in
        /// a preview or a shot, never a real one.
        static func finished(
            on provider: BridgeCloudProvider,
            holding state: RecoveryCodeReveal.State
        ) -> some View {
            SyncSetupWizard(
                restoreCode: RecoveryCodeReveal(previewState: state),
                provider: provider,
                onDone: {}
            )
            .environment(PreviewData.configStore())
            .environment(PreviewData.previewSync(storage: nil))
            #if BAE_OAUTH_PROVIDERS
                .environment(CloudSyncSetup())
            #endif
        }
    }

    extension SyncSetupWizard {
        /// The wizard as it opens, on its list of providers.
        static func choosing() -> some View {
            SyncSetupWizard(onDone: {})
                .environment(PreviewData.configStore())
                .environment(PreviewData.previewSync(storage: nil))
                #if BAE_OAUTH_PROVIDERS
                    .environment(CloudSyncSetup())
                #endif
        }
    }

    #Preview("Restore code") {
        SyncSetupWizard.finished(
            on: .s3,
            holding: .shown(code: PreviewData.recoveryCode)
        )
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
