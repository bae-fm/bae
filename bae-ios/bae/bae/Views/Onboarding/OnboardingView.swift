import BaeKit
import SwiftUI

/// First-run onboarding. Two ways to put a library on this device: add this
/// device to your own library with its recovery code — scanned from Add
/// Device on another of your devices, typed, or one coven saved in iCloud
/// Keychain — or join someone's library with the invitation an admin sent
/// you.
struct OnboardingView: View {
    let setup: LibrarySetup
    /// The restored or joined library's open store, with the error a partly
    /// finished restore or join reported.
    let onLibraryReady: @MainActor (OpenedLibrary, BridgeError?) -> Void

    private enum Mode {
        case entry
        case code(BootstrapCodeForm)
    }

    @State
    private var mode: Mode = .entry

    var body: some View {
        switch mode {
        case .entry:
            OnboardingEntryScreen(
                setup: setup,
                onJoin: { show(.join) },
                onAddDevice: { show(.restore(nil)) },
                onRestoreSaved: { show(.restore($0.code)) }
            )
        case .code(let form):
            BootstrapCodeScreen(form: form, onBack: { mode = .entry })
        }
    }

    private func show(_ entry: BootstrapCodeForm.Entry) {
        mode = .code(
            setup.bootstrapForm(entry: entry, onLibraryReady: onLibraryReady)
        )
    }
}

#if DEBUG
#Preview {
    OnboardingView(setup: .stub(), onLibraryReady: { _, _ in })
        .environment(PreviewData.removals())
}
#endif
