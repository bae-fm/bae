import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("BootstrapCodeView")

/// Restores a library from a restore code, or joins one by an invitation. The
/// code is typed, pasted or scanned, or is one coven saved in iCloud Keychain;
/// what it decodes to decides which. coven runs any provider sign-in in the
/// browser.
struct BootstrapCodeView: View {
    enum Entry {
        /// A restore, from a saved code or one the person enters.
        case restore(BridgeSavedRestoreCode?)
        case join
    }

    let entry: Entry
    let onLibraryReady: (OpenedLibrary, BridgeError?) -> Void
    let onBack: () -> Void

    @Environment(LibrarySetup.self)
    private var setup

    @State
    private var codeInput = ""
    /// The entered code's kind and metadata, or nil while the field is empty.
    @State
    private var decoded: Result<BridgeCodeInfo, Error>?
    @State
    private var personName = ""
    @State
    private var accessKey = ""
    @State
    private var secretKey = ""
    @State
    private var showScanner = false
    /// The running attempt, which reports its progress.
    @State
    private var bootstrap: LibraryBootstrap?
    @State
    private var task: Task<Void, Never>?
    @State
    private var error: DisplayError?

    private let saved: SavedRestoreEntry?

    init(
        entry: Entry,
        onLibraryReady: @escaping (OpenedLibrary, BridgeError?) -> Void,
        onBack: @escaping () -> Void
    ) {
        self.entry = entry
        self.onLibraryReady = onLibraryReady
        self.onBack = onBack
        if case .restore(let code?) = entry {
            saved = SavedRestoreEntry(code: code, info: code.info())
        }
        else {
            saved = nil
        }
    }

    private var attempt: BootstrapAttempt? {
        if let saved { return .restoreSaved(saved) }
        guard case .success(let info) = decoded else { return nil }
        let code = codeInput.trimmingCharacters(in: .whitespacesAndNewlines)
        return switch info {
        case .restore(let metadata): .restore(code: code, metadata: metadata)
        case .invite(let metadata, let libraryName):
            .join(code: code, metadata: metadata, name: libraryName)
        }
    }

    private var isRunning: Bool { task != nil }

    private var ready: Bool {
        guard let attempt, !isRunning else { return false }
        if attempt.needsS3Key && (accessKey.isEmpty || secretKey.isEmpty) {
            return false
        }
        if case .join = attempt {
            return !personName.trimmingCharacters(in: .whitespaces).isEmpty
        }
        return true
    }

    var body: some View {
        VStack(spacing: 0) {
            Text(title)
                .themeText(.title)
                .padding(.top, ThemeSpace.section)
                .padding(.bottom, ThemeSpace.line)
            Text(subtitle)
                .themeText(.body)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .padding(.bottom, ThemeSpace.edge)
            Form {
                codeSection
                if let attempt {
                    AttemptDetailsSection(
                        attempt: attempt,
                        personName: $personName,
                        accessKey: $accessKey,
                        secretKey: $secretKey
                    )
                }
            }
            .formStyle(.grouped)
            .disabled(isRunning)
            if let error {
                ErrorDetailDisclosure(error: error)
                    .padding(.horizontal)
                    .padding(.bottom, ThemeSpace.related)
            }
            if let status = bootstrap?.status {
                BootstrapStatusView(status: status)
                    .padding(.horizontal)
                    .padding(.bottom, ThemeSpace.group)
            }
            HStack(spacing: ThemeSpace.group) {
                Button(isRunning ? "Cancel" : "Back") {
                    if isRunning {
                        task?.cancel()
                    }
                    else {
                        onBack()
                    }
                }
                .buttonStyle(.bordered)
                Button(primaryTitle) { start() }
                    .buttonStyle(PrimaryButtonStyle())
                    .disabled(!ready)
                    .keyboardShortcut(.defaultAction)
            }
            .padding(.bottom, ThemeSpace.section)
        }
        .padding(.horizontal)
        .onDisappear { task?.cancel() }
        .sheet(isPresented: $showScanner) {
            CodeScannerSheet(
                onScan: { code in
                    codeInput = code
                    showScanner = false
                },
                onDismiss: { showScanner = false }
            )
        }
    }

    private var title: LocalizedStringKey {
        switch entry {
        case .restore: "Restore from cloud"
        case .join: "Join a library"
        }
    }

    private var subtitle: LocalizedStringKey {
        switch entry {
        case .restore(.some):
            "Restore with the code saved in iCloud Keychain."
        case .restore(nil):
            "Scan or paste your restore code."
        case .join:
            "Scan or paste the invitation an admin of the library sent you."
        }
    }

    private var primaryTitle: LocalizedStringKey {
        if case .join = attempt { return "Join" }
        return "Restore"
    }

    @ViewBuilder
    private var codeSection: some View {
        if let saved {
            Section("Saved restore code") {
                LabeledContent(
                    "Provider",
                    value: saved.info.location.displayName
                )
                LabeledContent(
                    "Stored in",
                    value: saved.info.location.locationSummary
                )
            }
        }
        else {
            Section("Code") {
                TextField("Paste a code", text: $codeInput)
                    .themeText(.mono)
                    .onChange(of: codeInput) { _, input in decode(input) }
                Button("Scan") { showScanner = true }
                if case .failure(let decodeError) = decoded {
                    ErrorDetailDisclosure(error: DisplayError(decodeError))
                }
            }
        }
    }
}

