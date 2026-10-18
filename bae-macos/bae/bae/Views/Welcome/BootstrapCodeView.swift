import BaeKit
import SwiftUI

/// Restores a library from a restore code, or joins one by an invitation. The
/// code is typed, pasted or scanned, or is one coven saved in iCloud Keychain;
/// what it decodes to decides which. coven runs any provider sign-in in the
/// browser.
struct BootstrapCodeView: View {
    let onBack: () -> Void

    @State
    private var form: BootstrapCodeForm
    @State
    private var showScanner = false

    init(
        form: BootstrapCodeForm,
        onBack: @escaping () -> Void
    ) {
        _form = State(initialValue: form)
        self.onBack = onBack
    }

    var body: some View {
        @Bindable
        var form = form
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
                if let attempt = form.attempt {
                    AttemptDetailsSection(
                        attempt: attempt,
                        personName: $form.personName,
                        accessKey: $form.accessKey,
                        secretKey: $form.secretKey
                    )
                }
            }
            .formStyle(.grouped)
            .disabled(form.isRunning)
            if let error = form.error {
                ErrorDetailDisclosure(error: error)
                    .padding(.horizontal)
                    .padding(.bottom, ThemeSpace.related)
            }
            if let status = form.status {
                BootstrapStatusView(status: status)
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
                Button(primaryTitle) { form.start() }
                    .buttonStyle(PrimaryButtonStyle())
                    .disabled(!form.ready)
                    .keyboardShortcut(.defaultAction)
            }
            .padding(.bottom, ThemeSpace.section)
        }
        .padding(.horizontal)
        .onChange(of: form.codeInput) { form.decodeCode() }
        .onDisappear { form.cancel() }
        .sheet(isPresented: $showScanner) {
            CodeScannerSheet(
                onScan: { code in
                    form.codeInput = code
                    showScanner = false
                },
                onDismiss: { showScanner = false }
            )
        }
    }

    private var title: LocalizedStringKey {
        switch form.entry {
        case .restore: "Restore from cloud"
        case .join: "Join a library"
        }
    }

    private var subtitle: LocalizedStringKey {
        switch form.entry {
        case .restore(.some):
            "Restore with the code saved in iCloud Keychain."
        case .restore(nil):
            "Scan or paste your restore code."
        case .join:
            "Scan or paste the invitation an admin of the library sent you."
        }
    }

    private var primaryTitle: LocalizedStringKey {
        if case .join = form.attempt { return "Join" }
        return "Restore"
    }

    @ViewBuilder
    private var codeSection: some View {
        @Bindable
        var form = form
        if let saved = form.saved {
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
                TextField("Paste a code", text: $form.codeInput)
                    .themeText(.mono)
                Button("Scan") { showScanner = true }
                if let decodeError = form.decodeError {
                    ErrorDetailDisclosure(error: decodeError)
                }
            }
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

#if DEBUG
    #Preview("Restore") {
        WelcomeWindowChrome {
            BootstrapCodeView(
                form: LibrarySetup.stub()
                    .bootstrapForm(
                        entry: .restore(nil),
                        onLibraryReady: { _, _ in }
                    ),
                onBack: {}
            )
        }
    }

    #Preview("Join") {
        WelcomeWindowChrome {
            BootstrapCodeView(
                form: LibrarySetup.stub()
                    .bootstrapForm(
                        entry: .join,
                        onLibraryReady: { _, _ in }
                    ),
                onBack: {}
            )
        }
    }
#endif
