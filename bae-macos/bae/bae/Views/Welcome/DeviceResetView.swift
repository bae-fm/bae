import BaeKit
import SwiftUI

/// Offered when opening a library found a damaged database or another copy of
/// this device: the warning first, then the reset, which loads the library
/// from storage again under a fresh device id. Also finishes a reset that
/// stopped partway, which only resetting again finishes.
struct DeviceResetView: View {
    /// Why the screen asks for a reset.
    enum Prompt {
        /// This device's copy can't be used; resetting discards its unsynced
        /// edits, which the warning says before the person chooses.
        case offered
        /// A reset already under way stopped partway; resetting again
        /// finishes it.
        case unfinished
    }

    let prompt: Prompt
    /// Why the library didn't open, or why the last reset failed.
    let error: DisplayError?
    let onReset: () -> Void
    let onCancel: () -> Void

    var body: some View {
        VStack(spacing: ThemeSpace.edge) {
            Spacer()
            Image(systemName: "arrow.counterclockwise.circle")
                .themeIcon(.hero)
                .foregroundStyle(.secondary)
            Text(title)
                .themeText(.title)
            Text(message)
                .themeText(.body)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .frame(maxWidth: WelcomeLayout.columnWidth)
            if let error {
                ErrorDetailDisclosure(error: error)
                    .frame(maxWidth: WelcomeLayout.columnWidth)
            }
            HStack(spacing: ThemeSpace.group) {
                Button("Cancel", action: onCancel)
                    .buttonStyle(.bordered)
                    .keyboardShortcut(.cancelAction)
                Button(confirmTitle, role: .destructive, action: onReset)
                    .buttonStyle(.bordered)
            }
            Spacer()
        }
        .padding()
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private var title: LocalizedStringKey {
        switch prompt {
        case .offered: "Reset this device?"
        case .unfinished: "Finish resetting this device?"
        }
    }

    private var message: String {
        switch prompt {
        case .offered:
            localizedCoreString("core.sync.reset_device.warning")
        case .unfinished:
            BridgeErrorCategory.replacementUnfinished(by: .reset)
                .localizedLine
        }
    }

    private var confirmTitle: String {
        switch prompt {
        case .offered:
            localizedCoreString("core.sync.reset_device.confirm")
        case .unfinished:
            String(localized: "Finish Resetting")
        }
    }
}

#if DEBUG
    #Preview("Reset required") {
        WelcomeWindowChrome {
            DeviceResetView(
                prompt: .offered,
                error: PreviewData.displayErrorWithDetail,
                onReset: {},
                onCancel: {}
            )
        }
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