// MARK: - Actions

extension BootstrapCodeView {
    private func decode(_ input: String) {
        error = nil
        let code = input.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !code.isEmpty else {
            decoded = nil
            return
        }
        decoded = Result { try decodeCodeInfo(code: code) }
    }

    private func start() {
        guard let attempt else {
            preconditionFailure("the primary button needs a decoded code")
        }
        let credentials =
            attempt.needsS3Key
            ? BridgeS3Credentials(
                accessKeyId: accessKey,
                secretAccessKey: secretKey
            ) : nil
        let bootstrap = setup.makeBootstrap()
        self.bootstrap = bootstrap
        error = nil
        task = Task {
            defer { task = nil }
            do {
                let result: BridgeBootstrapResult
                switch attempt {
                case .restoreSaved(let saved):
                    result = try await bootstrap.restore(
                        saved: saved.code,
                        credentials: credentials
                    )
                case .restore(let code, _):
                    result = try await bootstrap.restore(
                        code: code,
                        credentials: credentials
                    )
                case .join(let code, _, _):
                    result = try await bootstrap.join(
                        code: code,
                        personName: personName.trimmingCharacters(
                            in: .whitespaces
                        )
                    )
                }
                // A result that holds an open store is delivered even after a
                // cancel raced it, so the store is opened rather than leaked.
                deliver(result)
            }
            catch is CancellationError {
                logger.debug("bootstrap cancelled")
            }
            catch {
                logger.error("bootstrap failed: \(error.localizedDescription)")
                self.error = DisplayError(error)
            }
        }
    }

    private func deliver(_ result: BridgeBootstrapResult) {
        switch result {
        case .opened(let library):
            onLibraryReady(library, nil)
        case .published(let library, let warning),
            .profileRequired(let library, let warning):
            onLibraryReady(library, warning)
        case .declined:
            error = DisplayError(
                line: String(
                    localized:
                        "An admin declined this device's request to join."
                )
            )
        case .savedCodeRemoved:
            error = DisplayError(
                line: String(
                    localized:
                        "This restore code is no longer saved in iCloud Keychain."
                )
            )
        }
    }
}

/// What pressing a code screen's primary button does.
private enum BootstrapAttempt {
    case restoreSaved(SavedRestoreEntry)
    case restore(code: String, metadata: BridgeCodeMetadata)
    case join(code: String, metadata: BridgeCodeMetadata, name: String)

    /// Whether this restore needs the person's S3 key typed in: an S3
    /// library whose key isn't saved beside the code.
    var needsS3Key: Bool {
        switch self {
        case .restoreSaved(let saved):
            if case .s3 = saved.info.location {
                return !saved.info.hasS3Credentials
            }
            return false
        case .restore(_, let metadata):
            return metadata.provider == .s3
        case .join:
            return false
        }
    }
}

/// What the code says, the person's name for a join, and the S3 key a
/// restore needs when none came with the code.
private struct AttemptDetailsSection: View {
    let attempt: BootstrapAttempt
    @Binding
    var personName: String
    @Binding
    var accessKey: String
    @Binding
    var secretKey: String

    var body: some View {
        Section {
            switch attempt {
            case .restoreSaved:
                EmptyView()
            case .restore(_, let metadata):
                LabeledContent("Provider", value: metadata.provider.displayName)
                signInNote(metadata)
            case .join(_, let metadata, let libraryName):
                LabeledContent("Library", value: libraryName)
                LabeledContent("Provider", value: metadata.provider.displayName)
                signInNote(metadata)
                TextField("Your name", text: $personName)
            }
            if attempt.needsS3Key {
                TextField("Access Key", text: $accessKey)
                SecureField("Secret Key", text: $secretKey)
            }
        }
    }

    @ViewBuilder
    private func signInNote(_ metadata: BridgeCodeMetadata) -> some View {
        if metadata.oauthAccess != nil {
            Text(
                "You'll sign in to \(metadata.provider.displayName) in your browser."
            )
            .themeText(.detail)
            .foregroundStyle(.secondary)
        }
    }
}

/// What a running restore or join is doing, and anything coven is waiting on.
private struct BootstrapStatusView: View {
    let status: BridgeBootstrapStatus

    var body: some View {
        switch status {
        case .connecting:
            progress("Connecting...")
        case .waitingForApproval:
            progress("Waiting for an admin to approve this device...")
        case .loading:
            progress("Loading the library...")
        case .pending(let records):
            VStack(alignment: .leading, spacing: ThemeSpace.related) {
                progress("Waiting...")
                PendingRecordsView(
                    records: .ready(records: records),
                    perform: nil
                )
            }
        }
    }

    private func progress(_ title: LocalizedStringKey) -> some View {
        HStack(spacing: ThemeSpace.related) {
            ProgressView()
                .controlSize(.small)
            Text(title)
                .themeText(.body)
                .foregroundStyle(.secondary)
        }
    }
}

#if DEBUG
    #Preview("Restore") {
        WelcomeWindowChrome {
            BootstrapCodeView(
                entry: .restore(nil),
                onLibraryReady: { _, _ in },
                onBack: {}
            )
        }
        .environment(LibrarySetup.stub())
    }

    #Preview("Join") {
        WelcomeWindowChrome {
            BootstrapCodeView(
                entry: .join,
                onLibraryReady: { _, _ in },
                onBack: {}
            )
        }
        .environment(LibrarySetup.stub())
    }
#endif
