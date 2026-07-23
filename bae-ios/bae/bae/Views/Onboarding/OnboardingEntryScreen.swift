import BaeKit
import SwiftUI

/// The first-run chooser: join a library from another device, or restore from a
/// recovery code by scan or paste. The actions are the owner's — this screen
/// only lays out the buttons and shows the current error.
struct OnboardingEntryScreen: View {
    let error: String?
    let onJoin: () -> Void
    let onScanRecovery: () -> Void
    let onPasteRecovery: () -> Void

    var body: some View {
        OnboardingScreen {
            Image(systemName: "music.note.house.fill")
                .font(.system(size: 72))
                .foregroundStyle(Theme.accent)
            Text("bae")
                .font(.system(size: 48, weight: .bold))
            OnboardingSecondaryText(
                "Add this device to a library you already have on another device."
            )

            VStack(spacing: 12) {
                Button(action: onJoin) {
                    Text("Join a library")
                        .frame(maxWidth: 240)
                }
                .buttonStyle(.borderedProminent)

                Button(action: onScanRecovery) {
                    Text("Scan recovery code")
                        .frame(maxWidth: 240)
                }
                .buttonStyle(.bordered)

                Button(action: onPasteRecovery) {
                    Text("Paste recovery code")
                        .frame(maxWidth: 240)
                }
                .buttonStyle(.bordered)
            }
            .padding(.top, 16)

            if let error {
                Text(error)
                    .font(.callout)
                    .foregroundStyle(.red)
                    .multilineTextAlignment(.center)
                    .frame(maxWidth: 320)
            }
        }
    }
}

#if DEBUG
#Preview {
    PreviewScenes.welcome()
}
#endif
