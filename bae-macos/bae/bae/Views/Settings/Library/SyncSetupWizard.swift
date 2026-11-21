// A modal wizard, so sync is either fully configured or not at all.

import BaeKit
import SwiftUI

// MARK: - Wizard Steps

enum WizardStep: Equatable {
    case selectProvider
    case configure(BridgeCloudProvider)
    /// The storage is connected: the last screen shows the restore code.
    case restoreCode(BridgeCloudProvider)
}

// MARK: - Providers

/// The providers this build offers, in the bridge's display order.
private let providerOptions: [BridgeCloudProvider] = availableCloudProviders()

// MARK: - SyncSetupWizard

/// Sets up the library's storage: choose a provider and Continue, then its
/// short form or sign-in and folder choice, then the restore code, which is
/// where setup ends for every provider. Success records the location coven
/// connected.
struct SyncSetupWizard: View {
    let onDone: () -> Void

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
    /// The setup a step runs; a cancelled one leaves the wizard on its step
    /// once its call ends.
    @State
    private var run = StorageSetupRun()
    /// The height of the window the sheet hangs from, once the sheet has one.
    @State
    private var parentHeight: CGFloat?

    /// The tallest the sheet grows: its window's height less where the sheet
    /// hangs. A taller step scrolls its form. Nothing limits the sheet
    /// before it has a window.
    private var allowedHeight: CGFloat? {
        parentHeight.map { $0 - sheetHang }
    }

    init(onDone: @escaping () -> Void) {
        self.onDone = onDone
        if let only = providerOptions.first, providerOptions.count == 1 {
            _step = State(initialValue: .configure(only))
        }
    }

    #if DEBUG
        private var staging: SyncSetupWizardStaging?

