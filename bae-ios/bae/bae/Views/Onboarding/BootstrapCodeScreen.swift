import BaeKit
import SwiftUI

/// Restores a library with a restore code, or joins one by an invitation. The
/// code is pasted or scanned, or is one coven saved in iCloud Keychain; what it
/// decodes to decides which. Provider sign-in opens in the system's sign-in
/// sheet.
struct BootstrapCodeScreen: View {
    let onBack: () -> Void

    @State
    private var form: BootstrapCodeForm
    @State
    private var showScanner = false
    @State
    private var scanError: String?

    init(form: BootstrapCodeForm, onBack: @escaping () -> Void) {
        _form = State(initialValue: form)
        self.onBack = onBack
    }

    var body: some View {
        NavigationStack {
            List {
                codeSection
                if let attempt = form.attempt {
                    AttemptDetailsSection(attempt: attempt, form: form)
                }
                if let status = form.status {
                    Section {
                        BootstrapStatusView(status: status)
                    }
                }
                if let error = form.error {
                    Section {
                        ErrorDetailDisclosure(error: error)
                    }
                }
            }
            .disabled(form.isRunning)
            .navigationTitle(title)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button(form.isRunning ? "Cancel" : "Back") {
                        if form.isRunning {
                            form.cancel()
                        }
                        else {
                            onBack()
                        }
                    }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button(primaryTitle) { form.start() }
                        .disabled(!form.ready)
                }
            }
        }
        .onChange(of: form.codeInput) { form.decodeCode() }
        .onDisappear { form.cancel() }
        .fullScreenCover(isPresented: $showScanner) {
            ScannerSheet(
                onScanned: { code in
                    showScanner = false
                    form.acceptScan(code)
                },
                onError: { message in
                    showScanner = false
                    scanError = message
                },
                onClose: { showScanner = false }
            )
        }
    }

    private var title: LocalizedStringKey {
        switch form.entry {
        case .restore(.some): "Restore from iCloud Keychain"
        case .restore(nil): "Add device"
        case .join: "Join library"
        case .restoreOver: "Restore this library"
        case .finishRestore: "Finish restoring this library"
        }
    }

    private var subtitle: Text {
        switch form.entry {
        case .restore(.some):
            Text("Restore with the code saved in iCloud Keychain.")
        case .restore(nil):
            Text(
                "Scan or paste the code shown by Add Device… on one of your devices. If you've lost every device, use your recovery code."
            )
        case .join:
            Text(
                "Scan or paste the invitation an admin of the library sent you."
            )
        case .restoreOver:
            Text(localizedCoreString("core.sync.restore_device.warning"))
        case .finishRestore:
            Text(
                BridgeErrorCategory.replacementUnfinished(by: .restore)
                    .localizedLine
            )
        }
    }

    /// A join joins. A code entered on a new device adds this device to the
    /// person's own library, whichever choice took it; a code saved in iCloud
    /// Keychain, or one restoring over this device's copy, restores.
    private var primaryTitle: LocalizedStringKey {
        switch (form.attempt, form.entry) {
        case (.some(.join), _): "Join"
        case (_, .restore(nil)), (_, .join): "Add"
        case (_, .restore(.some)), (_, .restoreOver): "Restore"
        case (_, .finishRestore): "Finish Restoring"
        }
    }

    @ViewBuilder
    private var codeSection: some View {
        @Bindable
        var form = form
        if let saved = form.saved {
            Section {
                LabeledContent(
                    "Stored in",
                    value: saved.info.location.accountLine
                )
            } header: {
                Text("Saved recovery code")
            } footer: {
                subtitle
            }
        }
        else {
            Section {
                HStack(spacing: ThemeSpace.related) {
                    TextField("Paste a code", text: $form.codeInput)
                        .themeText(.mono)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                    Button("Scan") {
                        scanError = nil
                        CameraPermission.requestThenScan(
                            present: { showScanner = true },
                            onError: { scanError = $0 }
                        )
                    }
                    .buttonStyle(.borderless)
                }
                if let decodeError = form.decodeError {
                    ErrorDetailDisclosure(error: decodeError)
                }
                if let scanError {
                    ErrorText(scanError)
                }
            } header: {
                Text("Code")
            } footer: {
                subtitle
            }
        }
    }
}

/// What the code says, the person's name for a join, and the S3 key a restore
/// needs when none came with the code.
private struct AttemptDetailsSection: View {
    let attempt: BootstrapAttempt
    let form: BootstrapCodeForm

    var body: some View {
        @Bindable
        var form = form
        Section {
            switch attempt {
            case .restoreSaved:
                EmptyView()
            case .restore(_, let metadata, let s3Key):
                LabeledContent("Stored in", value: metadata.location.accountLine)
                signInNote(metadata)
                if s3Key != nil {
                    Text("Your S3 access key came with the scanned code.")
                        .themeText(.detail)
                        .foregroundStyle(.secondary)
                }
            case .join(_, let metadata, let libraryName):
                LabeledContent("Library", value: libraryName)
                LabeledContent("Stored in", value: metadata.location.accountLine)
                signInNote(metadata)
                TextField("Your name", text: $form.personName)
                    .textContentType(.name)
            }
            if attempt.needsS3Key {
                TextField("Access Key", text: $form.accessKey)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                SecureField("Secret Key", text: $form.secretKey)
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

#if DEBUG
#Preview("Restore") {
    BootstrapCodeScreen(
        form: LibrarySetup.stub()
            .bootstrapForm(entry: .restore(nil), onLibraryReady: { _, _ in }),
        onBack: {}
    )
}

#Preview("Join") {
    BootstrapCodeScreen(
        form: LibrarySetup.stub()
            .bootstrapForm(entry: .join, onLibraryReady: { _, _ in }),
        onBack: {}
    )
}
#endif
