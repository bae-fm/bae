import BaeKit
import SwiftUI

/// The connecting screen shown while a restore or join runs: a spinner and a
/// cancel button that aborts the in-flight link.
struct OnboardingLinkingScreen: View {
    enum Context: Equatable {
        case librarySetup
        case devicePairing(fingerprint: String?)
    }

    let context: Context
    let joinProgress: BridgeJoiningDeviceJoinProgress?
    let onCancel: () -> Void

    var body: some View {
        OnboardingScreen {
            if let joinProgress {
                DeviceJoinProgressView(joining: joinProgress)
            }
            else {
                ProgressView()
                    .controlSize(.large)
                Text(title)
                    .themeText(.heading)
                    .multilineTextAlignment(.center)
            }
            if case .devicePairing(let fingerprint) = context,
                let fingerprint
            {
                LabeledContent("This device", value: fingerprint)
                    .themeText(.mono)
            }
            else {
                OnboardingSecondaryText(
                    "bae is setting up the library on this device."
                )
            }
            Button("Cancel") {
                onCancel()
            }
            .buttonStyle(.bordered)
            .padding(.top, ThemeSpace.related)
        }
    }

    private var title: LocalizedStringKey {
        switch context {
        case .librarySetup:
            "Connecting to your library"
        case .devicePairing(fingerprint: nil):
            "Starting pairing..."
        case .devicePairing:
            "Waiting for approval..."
        }
    }
}

#if DEBUG
#Preview {
    OnboardingLinkingScreen(
        context: .librarySetup,
        joinProgress: nil,
        onCancel: {}
    )
}
#endif
