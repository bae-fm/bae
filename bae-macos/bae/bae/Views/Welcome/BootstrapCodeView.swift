import BaeKit
import SwiftUI

/// Restores a library over this device's copy, or from a code saved in iCloud
/// Keychain. The code is typed, pasted or scanned, or is the saved one; it
/// is read when the person chooses Restore. coven runs any provider sign-in in the browser.
struct BootstrapCodeView: View {
    let onBack: () -> Void

    @State
    private var form: BootstrapCodeForm
    @State
    private var showScanner = false
    private let screen: Screen

    /// What the screen says for each entry it is made for.
    private enum Screen {
        case restoreSaved
        case restoreOver
        case finishRestore

        init(_ entry: BootstrapCodeForm.Entry) {
            switch entry {
            case .restoreSaved: self = .restoreSaved
            case .restoreOver: self = .restoreOver
            case .finishRestore: self = .finishRestore
            case .code, .resume, .resumeRestore:
                preconditionFailure(
                    "the code sheets show this entry, not this screen"
                )
            }
        }
    }

    init(form: BootstrapCodeForm, onBack: @escaping () -> Void) {
        _form = State(initialValue: form)
        screen = Screen(form.entry)
        self.onBack = onBack
    }

    var body: some View {
        codeForm
            .onAppear { form.begin() }
            // Leaving stops a restore.
            .onDisappear { form.leave() }
    }

    private var codeForm: some View {
        @Bindable
        var form = form
        return VStack(spacing: 0) {
            Text(title)
                .themeText(.title)
                .padding(.top, ThemeSpace.section)
                .padding(.bottom, ThemeSpace.line)
            subtitle
                .themeText(.body)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .padding(.bottom, ThemeSpace.edge)
            Form {
                codeSection
                if let attempt = form.attempt {
                    AttemptDetailsSection(
                        attempt: attempt,
                        accessKey: $form.accessKey,
                        secretKey: $form.secretKey
                    )
                }
            }
            .formStyle(.grouped)
            .disabled(form.isRunning)
            if let error = form.error {
                Notice(error: error)
                    .padding(.horizontal)
                    .padding(.bottom, ThemeSpace.related)
            }
            if let status = form.status {
                MacProgressContainer(steps: form.openSteps) {
                    MacProgressContainer(steps: form.steps) {
                        BootstrapStatusView(status: status)
                    }
                }
                .padding(.horizontal)
                .padding(.bottom, ThemeSpace.group)
            }
            HStack(spacing: ThemeSpace.group) {
                Button(form.isRunning ? "Cancel" : "Back") {
                    if form.isRunning {
                        form.cancel()
                    }
                    else {
                        onBack()
                    }
                }
                .buttonStyle(.bordered)
                Button(primaryTitle) { form.submit() }
                    .buttonStyle(PrimaryButtonStyle())
                    .disabled(!form.canContinue)
                    .keyboardShortcut(.defaultAction)
            }
            .padding(.bottom, ThemeSpace.section)
        }
        .padding(.horizontal)
        .sheet(isPresented: $showScanner) {
            CodeScannerSheet(
                onScan: { code in
                    form.acceptCode(code)
                    showScanner = false
                },
                onDismiss: { showScanner = false }
            )
        }
    }

    private var title: LocalizedStringKey {
        switch screen {
        case .restoreSaved: "Restore from iCloud Keychain"
        case .restoreOver: "Restore this library"
        case .finishRestore: "Finish restoring this library"
        }
    }

    private var subtitle: Text {
        switch screen {
        case .restoreSaved:
            Text("Restore with the code saved in iCloud Keychain.")
        case .restoreOver:
            Text(localizedCoreString("core.sync.restore_device.warning"))
        case .finishRestore:
            Text(
                BridgeErrorCategory.replacementUnfinished(by: .restore)
                    .localizedLine
            )
        }
    }

    private var primaryTitle: LocalizedStringKey {
        switch screen {
        case .restoreSaved, .restoreOver: "Restore"
        case .finishRestore: "Finish restoring"
        }
    }

    @ViewBuilder
    private var codeSection: some View {
        if let saved = form.saved {
            Section("Saved restore code") {
                LabeledContent("Library", value: saved.info.name)
                LabeledContent(
                    "Stored in",
                    value: saved.info.location.accountLine
                )
            }
        }
        else {
            Section("Code") {
                CodeField(form: form)
                Button("Scan") { showScanner = true }
                if let decodeError = form.decodeError {
                    Notice(error: decodeError)
                }
            }
        }
    }
}

/// What the code says, and the S3 key a restore needs when none came with
/// the code.
private struct AttemptDetailsSection: View {
    let attempt: BootstrapAttempt
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
                LabeledContent(
                    "Stored in",
                    value: metadata.location.accountLine
                )
                signInNote(metadata)
                if metadata.provider == .s3 && metadata.carriesS3Key {
                    Text("Your S3 access key came with the scanned code.")
                        .themeText(.detail)
                        .foregroundStyle(.secondary)
                }
            case .join(_, let invite), .resume(_, let invite):
                LabeledContent("Library", value: invite.libraryName)
                LabeledContent(
                    "Stored in",
                    value: invite.metadata.location.accountLine
                )
                signInNote(invite.metadata)
            }
            if attempt.needsS3Key {
                TextField("Access key", text: $accessKey)
                SecureField("Secret key", text: $secretKey)
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
    #Preview("Restore over") {
        MacWelcomeWindow {
            BootstrapCodeView(
                form: LibrarySetup.stub()
                    .bootstrapForm(
                        entry: .restoreOver(libraryId: "library-preview"),
                        onLibraryReady: { _, _ in }
                    ),
                onBack: {}
            )
        }
    }
#endif
