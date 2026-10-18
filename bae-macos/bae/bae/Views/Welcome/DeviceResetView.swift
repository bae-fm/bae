import BaeKit
import SwiftUI

/// Offered when opening a library found a damaged database or another copy of
/// this device: the warning first, then the reset, which loads the library
/// from storage again under a fresh device id.
struct DeviceResetView: View {
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
            Text("Reset this device?")
                .themeText(.title)
            Text(localizedCoreString("core.sync.reset_device.warning"))
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
                Button(
                    localizedCoreString("core.pending.action.reset_device"),
                    role: .destructive,
                    action: onReset
                )
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
        WelcomeWindowChrome {
            DeviceResetView(
                error: PreviewData.displayErrorWithDetail,
                onReset: {},
                onCancel: {}
            )
        }
    }
#endif
