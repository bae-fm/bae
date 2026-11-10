import BaeKit
import SwiftUI

/// Offered when opening a library found a damaged database or another copy of
/// this device, or from Settings when sync found one: why, as a centred
/// notice, with the reset, whose warning an alert shows before it runs. A
/// reset that failed says so, and Retry runs it again without the warning the
/// person already accepted. Also offers finishing a reset that stopped
/// partway, once finishing it failed. Cancel leaves either without resetting.
struct DeviceResetView: View {
    /// Why the screen asks for a reset.
    enum Prompt {
        /// This device's copy can't be used; resetting discards its unsynced
        /// edits, which the warning says before the person chooses.
        case offered
        /// A reset already under way stopped partway, and finishing it
        /// failed; finishing tries again.
        case unfinished
    }

    let prompt: Prompt
    /// Why the library didn't open, or why the last reset failed.
    let error: LoadError?
    let onReset: () -> Void
    let onCancel: () -> Void

    @State
    private var confirming = false

    var body: some View {
        switch prompt {
        case .offered: offered
        case .unfinished: unfinished
        }
    }

    private var offered: some View {
        offeredNotice
            .padding()
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .alert("Reset this device?", isPresented: $confirming) {
                Button(
                    localizedCoreString("core.sync.reset_device.confirm"),
                    role: .destructive,
                    action: onReset
                )
                Button("Cancel", role: .cancel) {}
            } message: {
                Text(localizedCoreString("core.sync.reset_device.warning"))
            }
    }

    /// Why a reset is offered: the error opening the library found, or with
    /// none, as from Settings, the reset's own line. A failed reset is titled
    /// by what failed instead, its error behind Details.
    @ViewBuilder
    private var offeredNotice: some View {
        let cancel = ErrorAction(
            "Cancel",
            shortcut: .cancelAction,
            perform: onCancel
        )
        let offer = ErrorAction("Reset this device…") { confirming = true }
        if let error, error.step == .deviceReset {
            Notice(
                title: error.step.title,
                advice: nil,
                error: error.error,
                look: .centered,
                action: error.error.retryAction(perform: onReset),
                secondary: cancel
            )
        }
        else if let error {
            Notice(
                error: error.error,
                look: .centered,
                action: offer,
                secondary: cancel
            )
        }
        else {
            Notice(
                title: BridgeErrorCategory.deviceResetRequired.localizedLine,
                advice: nil,
                error: nil,
                look: .centered,
                action: offer,
                secondary: cancel
            )
        }
    }

    private var unfinished: some View {
        VStack(spacing: ThemeSpace.edge) {
            Spacer()
            Text("Finish resetting this device?")
                .themeText(.title)
            Text(
                BridgeErrorCategory.replacementUnfinished(by: .reset)
                    .localizedLine
            )
            .themeText(.body)
            .foregroundStyle(.secondary)
            .multilineTextAlignment(.center)
            .frame(maxWidth: WelcomeLayout.columnWidth)
            if let error {
                Notice(
                    title: error.step.title,
                    advice: error.step.advice,
                    error: error.error
                )
                .frame(maxWidth: WelcomeLayout.columnWidth)
            }
            HStack(spacing: ThemeSpace.group) {
                Button("Cancel", action: onCancel)
                    .buttonStyle(.bordered)
                    .keyboardShortcut(.cancelAction)
                Button("Finish resetting", role: .destructive, action: onReset)
                    .buttonStyle(.bordered)
            }
            Spacer()
        }
        .padding()
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

#if DEBUG
    #Preview("Reset required") {
        PreviewScenes.deviceReset(
            error: LoadError(.resetOffered, PreviewData.resetRequiredError)
        )
    }

    #Preview("Reset failed") {
        PreviewScenes.deviceReset(
            error: LoadError(.deviceReset, PreviewData.displayErrorWithDetail)
        )
    }

    #Preview("Reset unfinished") {
        WelcomeWindowChrome {
            DeviceResetView(
                prompt: .unfinished,
                error: nil,
                onReset: {},
                onCancel: {}
            )
        }
    }
#endif
