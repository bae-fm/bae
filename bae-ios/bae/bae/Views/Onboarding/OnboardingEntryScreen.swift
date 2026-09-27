import BaeKit
import SwiftUI

/// The first-run chooser: join a library from another device, or restore from a
/// recovery code by scan or paste.
struct OnboardingEntryScreen: View {
    let error: String?
    let onJoin: () -> Void
    let onScanRecovery: () -> Void
    let onPasteRecovery: () -> Void

    private static let buttonWidth: CGFloat = 240

    var body: some View {
        OnboardingScreen {
            Image(systemName: "music.note.house.fill")
                .themeIcon(.hero)
                .foregroundStyle(Theme.accent)
            Text("bae")
                .themeText(.wordmark)
            OnboardingSecondaryText(
                "Add this device to a library you already have on another device."
            )

            VStack(spacing: ThemeSpace.group) {
                Button(action: onJoin) {
                    Text("Join a library")
                        .frame(maxWidth: Self.buttonWidth)
                }
                .buttonStyle(PrimaryButtonStyle())

                Button(action: onScanRecovery) {
                    Text("Scan recovery code")
                        .frame(maxWidth: Self.buttonWidth)
                }
                .buttonStyle(.bordered)

                Button(action: onPasteRecovery) {
                    Text("Paste recovery code")
                        .frame(maxWidth: Self.buttonWidth)
                }
                .buttonStyle(.bordered)
            }
            .padding(.top, ThemeSpace.edge)

            if let error {
                Text(error)
                    .themeText(.body)
                    .foregroundStyle(Theme.danger)
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
