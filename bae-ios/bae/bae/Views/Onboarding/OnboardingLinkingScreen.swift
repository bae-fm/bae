import SwiftUI

/// The connecting screen shown while a restore or join runs: a spinner and a
/// cancel button that aborts the in-flight link.
struct OnboardingLinkingScreen: View {
    let onCancel: () -> Void

    var body: some View {
        OnboardingScreen {
            ProgressView()
                .controlSize(.large)
            Text("Connecting to your library")
                .font(.headline)
                .multilineTextAlignment(.center)
            OnboardingSecondaryText("bae is setting up the library on this device.")
            Button("Cancel") {
                onCancel()
            }
            .buttonStyle(.bordered)
            .padding(.top, 8)
        }
    }
}
