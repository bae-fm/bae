import BaeKit
import SwiftUI

/// Restores a library from a restore code, or joins one by an invitation. The
/// code is typed, pasted or scanned, or is one coven saved in iCloud Keychain;
/// what it decodes to decides which, and an invite joins as soon as it reads.
/// coven runs any provider sign-in in the browser. A join waiting for approval
/// shows its own screen, which has no cancel once its request may have been
/// sent.
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
        Group {
            if form.isRunning, let invite = form.attempt?.invite {
                JoinWaitingView(form: form, invite: invite)
            }
            else {
                codeForm
            }
        }
        .onAppear { form.begin() }
        // Leaving stops a restore; a join's call is dropped and the join
        // resumes at bae's next start.
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
        .sheet(isPresented: $showScanner) {
            CodeScannerSheet(
                onScan: { code in
                    form.acceptScan(code)
                    showScanner = false
                },
                onDismiss: { showScanner = false }
            )
        }
    }

    private var title: LocalizedStringKey {
        switch form.entry {
        case .restore(.some): "Restore from iCloud Keychain"
        case .restore(nil): "Add device"
        case .join, .resume: "Join library"
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
        case .join, .resume:
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
        case (.some(.join), _), (.some(.resume), _), (_, .resume): "Join"
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
            Section("Saved recovery code") {
                LabeledContent("Library", value: saved.info.name)
                LabeledContent(
                    "Stored in",
                    value: saved.info.location.accountLine
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
            case .restore(_, let metadata, let s3Key):
                LabeledContent(
                    "Stored in",
                    value: metadata.location.accountLine
                )
                signInNote(metadata)
                if s3Key != nil {
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

/// A join running: the library's name, what it waits on, and, while it waits
/// for approval, a sent invite's confirmation number with when it checks next
/// and Check Now. Cancel shows only until the request may have been sent.
private struct JoinWaitingView: View {
    let form: BootstrapCodeForm
    let invite: BridgeInviteInfo

    var body: some View {
        VStack(spacing: ThemeSpace.group) {
            Spacer()
            Text(verbatim: invite.libraryName)
                .themeText(.title)
            if let status = form.status {
                BootstrapStatusView(status: status)
            }
            if let error = form.error {
                ErrorDetailDisclosure(error: error)
            }
            HStack(spacing: ThemeSpace.group) {
                if form.cancellable {
                    Button("Cancel") { form.cancel() }
                        .buttonStyle(.bordered)
                }
                if case .waitingForApproval(_, let nextCheck) = form.status {
                    Button("Check Now") { form.checkNow() }
                        .buttonStyle(PrimaryButtonStyle())
                        .disabled(!nextCheck.endsOnCheckNow)
                }
            }
            Spacer()
        }
        .padding(.horizontal)
    }
}

extension BridgeNextCheck {
    /// Whether Check Now ends this wait: the join's own backoff, but neither a
    /// check already running nor a provider's cooldown.
    fileprivate var endsOnCheckNow: Bool {
        if case .at = self { return true }
        return false
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