        // periphery:ignore - called by baeShots
        /// The wizard on the screen `staging` names.
        init(staging: SyncSetupWizardStaging) {
            onDone = {}
            self.staging = staging
            _choice = State(initialValue: staging.choice)
            _run = State(initialValue: staging.run)
            _step = State(initialValue: staging.step)
            _restoreCode = State(initialValue: staging.reveal)
        }
    #endif

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
        } footer: {
            EmptyView()
        }
        .frame(maxHeight: allowedHeight)
        .background(ParentWindowHeight(height: $parentHeight))
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
            String(localized: "Set up a cloud home")
        case .configure(let provider), .restoreCode(let provider):
            String(
                localized: "Set up a cloud home: \(provider.displayName)",
                comment:
                    "Cloud home setup sheet title once a provider is chosen; the argument is the provider's name"
            )
        }
    }

    // MARK: - Provider List

    private var providerList: some View {
        VStack(spacing: 0) {
            Form {
                MacSection {
                    ForEach(providerOptions, id: \.self) { provider in
                        Button {
                            choice = provider
                        } label: {
                            MacRadio(
                                title: Text(provider.displayName),
                                note: Text(provider.setupSummary),
                                checked: provider == choice
                            )
                        }
                        .buttonStyle(.accentPlain)
                        // The row is a radio button of the provider group,
                        // as the board draws it, and says so to VoiceOver.
                        .accessibilityRepresentation {
                            Picker(selection: $choice) {
                                Text(
                                    "\(provider.displayName) \(provider.setupSummary)"
                                )
                                .tag(provider)
                            } label: {
                                EmptyView()
                            }
                            .pickerStyle(.radioGroup)
                        }
                    }
                }
            }
            .formStyle(.grouped)
            .fitsItsContent()

            MacSheetFooter {
                MacButton("Cancel") { onDone() }
                MacButton("Continue", kind: .primary) {
                    withAnimation(.easeInOut(duration: 0.15)) {
                        step = .configure(choice)
                    }
                }
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
                if case .folders(let browser)? = staging?.fields {
                    return browser
                }
            #endif
            return cloudSyncSetup.folderBrowser(provider)
        }
    #endif

    private var s3Form: MacS3Form {
        #if DEBUG
            if case .s3(let form)? = staging?.fields {
                return form
            }
        #endif
        return MacS3Form(leave: leave, onConnected: connected)
    }

    /// The storage is connected: show the restore code before the sheet
    /// closes, whether or not the Mac's keychain carries it anywhere else.
    private func connected(_: BridgeStorageSetup) {
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
                MacRestoreCode(reveal: reveal)
            }
            .formStyle(.grouped)
            .fitsItsContent()

            MacSheetFooter {
                MacButton("Done", kind: .primary) { onDone() }
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
        // periphery:ignore - called by MacS3Form's staged form
        /// This step opening on `failure`, which a shot stages.
        func opening(on failure: DisplayError?) -> Self {
            var step = self
            step._error = State(initialValue: failure)
            return step
        }
    #endif

    var body: some View {
        VStack(spacing: 0) {
            if run.isRunning {
                StorageSetupRunLine(run: run, checkedFolder: checkedFolder)
                    .padding()
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            else {
                if let error {
                    MacNotice(error: error)
                        .padding(.horizontal, ThemeSpace.section)
                        .padding(.top, ThemeSpace.section)
                }
                Form {
                    fields
                }
                .formStyle(.grouped)
                .fitsItsContent()
            }
            MacSheetFooter(
                lead: { if !run.isRunning { leading } },
                trailing: {
                    if let cancel {
                        MacButton("Cancel") { cancel() }
                            .disabled(run.isCancelling)
                    }
                    if !run.isRunning {
                        MacButton("Set up", kind: .primary) {
                            connect()
                        }
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
    /// What a shot stages in `SyncSetupWizard` in place of the person's steps.
    @MainActor
    struct SyncSetupWizardStaging {
        /// What a provider's own screen holds.
        enum Provider {
            /// Dropbox or OneDrive's sign-in or folder choice, as it stands.
            case folders(StorageFolderBrowser)
            /// S3's form, as it stands.
            case s3(MacS3Form)
            /// iCloud's, which has no fields.
            case iCloud
        }

        /// The screen the wizard opens on, with what it holds.
        enum Screen {
            /// The list of providers, with one selected.
            case providers(chosen: BridgeCloudProvider)
            /// A provider's own screen.
            case provider(BridgeCloudProvider, Provider)
            /// The restore code's screen.
            case restoreCode(BridgeCloudProvider, RestoreCodeReveal)
        }

        let screen: Screen

        /// The wizard's step for `screen`.
        var step: WizardStep {
            switch screen {
            case .providers: .selectProvider
            case .provider(let provider, _): .configure(provider)
            case .restoreCode(let provider, _): .restoreCode(provider)
            }
        }

        /// The provider the list has selected.
        var choice: BridgeCloudProvider {
            switch screen {
            case .providers(let chosen): chosen
            case .provider(let provider, _), .restoreCode(let provider, _):
                provider
            }
        }

        /// The provider's own screen, when `screen` is that.
        var fields: Provider? {
            if case .provider(_, let fields) = screen {
                fields
            }
            else {
                nil
            }
        }

        /// The restore code, when `screen` is its screen.
        var reveal: RestoreCodeReveal {
            if case .restoreCode(_, let reveal) = screen {
                reveal
            }
            else {
                RestoreCodeReveal()
            }
        }
        var run = StorageSetupRun()
    }
#endif

/// How far below the top of its window a sheet hangs.
private let sheetHang: CGFloat = 36

extension View {
    /// Draws a grouped form at the height of its rows, so the sheet around
    /// it is as tall as its step needs and its buttons sit right under the
    /// rows they act on. When the rows are taller than the room the sheet
    /// has, the form scrolls and the buttons stay under it.
    fileprivate func fitsItsContent() -> some View {
        ViewThatFits(in: .vertical) {
            scrollDisabled(true).fixedSize(horizontal: false, vertical: true)
            self
        }
    }
}

/// Tells its owner the height of the window the sheet hangs from, when the
/// sheet gets its window and each time that window resizes.
private struct ParentWindowHeight: NSViewRepresentable {
    @Binding
    var height: CGFloat?

    func makeNSView(context: Context) -> HostView {
        let view = HostView()
        view.report = { height = $0 }
        return view
    }

    func updateNSView(_ view: HostView, context: Context) {
        view.report = { height = $0 }
    }

    final class HostView: NSView {
        var report: (CGFloat?) -> Void = { _ in }
        private var resizes: NSObjectProtocol?

        private func stopObserving() {
            if let resizes {
                NotificationCenter.default.removeObserver(resizes)
            }
            resizes = nil
        }

        override func viewWillMove(toWindow newWindow: NSWindow?) {
            super.viewWillMove(toWindow: newWindow)
            stopObserving()
        }

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            stopObserving()
            guard let parent = window?.sheetParent else {
                report(nil)
                return
            }
            report(parent.frame.height)
            resizes = NotificationCenter.default.addObserver(
                forName: NSWindow.didResizeNotification,
                object: parent,
                queue: .main
            ) { [weak self] _ in
                self?.report(parent.frame.height)
            }
        }
    }
}
