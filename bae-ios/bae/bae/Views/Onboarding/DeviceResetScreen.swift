import BaeKit
import SwiftUI

/// Offered when opening a library found a damaged database or another copy of
/// this device: the warning first, then the reset, which loads the library
/// from storage again under a fresh device id.
struct DeviceResetScreen: View {
    /// Why the library didn't open, or why the last reset failed.
    let error: DisplayError?
    let onReset: () -> Void
    let onCancel: () -> Void

    var body: some View {
        OnboardingScreen {
            Image(systemName: "arrow.counterclockwise.circle")
                .themeIcon(.hero)
                .foregroundStyle(.secondary)
            Text("Reset this device?")
                .themeText(.title)
            Text(localizedCoreString("core.sync.reset_device.warning"))
                .themeText(.body)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .frame(maxWidth: 320)
            if let error {
                ErrorDetailDisclosure(error: error)
                    .frame(maxWidth: 320)
            }
            HStack(spacing: ThemeSpace.group) {
                Button("Cancel", action: onCancel)
                    .buttonStyle(.bordered)
                Button(
                    localizedCoreString("core.pending.action.reset_device"),
                    role: .destructive,
                    action: onReset
                )
                .buttonStyle(.bordered)
            }
        }
    }
}

#if DEBUG
#Preview {
    DeviceResetScreen(error: nil, onReset: {}, onCancel: {})
}
#endif
