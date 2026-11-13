import BaeKit
import SwiftUI

/// The welcome window while a code is entered, scanned or resumed. Enter
/// code, and the S3 key a restore needs, are sheets over the first screen
/// (`background`); an invite's code goes on to the step that asks the
/// person's name, in the window; a running restore takes the whole window,
/// with Cancel, and a running join takes it with its wait for approval, as a
/// running pairing does with the number to check. A join or pairing that ends
/// without letting this device in takes it with one step that says so. A restore or join that stops, or a code that doesn't read, brings the sheet back with why.
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
            else if form.pairingEnded {
                PairingEndedView {
                    form.acknowledgeEnd()
                }
            }
            else if let invite = form.ended {
                JoinEndedView(inviter: invite.inviter) {
                    form.acknowledgeEnd()
                    // A join resumed at launch has no code screen to return to.
                    if case .resume = form.entry { onClose() }
                }
            }
            else if let invite = form.asksName {
                JoinNameView(
                    libraryName: invite.libraryName,
                    name: Binding(
                        get: { form.joinName },
                        set: { form.joinName = $0 }
                    ),
                    error: form.error,
                    canJoin: form.canJoin,
                    onJoin: form.join,
                    onCancel: onClose
                )
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
            // While the join's step shows, the join checks for approval every
            // few seconds.
            JoinWaitingView(
                invite: invite,
                status: form.status,
                steps: form.steps,
                error: form.error,
                cancellable: form.cancellable,
                onCancel: form.cancel
            )
            .onAppear { form.setWaitingScreenShown(true) }
            .onDisappear { form.setWaitingScreenShown(false) }
        }
        else if case .pair = form.attempt {
            // While the pairing's step shows, it checks for the other
            // device's approval every few seconds.
            PairingWaitingView(
                status: form.status,
                steps: form.steps,
                onCancel: form.cancel
            )
            .onAppear { form.setWaitingScreenShown(true) }
            .onDisappear { form.setWaitingScreenShown(false) }
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
