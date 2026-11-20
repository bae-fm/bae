import BaeKit
import SwiftUI

/// The Play button of a source in the import, and the Stop button that
/// replaces it while the second player plays that source. With `hitSize`, the
/// button is a square of that size.
struct MacAuditionButton: View {
    let isPlaying: Bool
    var hitSize: CGFloat?
    let action: () -> Void

    var body: some View {
        let label = localizedCoreString(
            isPlaying ? "ui.import.slots.stop" : "ui.import.slots.play"
        )
        Button(action: action) {
            Image(systemName: isPlaying ? "stop.fill" : "play.fill")
                .themeIcon(.small)
                .foregroundStyle(
                    isPlaying
                        ? AnyShapeStyle(Theme.accent)
                        : AnyShapeStyle(.secondary)
                )
                .frame(width: hitSize, height: hitSize)
                .contentShape(Rectangle())
        }
        .buttonStyle(.accentPlain)
        .help(label)
        .accessibilityLabel(label)
    }
}
