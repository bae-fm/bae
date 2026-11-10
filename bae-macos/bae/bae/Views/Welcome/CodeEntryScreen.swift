import BaeKit
import SwiftUI

/// The welcome window while a code is entered, scanned or resumed. Enter
/// code, and the S3 key a restore needs, are sheets over the first screen
/// (`background`); a running restore takes the whole window, with Cancel,
/// and a running join takes it with its wait for approval. A restore or join
/// that stops, or a code that doesn't read, brings the sheet back with why.
/// Cancel on the sheet, or leaving the scanner, closes the screen.
struct CodeEntryScreen<Background: View>: View {
    @State
    private var form: BootstrapCodeForm
    /// Whether the scanner shows in place of Enter code, as it does until a
    /// code is scanned.
    @State
    private var scanning: Bool
    private let onClose: () -> Void
    private let background: Background

    /// `scanning` opens the scanner as the screen appears. The screen keeps
    /// the first form it is handed for as long as it shows.
    init(
        form: BootstrapCodeForm,
        scanning: Bool,
        onClose: @escaping () -> Void,
        @ViewBuilder background: () -> Background
    ) {
        _form = State(initialValue: form)
        _scanning = State(initialValue: scanning)
        self.onClose = onClose
        self.background = background()
    }

    var body: some View {
        Group {
            if form.isRunning {
                running
            }
            else {
                background.sheet(
                    isPresented: Binding(
                        get: { true },
                        set: { if !$0 { onClose() } }
                    )
                ) {
                    sheet
                }
            }
        }
        .onAppear { form.begin() }
        // Leaving stops a restore; a join's call is dropped and the join
        // resumes at bae's next start.
        .onDisappear { form.leave() }
    }

    @ViewBuilder
    private var running: some View {
        if let invite = form.attempt?.invite {
            JoinWaitingView(form: form, invite: invite)
        }
        else {
            CodeRunningView(
                openSteps: form.openSteps,
                steps: form.steps,
                status: form.status,
                onCancel: form.cancel
            )
        }
    }

    @ViewBuilder
    private var sheet: some View {
        if scanning {
            CodeScannerSheet(
                onScan: { code in
                    scanning = false
                    form.acceptCode(code)
                },
                onDismiss: onClose
            )
        }
        else if form.attempt?.needsS3Key == true {
            RestoreS3KeySheet(form: form)
        }
        else {
            EnterCodeSheet(form: form, onCancel: onClose)
        }
    }
}
