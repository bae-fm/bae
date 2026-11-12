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
    #if BAE_OAUTH_PROVIDERS
        @Environment(CloudSyncSetup.self)
        private var cloudSyncSetup
    #endif

    @State
    private var step: WizardStep = .selectProvider
    /// The restore code the last screen shows, made once setup is done.
    @State
    private var restoreCode = RestoreCodeReveal()
    /// The provider the list has selected, which Continue opens.
    @State
    private var choice: BridgeCloudProvider = providerOptions.first ?? .s3
    /// The setup a step runs; a cancelled one closes the wizard once its
    /// call ends.
    @State
    private var run: StorageSetupRun
    #if DEBUG
        /// The S3 form's fields, when a shot stages them.
        private var stagedS3: S3SetupFields?
        #if BAE_OAUTH_PROVIDERS
            /// The folder picker, when a shot stages it.
            private var stagedBrowser: StorageFolderBrowser?
        #endif
        /// The failure the S3 form shows at its top, when a shot stages one.
        private var stagedTopFailure: DisplayError?
    #endif

    init(onDone: @escaping () -> Void) {
        self.onDone = onDone
        if let only = providerOptions.first, providerOptions.count == 1 {
            _step = State(initialValue: .configure(only))
        }
        _run = State(initialValue: StorageSetupRun(onCancelled: {}))
    }

    var body: some View {
        MacSheet(verbatim: headerTitle, message: headerMessage, width: 460) {
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
        } footer: {
            EmptyView()
        }
        .frame(height: 560)
        .environment(run)
    }

    /// The line under the title, where the step has one.
    private var headerMessage: LocalizedStringKey? {
        switch step {
        case .selectProvider:
            "Choose where this library is kept. Everything is encrypted on this Mac before it is uploaded."
        case .configure(.cloudKit):
            "This library’s cloud home will be bae’s space in iCloud, using the Apple Account this Mac is signed in to."
        case .configure, .restoreCode:
            nil
        }
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
                    ForEach(providerOptions, id: \.self) { provider in
                        Button {
                            choice = provider
                        } label: {
                            ProviderRow(
                                provider: provider,
                                chosen: provider == choice
                            )
                        }
                        .buttonStyle(.accentPlain)
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
                    browser: folderBrowser(.dropbox),
                    leave: leave,
                    onConnected: connected
                )
            #else
                EmptyView()
            #endif
        case .oneDrive:
            #if BAE_OAUTH_PROVIDERS
                OAuthFolderSetup(
                    browser: folderBrowser(.oneDrive),
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

    #if BAE_OAUTH_PROVIDERS
        private func folderBrowser(
            _ provider: StorageFolderBrowser.Provider
        ) -> StorageFolderBrowser {
            #if DEBUG
                if let stagedBrowser { return stagedBrowser }
            #endif
            return cloudSyncSetup.folderBrowser(provider)
        }
    #endif

    private var s3Form: S3SetupForm {
        #if DEBUG
            if let stagedS3 {
                return S3SetupForm(
                    staged: stagedS3,
                    failure: stagedTopFailure,
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
                fields: { EmptyView() }
            )
        }
    }
#endif

/// Setup's last screen, for every provider: the library's restore code, which
/// gets the person back in if they lose every device. bae can't tell whether
/// the Mac's keychain carries it anywhere else, so setup ends here, and Done
/// lands on the Sync page; Settings holds the code too.
private struct RestoreCodeStep: View {
    let onDone: () -> Void
    let reveal: RestoreCodeReveal

    @Environment(Sync.self)
    private var sync

    var body: some View {
        VStack(spacing: 0) {
            Form {
                RestoreCodeSection(reveal: reveal)
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
        // The code is made as this screen opens, unless a shot
        // already holds one.
        .onAppear {
            if reveal.state == .closed {
                reveal.open(sync.generateRestoreCode)
            }
        }
        .onDisappear { reveal.close() }
    }
}

extension SetupStep where Leading == EmptyView {
    init(
        ready: Bool,
        operation:
            @escaping (BridgeLibrarySetup) async throws -> BridgeStorageSetup,
        leave: @escaping () -> Void,
        onConnected: @escaping (BridgeStorageSetup) -> Void,
        claim: @escaping (any Error) -> Bool = { _ in false },
        describe: @escaping (any Error) -> DisplayError? = { _ in nil },
        checkedFolder: String? = nil,
        @ViewBuilder fields: () -> Fields
    ) {
        self.init(
            ready: ready,
            operation: operation,
            leave: leave,
            onConnected: onConnected,
            claim: claim,
            describe: describe,
            checkedFolder: checkedFolder,
            leading: { EmptyView() },
            fields: fields
        )
    }
}

/// A setup step's fields above its footer: Cancel, which goes back one screen
/// and, while the connection is set up, stops it, and the one action. A
/// failure shows at the top of the form, over the fields. While the
/// connection is set up, the step it runs shows over its bar in place of the
/// fields, with only Cancel under it, and a cancel under way shows there in
/// its place.
struct SetupStep<Fields: View, Leading: View>: View {
    let ready: Bool
    /// Set the storage up, writing each step into the setup's stream.
    let operation: (BridgeLibrarySetup) async throws -> BridgeStorageSetup
    /// Go back one screen, when no setup runs.
    let leave: () -> Void
    let onConnected: (BridgeStorageSetup) -> Void
    /// Offered a failed setup before the step shows it at the top: true when
    /// the fields show it instead.
    let claim: (any Error) -> Bool
    /// The words a failed setup shows at the top, for the failures the step
    /// words itself; nil leaves core's.
    let describe: (any Error) -> DisplayError?
    /// The name of the folder the person chose, which the setup's check of it
    /// speaks of; nil when none was chosen.
    let checkedFolder: String?
    /// What sits at the left of the footer, opposite the actions.
    let leading: Leading
    let fields: Fields

    init(
        ready: Bool,
        operation:
            @escaping (BridgeLibrarySetup) async throws -> BridgeStorageSetup,
        leave: @escaping () -> Void,
        onConnected: @escaping (BridgeStorageSetup) -> Void,
        claim: @escaping (any Error) -> Bool = { _ in false },
        describe: @escaping (any Error) -> DisplayError? = { _ in nil },
        checkedFolder: String? = nil,
        @ViewBuilder leading: () -> Leading,
        @ViewBuilder fields: () -> Fields
    ) {
        self.ready = ready
        self.operation = operation
        self.leave = leave
        self.onConnected = onConnected
        self.claim = claim
        self.describe = describe
        self.checkedFolder = checkedFolder
        self.leading = leading()
        self.fields = fields()
    }

    @Environment(Sync.self)
    private var sync
    @Environment(StorageSetupRun.self)
    private var run
    @State
    private var error: DisplayError?
    #if DEBUG
        /// A failure a shot shows at the top in place of a run's.
        private var stagedError: DisplayError?

        func staging(error: DisplayError?) -> Self {
            var step = self
            step.stagedError = error
            return step
        }
    #endif

    /// The failure the form shows at its top.
    private var shownError: DisplayError? {
        #if DEBUG
            if let stagedError { return stagedError }
        #endif
        return error
    }

    var body: some View {
        VStack(spacing: 0) {
            if run.isRunning {
                StorageSetupRunLine(run: run, checkedFolder: checkedFolder)
                    .padding()
                    .frame(maxWidth: .infinity, alignment: .leading)
                Spacer(minLength: 0)
            }
            else {
                if let shownError {
                    Notice(error: shownError)
                        .padding(.horizontal, ThemeSpace.section)
                        .padding(.top, ThemeSpace.section)
                }
                Form {
                    fields
                }
                .formStyle(.grouped)
            }
            MacSheetFooter(
                lead: { if !run.isRunning { leading } },
                trailing: {
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
            )
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
                if !claim(failure) {
                    error = describe(failure) ?? DisplayError(failure)
                }
            }
        )
    }
}

#if DEBUG
    // MARK: - Previews

    extension SyncSetupWizard {
        /// Where a shot stages the wizard's S3 setup.
        enum SetupStaging {
            /// Running `step`, before its point of no return.
            case running(BridgeStorageSetupStep)
            /// Running `step`, past its point of no return.
            case finishing(BridgeStorageSetupStep)
            /// The person cancelled it while it connected.
            case cancelling
            /// The setup failed with `failure`, the form as typed.
            case failed(BridgeStorageFailure)
            /// The service refused the key, which shows at the top.
            case refusedKey(S3SetupFields.KeyRejection)
        }

        /// The S3 form's fields as a shot types them.
        private static func stagedFields() -> S3SetupFields {
            let fields = S3SetupFields()
            fields.endpoint = "https://s3.home.lan"
            fields.region = "us-east-1"
            fields.bucket = "my-bucket"
            fields.accessKey = "a1b2c3d4e5f6"
            fields.secretKey = "abcdefghijkl"
            return fields
        }

        /// The wizard on the S3 form, its setup staged at `staging` in
        /// place of core's.
        static func staged(_ staging: SetupStaging) -> some View {
            let sync = Sync(cancelStorageSetup: { .undoing })
            let (step, finishing): (BridgeStorageSetupStep, Bool) =
                switch staging {
                case .running(let step): (step, false)
                case .finishing(let step): (step, true)
                case .cancelling, .failed, .refusedKey: (.connecting, false)
                }
            let run = StorageSetupRun(onCancelled: {})
            if case .failed = staging {
            }
            else if case .refusedKey = staging {
            }
            else {
                run.start(
                    StorageSetupFollowing(
                        staging: BridgeStorageSetupProgress(
                            step: step,
                            key: "core.library.storage_setup_step.connecting",
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
                let fields = stagedFields()
                _ = fields.place(
                    BridgeError.Diagnostic(
                        category: .storage(failure: failure),
                        detail: ""
                    )
                )
                wizard.stagedS3 = fields
            }
            if case .refusedKey(let rejection) = staging {
                let fields = stagedFields()
                wizard.stagedS3 = fields
                wizard.stagedTopFailure = DisplayError(line: rejection.words)
            }
            return
                wizard
                .environment(PreviewData.configStore())
                .environment(sync)
                #if BAE_OAUTH_PROVIDERS
                    .environment(CloudSyncSetup())
                #endif
        }

        #if BAE_OAUTH_PROVIDERS
        #endif

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
            restoreCode: RestoreCodeReveal,
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
        #endif
    }

    extension SyncSetupWizard {
        /// Setup's last screen for `provider`, in `state`: a fixture code in
        /// a shot, never a real one.
        static func finished(
            on provider: BridgeCloudProvider,
            holding state: RestoreCodeReveal.State
        ) -> some View {
            SyncSetupWizard(
                restoreCode: RestoreCodeReveal(previewState: state),
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
        /// The wizard on iCloud's screen, before setup starts.
        static func iCloud() -> some View {
            SyncSetupWizard(
                provider: .cloudKit,
                run: StorageSetupRun(onCancelled: {}),
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

    #if BAE_OAUTH_PROVIDERS

    #endif
#endif
